// ============================================================================
// Pro-Rata Matching Algorithm
// Used in derivatives exchanges (CME, Eurex)
// ============================================================================

use crate::domain::{Order, OrderBookLevel, OrderBookSide, OrderId, Trade};
use crate::interfaces::MatchingAlgorithm;
use rust_decimal::Decimal;
use std::sync::Arc;

/// Pro-Rata matching algorithm
///
/// Allocates fills proportionally based on order size at each price level.
/// Commonly used in futures and derivatives markets.
///
/// # Example
/// ```text
/// Book at 50000:
///   Order A: 100 BTC
///   Order B: 200 BTC
///   Order C:  50 BTC (below minimum, excluded)
///   Total: 300 BTC (eligible)
///
/// Incoming: Sell 150 BTC @ 50000
/// Allocation:
///   A gets: 150 * (100/300) = 50 BTC
///   B gets: 150 * (200/300) = 100 BTC
/// ```
pub struct ProRata {
    /// Minimum order size to participate in pro-rata allocation
    pub minimum_quantity: Decimal,
    /// Whether to give FIFO priority to the top order
    pub top_of_book_fifo: bool,
}

impl ProRata {
    pub fn new(minimum_quantity: Decimal, top_of_book_fifo: bool) -> Self {
        Self {
            minimum_quantity,
            top_of_book_fifo,
        }
    }

    /// Calculate pro-rata allocation for orders at a price level
    fn calculate_allocation(
        &self,
        level: &OrderBookLevel,
        quantity_to_fill: Decimal,
    ) -> Vec<(OrderId, Decimal)> {
        let mut allocations = Vec::new();
        let mut eligible_quantity = Decimal::ZERO;

        // Collect eligible orders (above minimum size)
        let mut eligible_orders = Vec::new();
        let mut ineligible_orders = Vec::new();

        // Drain orders to inspect them
        while let Some(order) = level.orders.pop() {
            let remaining = order.get_remaining_quantity();
            if remaining >= self.minimum_quantity {
                eligible_quantity += remaining;
                eligible_orders.push((order.id, remaining, order));
            } else {
                // Save ineligible orders to put back later
                ineligible_orders.push(order);
            }
        }

        // Put back ineligible orders
        for order in ineligible_orders {
            level.orders.push(order);
        }

        if eligible_quantity == Decimal::ZERO {
            return allocations;
        }

        // Calculate pro-rata allocations
        let mut total_allocated = Decimal::ZERO;

        for (order_id, order_quantity, order) in eligible_orders.iter() {
            let allocation = (order_quantity / eligible_quantity) * quantity_to_fill;
            let allocation = allocation.floor(); // Round down to avoid over-allocation

            allocations.push((*order_id, allocation));
            total_allocated += allocation;

            // Put order back for later use
            level.orders.push(Arc::clone(order));
        }

        // Handle remainder with FIFO (allocate to first order)
        let remainder = quantity_to_fill - total_allocated;
        if remainder > Decimal::ZERO && !allocations.is_empty() {
            allocations[0].1 += remainder;
        }

        allocations
    }
}

impl MatchingAlgorithm for ProRata {
    fn match_order(&self, incoming_order: Arc<Order>, opposite_side: &OrderBookSide) -> Vec<Trade> {
        let mut trades = Vec::new();

        while incoming_order.get_remaining_quantity() > Decimal::ZERO {
            let best_level = match opposite_side.best_level() {
                Some(level) => level,
                None => break,
            };

            if !self.prices_cross(&incoming_order, best_level.price) {
                break;
            }

            let remaining_to_fill = incoming_order.get_remaining_quantity();

            // Calculate pro-rata allocation
            let allocations = self.calculate_allocation(&best_level, remaining_to_fill);

            if allocations.is_empty() {
                break;
            }

            // Execute allocations
            for (order_id, allocated_qty) in allocations {
                if allocated_qty <= Decimal::ZERO {
                    continue;
                }

                // Find the order in the level
                let mut found_order: Option<Arc<Order>> = None;
                while let Some(order) = best_level.orders.pop() {
                    if order.id == order_id {
                        found_order = Some(order);
                        break;
                    } else {
                        // Put back other orders
                        best_level.orders.push(order);
                    }
                }

                if let Some(maker_order) = found_order {
                    let trade_quantity = allocated_qty.min(maker_order.get_remaining_quantity());

                    if trade_quantity > Decimal::ZERO
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
                        if maker_order.get_remaining_quantity() > Decimal::ZERO {
                            best_level.orders.push(maker_order);
                        }
                    }
                }

                if incoming_order.get_remaining_quantity() == Decimal::ZERO {
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
        "ProRata"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, Side, TimeInForce};

    #[test]
    fn test_pro_rata_allocation() {
        let algo = ProRata::new(Decimal::ZERO, false);
        let side = OrderBookSide::new(Side::Sell);

        // Add two sell orders at same price with different sizes
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(10), // 10 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(20), // 20 BTC
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());

        // Buy 15 BTC - should allocate pro-rata
        let buy = Arc::new(Order::new(
            "user3".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(15),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Should generate trades
        assert!(!trades.is_empty());

        // Total filled should be 15
        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(15));
    }

    #[test]
    fn test_pro_rata_minimum_quantity() {
        let algo = ProRata::new(Decimal::from(5), false);
        let side = OrderBookSide::new(Side::Sell);

        // Small order (below minimum)
        let sell_small = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(1), // Below minimum
            TimeInForce::GoodTillCancel,
        ));

        // Large order (above minimum)
        let sell_large = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(10),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_small);
        side.add_order(sell_large.clone());

        let buy = Arc::new(Order::new(
            "user3".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(5),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Should only match with large order
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].maker_order_id, sell_large.id);
    }
}
