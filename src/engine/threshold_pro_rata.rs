// ============================================================================
// Threshold Pro-Rata Matching Algorithm
// Used by various derivatives exchanges to protect smaller orders
// ============================================================================

use crate::domain::{Order, OrderBookLevel, OrderBookSide, OrderId, Trade};
use crate::interfaces::MatchingAlgorithm;
use rust_decimal::Decimal;
use std::sync::Arc;

/// Threshold Pro-Rata matching algorithm
///
/// Orders are treated differently based on a size threshold:
/// - Orders BELOW the threshold: Get FIFO treatment (time priority)
/// - Orders ABOVE the threshold: Get pro-rata allocation (size-based)
///
/// This protects smaller retail traders from being disadvantaged by large orders
/// while still providing size-based allocation for institutional participants.
///
/// # Example
/// ```text
/// Configuration:
///   - Threshold: 50 BTC
///   - Minimum quantity for pro-rata: 10 BTC
///
/// Book at 50000:
///   Order A: 20 BTC  (below threshold → FIFO)
///   Order B: 30 BTC  (below threshold → FIFO)
///   Order C: 100 BTC (above threshold → pro-rata)
///   Order D: 200 BTC (above threshold → pro-rata)
///   Order E: 5 BTC   (below minimum, excluded)
///
/// Incoming: Sell 200 BTC @ 50000
///
/// Step 1 - FIFO allocation for small orders (A, B):
///   A gets: 20 BTC (fully filled)
///   B gets: 30 BTC (fully filled)
///   Remaining: 200 - 50 = 150 BTC
///
/// Step 2 - Pro-rata for large orders (C, D):
///   Total large: 300 BTC
///   C gets: 150 * (100/300) = 50 BTC
///   D gets: 150 * (200/300) = 100 BTC
///
/// Final allocation:
///   A: 20 BTC (FIFO)
///   B: 30 BTC (FIFO)
///   C: 50 BTC (pro-rata)
///   D: 100 BTC (pro-rata)
/// ```
pub struct ThresholdProRata {
    /// Size threshold - orders below this get FIFO, above get pro-rata
    pub threshold: Decimal,

    /// Minimum order size to participate in pro-rata allocation
    pub minimum_quantity: Decimal,
}

impl ThresholdProRata {
    pub fn new(threshold: Decimal, minimum_quantity: Decimal) -> Self {
        Self {
            threshold,
            minimum_quantity,
        }
    }

    /// Calculate allocation for a price level with threshold-based logic
    fn calculate_allocation(
        &self,
        level: &OrderBookLevel,
        quantity_to_fill: Decimal,
    ) -> Vec<(OrderId, Decimal)> {
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

        // Separate orders into three categories:
        // 1. Small orders (below threshold) - ALL get FIFO treatment
        // 2. Large orders (>= threshold and >= minimum) - get pro-rata
        // 3. Large orders below minimum - excluded from pro-rata
        let mut small_orders = Vec::new();
        let mut large_orders = Vec::new();
        let mut large_total_quantity = Decimal::ZERO;

        for order in all_orders.iter() {
            let remaining = order.get_remaining_quantity();

            if remaining < self.threshold {
                // Small orders ALL get FIFO treatment (no minimum check)
                small_orders.push((order.id, remaining));
            } else {
                // Large orders (>= threshold)
                if remaining >= self.minimum_quantity {
                    // Above minimum: participate in pro-rata
                    large_total_quantity += remaining;
                    large_orders.push((order.id, remaining));
                }
                // Large orders below minimum are excluded
            }
        }

        // Step 1: Allocate to small orders in FIFO order
        for (order_id, order_quantity) in small_orders {
            if remaining_to_allocate <= Decimal::ZERO {
                break;
            }

            let allocation = remaining_to_allocate.min(order_quantity);
            allocations.push((order_id, allocation));
            remaining_to_allocate -= allocation;
        }

        // Step 2: Allocate to large orders pro-rata
        if remaining_to_allocate > Decimal::ZERO && large_total_quantity > Decimal::ZERO {
            let mut prorata_allocated = Decimal::ZERO;

            for (order_id, order_quantity) in large_orders.iter() {
                let allocation = (order_quantity / large_total_quantity) * remaining_to_allocate;
                let allocation = allocation.floor(); // Round down

                allocations.push((*order_id, allocation));
                prorata_allocated += allocation;
            }

            // Handle remainder - give to first large order
            let remainder = remaining_to_allocate - prorata_allocated;
            if remainder > Decimal::ZERO && !large_orders.is_empty() {
                // Find the first large order in allocations
                if let Some(first_large) = allocations.iter_mut().find(|(id, _)| {
                    large_orders.iter().any(|(large_id, _)| large_id == id)
                }) {
                    first_large.1 += remainder;
                }
            }
        }

        allocations
    }
}

impl MatchingAlgorithm for ThresholdProRata {
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

