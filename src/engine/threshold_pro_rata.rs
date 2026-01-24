// ============================================================================
// Threshold Pro-Rata Matching Algorithm
// Used by various derivatives exchanges to protect smaller orders
// ============================================================================

use crate::domain::{Order, OrderBookLevel, OrderBookSide, OrderId, Trade};
use crate::interfaces::MatchingAlgorithm;
use crate::numeric::Quantity;
use std::sync::Arc;

/// Threshold Pro-Rata matching algorithm
///
/// Orders are treated differently based on a size threshold:
/// - Orders BELOW the threshold: Get FIFO treatment (time priority)
/// - Orders ABOVE the threshold: Get pro-rata allocation (size-based)
///
/// This protects smaller retail traders from being disadvantaged by large orders
/// while still providing size-based allocation for institutional participants.
pub struct ThresholdProRata {
    /// Size threshold - orders below this get FIFO, above get pro-rata
    pub threshold: Quantity,

    /// Minimum order size to participate in pro-rata allocation
    pub minimum_quantity: Quantity,
}

impl ThresholdProRata {
    pub fn new(threshold: Quantity, minimum_quantity: Quantity) -> Self {
        Self {
            threshold,
            minimum_quantity,
        }
    }

    /// Calculate allocation for a price level with threshold-based logic
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

        // Put all orders back (maintaining order)
        for order in all_orders.iter().rev() {
            level.orders.push(Arc::clone(order));
        }

        let mut remaining_to_allocate = quantity_to_fill;

        // Separate orders into categories
        let mut small_orders = Vec::new();
        let mut large_orders = Vec::new();
        let mut large_total_quantity = Quantity::ZERO;

        for order in all_orders.iter() {
            let remaining = order.get_remaining_quantity();

            if remaining < self.threshold {
                // Small orders get FIFO treatment
                small_orders.push((order.id, remaining));
            } else {
                // Large orders get pro-rata
                if remaining >= self.minimum_quantity {
                    large_total_quantity = large_total_quantity + remaining;
                    large_orders.push((order.id, remaining));
                }
            }
        }

        // Step 1: Allocate to small orders in FIFO order
        for (order_id, order_quantity) in small_orders {
            if remaining_to_allocate <= Quantity::ZERO {
                break;
            }

            let allocation = remaining_to_allocate.min(order_quantity);
            allocations.push((order_id, allocation));
            remaining_to_allocate = remaining_to_allocate - allocation;
        }

        // Step 2: Allocate to large orders pro-rata
        if remaining_to_allocate > Quantity::ZERO && large_total_quantity > Quantity::ZERO {
            let mut prorata_allocated = Quantity::ZERO;
            let large_total_raw = large_total_quantity.raw_value();
            let remaining_raw = remaining_to_allocate.raw_value();

            for (order_id, order_quantity) in large_orders.iter() {
                let order_raw = order_quantity.raw_value();
                let allocation_raw = ((order_raw as i128 * remaining_raw as i128) / large_total_raw as i128) as i64;
                let allocation = Quantity::from_raw(allocation_raw);

                allocations.push((*order_id, allocation));
                prorata_allocated = prorata_allocated + allocation;
            }

            // Handle remainder - give to first large order
            let remainder = remaining_to_allocate - prorata_allocated;
            if remainder > Quantity::ZERO && !large_orders.is_empty() {
                if let Some(first_large) = allocations.iter_mut().find(|(id, _)| {
                    large_orders.iter().any(|(large_id, _)| large_id == id)
                }) {
                    first_large.1 = first_large.1 + remainder;
                }
            }
        }

        allocations
    }
}

impl MatchingAlgorithm for ThresholdProRata {
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

            // Calculate allocation (FIFO for small, pro-rata for large)
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
        "Threshold-ProRata"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, Side, TimeInForce};
    use crate::numeric::Price;

    #[test]
    fn test_threshold_small_orders_fifo() {
        // Threshold: 50 BTC
        let algo = ThresholdProRata::new(Quantity::from_integer(50).unwrap(), Quantity::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        // Add small orders (all below threshold)
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(10).unwrap(), // Below threshold
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(20).unwrap(), // Below threshold
            TimeInForce::GoodTillCancel,
        ));

        let sell3 = Arc::new(Order::new(
            "user3".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(30).unwrap(), // Below threshold
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());
        side.add_order(sell3.clone());

        // Buy 40 BTC - should fill sell1 and sell2 in FIFO order
        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(40).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Total should be 40 BTC
        let total_filled: Quantity = trades.iter().map(|t| t.quantity).fold(Quantity::ZERO, |a, b| a + b);
        assert_eq!(total_filled, Quantity::from_integer(40).unwrap());

        // Verify FIFO order
        assert_eq!(trades[0].maker_order_id, sell1.id);
        assert_eq!(trades[0].quantity, Quantity::from_integer(10).unwrap());
        assert_eq!(trades[1].maker_order_id, sell2.id);
        assert_eq!(trades[1].quantity, Quantity::from_integer(20).unwrap());
        assert_eq!(trades[2].maker_order_id, sell3.id);
        assert_eq!(trades[2].quantity, Quantity::from_integer(10).unwrap());
    }

    #[test]
    fn test_threshold_large_orders_prorata() {
        // Threshold: 50 BTC
        let algo = ThresholdProRata::new(Quantity::from_integer(50).unwrap(), Quantity::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        // Add large orders (all above threshold)
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(), // Above threshold
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(200).unwrap(), // Above threshold
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());

        // Buy 150 BTC - should allocate pro-rata
        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(150).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        let total_filled: Quantity = trades.iter().map(|t| t.quantity).fold(Quantity::ZERO, |a, b| a + b);
        assert_eq!(total_filled, Quantity::from_integer(150).unwrap());

        // Pro-rata: sell1 gets 150 * (100/300) = 50, sell2 gets 150 * (200/300) = 100
        let sell1_filled: Quantity = trades.iter()
            .filter(|t| t.maker_order_id == sell1.id)
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);

        let sell2_filled: Quantity = trades.iter()
            .filter(|t| t.maker_order_id == sell2.id)
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);

        assert_eq!(sell1_filled, Quantity::from_integer(50).unwrap());
        assert_eq!(sell2_filled, Quantity::from_integer(100).unwrap());
    }

    #[test]
    fn test_threshold_empty_book() {
        let algo = ThresholdProRata::new(Quantity::from_integer(50).unwrap(), Quantity::ZERO);
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
        assert!(trades.is_empty(), "No trades with empty book");
    }
}
