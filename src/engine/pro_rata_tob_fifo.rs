// ============================================================================
// Pro-Rata with Top-of-Book FIFO Matching Algorithm
// Used by Eurex, ICE Futures, and other major derivatives exchanges
// ============================================================================

use crate::domain::{Order, OrderBookLevel, OrderBookSide, OrderId, Trade};
use crate::interfaces::MatchingAlgorithm;
use crate::numeric::Quantity;
use std::sync::Arc;

/// Pro-Rata with Top-of-Book FIFO matching algorithm
///
/// The first order at each price level gets filled completely (FIFO priority),
/// then remaining quantity is distributed pro-rata among other orders.
/// This hybrid approach rewards both queue position and order size.
///
/// # Example
/// ```text
/// Book at 50000:
///   Order A: 10 BTC  (first in queue)
///   Order B: 100 BTC
///   Order C: 200 BTC
///   Order D: 50 BTC (below minimum, excluded from pro-rata)
///
/// Incoming: Sell 150 BTC @ 50000
///
/// Step 1 - FIFO: Order A gets filled completely: 10 BTC
/// Remaining: 150 - 10 = 140 BTC
///
/// Step 2 - Pro-Rata among B and C (total: 300 BTC eligible):
///   B gets: 140 * (100/300) = 46 BTC
///   C gets: 140 * (200/300) = 93 BTC
///   D excluded (below minimum)
///
/// Final allocation:
///   A: 10 BTC (FIFO)
///   B: 46 BTC (pro-rata)
///   C: 93 BTC (pro-rata) + 1 BTC (remainder)
/// ```
pub struct ProRataTobFifo {
    /// Minimum order size to participate in pro-rata allocation
    pub minimum_quantity: Quantity,
}

impl ProRataTobFifo {
    pub fn new(minimum_quantity: Quantity) -> Self {
        Self { minimum_quantity }
    }

    /// Calculate allocation for a price level:
    /// 1. First order gets FIFO priority (filled completely)
    /// 2. Remaining orders get pro-rata allocation
    fn calculate_allocation(
        &self,
        level: &OrderBookLevel,
        quantity_to_fill: Quantity,
    ) -> Vec<(OrderId, Quantity)> {
        let mut allocations = Vec::new();

        // Collect all orders from the level
        let mut all_orders = Vec::new();
        while let Some(order) = level.orders.pop() {
            all_orders.push(order);
        }

        if all_orders.is_empty() {
            return allocations;
        }

        // Put all orders back first (maintaining order)
        for order in all_orders.iter().rev() {
            level.orders.push(Arc::clone(order));
        }

        let mut remaining_to_allocate = quantity_to_fill;

        // Step 1: Give FIFO priority to the first order (top of book)
        let first_order = &all_orders[0];
        let first_order_qty = first_order.get_remaining_quantity();

        if first_order_qty > Quantity::ZERO {
            let fifo_allocation = remaining_to_allocate.min(first_order_qty);
            allocations.push((first_order.id, fifo_allocation));
            remaining_to_allocate = remaining_to_allocate - fifo_allocation;
        }

        // If everything was allocated to first order, we're done
        if remaining_to_allocate <= Quantity::ZERO || all_orders.len() == 1 {
            return allocations;
        }

        // Step 2: Pro-rata allocation for remaining orders (excluding first)
        let mut eligible_quantity = Quantity::ZERO;
        let mut eligible_orders = Vec::new();

        for order in all_orders.iter().skip(1) {
            let remaining = order.get_remaining_quantity();
            if remaining >= self.minimum_quantity {
                eligible_quantity = eligible_quantity + remaining;
                eligible_orders.push((order.id, remaining));
            }
        }

        if eligible_quantity == Quantity::ZERO {
            return allocations;
        }

        // Calculate pro-rata allocations
        let mut total_allocated = Quantity::ZERO;
        let eligible_raw = eligible_quantity.raw_value();
        let remaining_raw = remaining_to_allocate.raw_value();

        for (order_id, order_quantity) in eligible_orders.iter() {
            let order_raw = order_quantity.raw_value();
            let allocation_raw = ((order_raw as i128 * remaining_raw as i128) / eligible_raw as i128) as i64;
            let allocation = Quantity::from_raw(allocation_raw);

            allocations.push((*order_id, allocation));
            total_allocated = total_allocated + allocation;
        }

        // Handle remainder - give to first eligible pro-rata order
        let remainder = remaining_to_allocate - total_allocated;
        if remainder > Quantity::ZERO && allocations.len() > 1 {
            // Find first pro-rata allocation (skip the FIFO one at index 0)
            allocations[1].1 = allocations[1].1 + remainder;
        }

        allocations
    }
}