            // Calculate allocation (FIFO for small, pro-rata for large)
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
        "Threshold-ProRata"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, Side, TimeInForce};

    #[test]
    fn test_threshold_small_orders_fifo() {
        // Threshold: 50 BTC
        let algo = ThresholdProRata::new(Decimal::from(50), Decimal::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        // Add small orders (all below threshold)
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(10), // Below threshold
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(20), // Below threshold
            TimeInForce::GoodTillCancel,
        ));

        let sell3 = Arc::new(Order::new(
            "user3".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(30), // Below threshold
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
            Some(Decimal::from(50000)),
            Decimal::from(40),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Should match with sell1, sell2, and partially sell3 in FIFO order
        // sell1: 10, sell2: 20, sell3: 10 (partial) = 40 total
        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(40));

        // Verify FIFO order
        assert_eq!(trades[0].maker_order_id, sell1.id);
        assert_eq!(trades[0].quantity, Decimal::from(10));
        assert_eq!(trades[1].maker_order_id, sell2.id);
        assert_eq!(trades[1].quantity, Decimal::from(20));
        assert_eq!(trades[2].maker_order_id, sell3.id);
        assert_eq!(trades[2].quantity, Decimal::from(10));
    }

    #[test]
    fn test_threshold_large_orders_prorata() {
        // Threshold: 50 BTC
        let algo = ThresholdProRata::new(Decimal::from(50), Decimal::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        // Add large orders (all above threshold)
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100), // Above threshold
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(200), // Above threshold
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
            Some(Decimal::from(50000)),
            Decimal::from(150),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(150));

        // Pro-rata: sell1 gets 150 * (100/300) = 50, sell2 gets 150 * (200/300) = 100
        let sell1_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell1.id)
            .map(|t| t.quantity)
            .sum();

        let sell2_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell2.id)
            .map(|t| t.quantity)
            .sum();

        assert_eq!(sell1_filled, Decimal::from(50));
        assert_eq!(sell2_filled, Decimal::from(100));
    }

    #[test]
    fn test_threshold_mixed_orders() {
        // Threshold: 50 BTC, minimum: 10 BTC
        let algo = ThresholdProRata::new(Decimal::from(50), Decimal::from(10));
        let side = OrderBookSide::new(Side::Sell);

        // Mix of small and large orders
        let sell_small1 = Arc::new(Order::new(
            "small1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(20), // Small (below threshold)
            TimeInForce::GoodTillCancel,
        ));

        let sell_small2 = Arc::new(Order::new(
            "small2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(30), // Small (below threshold)
            TimeInForce::GoodTillCancel,
        ));

        let sell_large1 = Arc::new(Order::new(
            "large1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100), // Large (above threshold)
            TimeInForce::GoodTillCancel,
        ));

        let sell_large2 = Arc::new(Order::new(
            "large2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(200), // Large (above threshold)
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_small1.clone());
        side.add_order(sell_small2.clone());
        side.add_order(sell_large1.clone());
        side.add_order(sell_large2.clone());

        // Buy 200 BTC
        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(200),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(200));

        // Small orders should be filled completely in FIFO order
        let small1_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_small1.id)
            .map(|t| t.quantity)
            .sum();

        let small2_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_small2.id)
            .map(|t| t.quantity)
            .sum();

        assert_eq!(small1_filled, Decimal::from(20), "Small order 1 should be filled completely");
        assert_eq!(small2_filled, Decimal::from(30), "Small order 2 should be filled completely");

        // Remaining 150 BTC should be allocated pro-rata to large orders
        // large1: 150 * (100/300) = 50
        // large2: 150 * (200/300) = 100
        let large1_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_large1.id)
            .map(|t| t.quantity)
            .sum();

        let large2_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_large2.id)
            .map(|t| t.quantity)
            .sum();

        assert_eq!(large1_filled, Decimal::from(50));
        assert_eq!(large2_filled, Decimal::from(100));
    }

    #[test]
    fn test_threshold_minimum_quantity_filter() {
        // Threshold: 50 BTC, minimum: 20 BTC
        let algo = ThresholdProRata::new(Decimal::from(50), Decimal::from(20));
        let side = OrderBookSide::new(Side::Sell);

        // Small order below minimum (should still participate in FIFO)
        let sell_tiny = Arc::new(Order::new(
            "tiny".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(10), // Below threshold and below minimum
            TimeInForce::GoodTillCancel,
        ));

        // Medium order (below threshold, so treated as small/FIFO)
        let sell_medium = Arc::new(Order::new(
            "medium".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(15), // Below threshold (50), so gets FIFO treatment
            TimeInForce::GoodTillCancel,
        ));

        // Large order above minimum
        let sell_large = Arc::new(Order::new(
            "large".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100), // Above threshold and above minimum
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_tiny.clone());
        side.add_order(sell_medium.clone());
        side.add_order(sell_large.clone());

        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Should fill in this order:
        // 1. tiny: 10 BTC (FIFO, below threshold)
        // 2. medium: 15 BTC (FIFO, below threshold)
        // 3. large: 75 BTC (pro-rata, above threshold)
        // Total: 100 BTC
        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(100));

        // Tiny order should get filled (below threshold = FIFO)
        let tiny_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_tiny.id)
            .map(|t| t.quantity)
            .sum();
        assert_eq!(tiny_filled, Decimal::from(10));

        // Medium order should get filled (below threshold = FIFO)
        let medium_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_medium.id)
            .map(|t| t.quantity)
            .sum();
        assert_eq!(medium_filled, Decimal::from(15));

        // Large order should get the remaining 75 BTC
        let large_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_large.id)
            .map(|t| t.quantity)
            .sum();
        assert_eq!(large_filled, Decimal::from(75));
    }

    #[test]
    fn test_threshold_empty_book() {
        let algo = ThresholdProRata::new(Decimal::from(50), Decimal::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);
        assert!(trades.is_empty(), "No trades with empty book");
    }

    #[test]
    fn test_threshold_only_small_orders() {
        let algo = ThresholdProRata::new(Decimal::from(100), Decimal::ZERO);
        let side = OrderBookSide::new(Side::Sell);

        // All orders below threshold
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(50),
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(50),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());

        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Should be pure FIFO
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].maker_order_id, sell1.id);
        assert_eq!(trades[0].quantity, Decimal::from(50));
        assert_eq!(trades[1].maker_order_id, sell2.id);
        assert_eq!(trades[1].quantity, Decimal::from(50));
    }
}
