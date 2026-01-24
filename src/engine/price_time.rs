// ============================================================================
// Price/Time Priority Matching Algorithm (FIFO)
// Most common in traditional exchanges (NASDAQ, NYSE, etc.)
// ============================================================================

use crate::domain::{Order, OrderBookSide, Trade};
use crate::interfaces::MatchingAlgorithm;
use crate::numeric::Quantity;
use std::sync::Arc;

/// Price/Time Priority (FIFO) matching algorithm
///
/// Orders at the same price level are matched in time priority order.
/// This is the most common matching algorithm used by traditional exchanges.
///
/// # Example
/// ```text
/// Book:  50000 @ 1.0 BTC (Order A, t=100)
///        50000 @ 2.0 BTC (Order B, t=101)
///
/// Incoming: Buy 1.5 BTC @ 50000
/// Result: Match 1.0 with A, then 0.5 with B
/// ```
pub struct PriceTimePriority {
    use_simd: bool,
}

impl PriceTimePriority {
    pub fn new(use_simd: bool) -> Self {
        Self { use_simd }
    }
}

impl MatchingAlgorithm for PriceTimePriority {
    fn match_order(&self, incoming_order: Arc<Order>, opposite_side: &OrderBookSide) -> Vec<Trade> {
        let mut trades = Vec::new();

        // TODO: SIMD optimization will be re-implemented in Phase 4 with FixedDecimal
        // The old f64-based SIMD code has been removed as part of the architecture refactoring.
        // The new implementation will use i64-based SIMD operations via SimdMatcher trait.
        let _ = self.use_simd; // Silence unused warning until Phase 4

        // Match orders in FIFO order
        while incoming_order.get_remaining_quantity() > Quantity::ZERO {
            // Get best price level
            let best_level = match opposite_side.best_level() {
                Some(level) => level,
                None => break,
            };

            // Check if prices cross
            if !self.prices_cross(&incoming_order, best_level.price) {
                break;
            }

            // Pop orders from the level (FIFO)
            while let Some(maker_order) = best_level.orders.pop() {
                let maker_remaining = maker_order.get_remaining_quantity();
                let taker_remaining = incoming_order.get_remaining_quantity();

                if maker_remaining == Quantity::ZERO {
                    continue; // Skip already filled orders
                }

                let trade_quantity = taker_remaining.min(maker_remaining);

                // Atomic fill operations
                if maker_order.try_fill(trade_quantity) && incoming_order.try_fill(trade_quantity) {
                    // Create trade
                    let trade = Trade::new(
                        (*incoming_order.instrument).clone(),
                        maker_order.id,
                        incoming_order.id,
                        maker_order.price.unwrap(),
                        trade_quantity,
                    );

                    // Update level quantity
                    best_level.subtract_quantity(trade_quantity);

                    trades.push(trade);

                    // If maker still has quantity, put it back
                    if maker_order.get_remaining_quantity() > Quantity::ZERO {
                        best_level.orders.push(Arc::clone(&maker_order));
                        break; // Process next incoming order
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

            if incoming_order.get_remaining_quantity() == Quantity::ZERO {
                break;
            }
        }

        trades
    }

    fn name(&self) -> &str {
        if self.use_simd {
            "PriceTime-SIMD"
        } else {
            "PriceTime"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, Side, TimeInForce};
    use crate::numeric::Price;

    #[test]
    fn test_price_time_fifo_order() {
        let algo = PriceTimePriority::new(false);
        let side = OrderBookSide::new(Side::Sell);

        // Add two sell orders at same price, different times
        let sell1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());

        // Add buy order
        let buy = Arc::new(Order::new(
            "user3".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        assert_eq!(trades.len(), 1);
        // Should match with first order (sell1) due to FIFO
        assert_eq!(trades[0].maker_order_id, sell1.id);
    }

    #[test]
    fn test_price_time_partial_fill() {
        let algo = PriceTimePriority::new(false);
        let side = OrderBookSide::new(Side::Sell);

        let sell = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell.clone());

        // Buy more than available
        let buy = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(2).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy.clone(), &side);

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, Quantity::from_integer(1).unwrap());
        assert_eq!(buy.get_remaining_quantity(), Quantity::from_integer(1).unwrap());
    }
}