impl MatchingAlgorithm for ProRataTobFifo {
    fn match_order(&self, incoming_order: Arc<Order>, opposite_side: &OrderBookSide) -> Vec<Trade> {
        let mut trades = Vec::new();

        while incoming_order.get_remaining_quantity() > Quantity::ZERO {
            let best_level = match opposite_side.best_level() {
                Some(level) => level,
                None => break,
            };

            if !self.prices_cross(&incoming_order, best_level.price) {
                break;
            }

            let remaining_to_fill = incoming_order.get_remaining_quantity();

            // Calculate allocation (FIFO for first, pro-rata for rest)
            let allocations = self.calculate_allocation(&best_level, remaining_to_fill);

            if allocations.is_empty() {
                break;
            }

            // Execute allocations
            for (order_id, allocated_qty) in allocations {
                if allocated_qty <= Quantity::ZERO {
                    continue;
                }

                // Find the order in the level
                let mut found_order: Option<Arc<Order>> = None;
                let mut temp_orders = Vec::new();

                while let Some(order) = best_level.orders.pop() {
                    if order.id == order_id {
                        found_order = Some(order);
                        break;
                    } else {
                        temp_orders.push(order);
                    }
                }

                // Put back orders we didn't match
                for order in temp_orders.into_iter().rev() {
                    best_level.orders.push(order);
                }

                if let Some(maker_order) = found_order {
                    let trade_quantity = allocated_qty.min(maker_order.get_remaining_quantity());

                    if trade_quantity > Quantity::ZERO
                        && maker_order.try_fill(trade_quantity)
                        && incoming_order.try_fill(trade_quantity)
                    {
                        let trade = Trade::new(
                            (*incoming_order.instrument).clone(),
                            maker_order.id,
                            incoming_order.id,
                            maker_order.price.unwrap(),
                            trade_quantity,
                        );

                        best_level.subtract_quantity(trade_quantity);
                        trades.push(trade);

                        // Put maker order back if not fully filled
                        if maker_order.get_remaining_quantity() > Quantity::ZERO {
                            best_level.orders.push(maker_order);
                        }
                    }
                }

                if incoming_order.get_remaining_quantity() == Quantity::ZERO {
                    break;
                }
            }

            // Clean up empty levels
            if best_level.is_empty() {
                opposite_side.remove_empty_levels();
            }

            // Prevent infinite loop
            if incoming_order.get_remaining_quantity() == remaining_to_fill {
                break;
            }
        }

        trades
    }

    fn name(&self) -> &str {
        "ProRata-TOB-FIFO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, Side, TimeInForce};
    use crate::numeric::Price;

    #[test]
    fn test_tob_fifo_first_order_gets_priority() {
        let algo = ProRataTobFifo::new(Quantity::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        // Add three sell orders at same price
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(10).unwrap(), // First order: 10 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(), // Second order: 100 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell3 = Arc::new(Order::new(
            "user3".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(200).unwrap(), // Third order: 200 BTC
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());
        side.add_order(sell3.clone());

        // Buy 150 BTC - first gets 10, then 140 pro-rata between sell2 and sell3
        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(150).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy.clone(), &side);

        // Verify total filled
        let total_filled: Quantity = trades.iter().map(|t| t.quantity).fold(Quantity::ZERO, |a, b| a + b);
        assert_eq!(total_filled, Quantity::from_integer(150).unwrap());

        // Find trade with first order
        let first_trade = trades.iter().find(|t| t.maker_order_id == sell1.id);
        assert!(first_trade.is_some(), "First order should have a trade");

        // First order should be filled completely (10 BTC)
        let first_filled = first_trade.unwrap().quantity;
        assert_eq!(first_filled, Quantity::from_integer(10).unwrap(), "First order should get full 10 BTC via FIFO");

        // Verify incoming order is fully filled
        assert_eq!(buy.get_remaining_quantity(), Quantity::ZERO);
    }

    #[test]
    fn test_tob_fifo_first_order_partial_fill() {
        let algo = ProRataTobFifo::new(Quantity::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        // First order is larger than incoming
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());

        // Buy only 50 BTC - should all go to first order
        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(50).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Should only match with first order
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].maker_order_id, sell1.id);
        assert_eq!(trades[0].quantity, Quantity::from_integer(50).unwrap());

        // Second order should not be touched
        assert_eq!(sell2.get_remaining_quantity(), Quantity::from_integer(100).unwrap());
    }

    #[test]
    fn test_tob_fifo_with_minimum_quantity() {
        let algo = ProRataTobFifo::new(Quantity::from_integer(50).unwrap()); // 50 BTC minimum
        let side = OrderBookSide::new(Side::Sell);

        // Add orders
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(10).unwrap(), // First order gets FIFO regardless of size
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(30).unwrap(), // Below minimum, excluded from pro-rata
            TimeInForce::GoodTillCancel,
        ));

        let sell3 = Arc::new(Order::new(
            "user3".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(), // Above minimum, participates in pro-rata
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());
        side.add_order(sell3.clone());

        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // First order should get filled (FIFO): 10 BTC
        let first_trade = trades.iter().find(|t| t.maker_order_id == sell1.id);
        assert!(first_trade.is_some());
        assert_eq!(first_trade.unwrap().quantity, Quantity::from_integer(10).unwrap());

        // sell2 should NOT participate (below minimum)
        let second_trade = trades.iter().find(|t| t.maker_order_id == sell2.id);
        assert!(second_trade.is_none(), "Order below minimum should not participate in pro-rata");

        // sell3 should get the remaining (pro-rata, but it's the only eligible one)
        let third_trade = trades.iter().find(|t| t.maker_order_id == sell3.id);
        assert!(third_trade.is_some());
        assert_eq!(third_trade.unwrap().quantity, Quantity::from_integer(90).unwrap());
    }

    #[test]
    fn test_tob_fifo_empty_book() {
        let algo = ProRataTobFifo::new(Quantity::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        assert!(trades.is_empty(), "No trades should occur with empty book");
    }
}
