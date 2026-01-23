// ============================================================================
// LMM Priority (Lead Market Maker) Matching Algorithm
// Used by many derivatives exchanges to incentivize market makers
// ============================================================================

use crate::domain::{Order, OrderBookLevel, OrderBookSide, OrderId, Trade};
use crate::interfaces::MatchingAlgorithm;
use rust_decimal::Decimal;
use std::collections::HashSet;
use std::sync::Arc;

/// LMM Priority matching algorithm
///
/// Designated Lead Market Makers (LMMs) receive a percentage allocation of each trade
/// before the remaining quantity is distributed pro-rata among all participants.
/// This incentivizes dedicated liquidity providers to maintain tight spreads.
///
/// # Example
/// ```text
/// Configuration:
///   - LMM accounts: ["mm1", "mm2"]
///   - LMM allocation: 40%
///   - Minimum quantity: 10 BTC
///
/// Book at 50000:
///   Order A (mm1):  100 BTC (LMM)
///   Order B (user): 150 BTC
///   Order C (mm2):  50 BTC (LMM)
///   Order D (user): 200 BTC
///
/// Incoming: Sell 200 BTC @ 50000
///
/// Step 1 - LMM Allocation (40% = 80 BTC):
///   Total LMM size: 150 BTC (A + C)
///   A gets: 80 * (100/150) = 53 BTC
///   C gets: 80 * (50/150) = 26 BTC
///
/// Step 2 - Pro-Rata for remaining (60% = 120 BTC):
///   All eligible orders (A, B, C, D) total: 500 BTC
///   A gets: 120 * (100/500) = 24 BTC  → Total: 53 + 24 = 77 BTC
///   B gets: 120 * (150/500) = 36 BTC
///   C gets: 120 * (50/500) = 12 BTC   → Total: 26 + 12 = 38 BTC
///   D gets: 120 * (200/500) = 48 BTC
/// ```
pub struct LmmPriority {
    /// Set of account IDs designated as Lead Market Makers
    pub lmm_accounts: HashSet<String>,

    /// Percentage of each trade allocated to LMMs (e.g., 0.4 for 40%)
    pub lmm_allocation_pct: Decimal,

    /// Minimum order size to participate in pro-rata allocation
    pub minimum_quantity: Decimal,
}

impl LmmPriority {
    pub fn new(
        lmm_accounts: Vec<String>,
        lmm_allocation_pct: Decimal,
        minimum_quantity: Decimal,
    ) -> Self {
        Self {
            lmm_accounts: lmm_accounts.into_iter().collect(),
            lmm_allocation_pct,
            minimum_quantity,
        }
    }

    /// Check if an account is a Lead Market Maker
    fn is_lmm(&self, account_id: &str) -> bool {
        self.lmm_accounts.contains(account_id)
    }

    /// Calculate LMM and pro-rata allocations for orders at a price level
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

        // Put all orders back
        for order in all_orders.iter().rev() {
            level.orders.push(Arc::clone(order));
        }

        // Separate LMM and non-LMM orders, filter by minimum quantity
        let mut lmm_orders = Vec::new();
        let mut all_eligible_orders = Vec::new();
        let mut lmm_total_quantity = Decimal::ZERO;
        let mut total_eligible_quantity = Decimal::ZERO;

        for order in all_orders.iter() {
            let remaining = order.get_remaining_quantity();

            if remaining >= self.minimum_quantity {
                total_eligible_quantity += remaining;
                all_eligible_orders.push((order.id, remaining, self.is_lmm(&order.user_id)));

                if self.is_lmm(&order.user_id) {
                    lmm_total_quantity += remaining;
                    lmm_orders.push((order.id, remaining));
                }
            }
        }

        if total_eligible_quantity == Decimal::ZERO {
            return allocations;
        }

        // Step 1: LMM allocation
        let lmm_allocation_qty = quantity_to_fill * self.lmm_allocation_pct;
        let mut lmm_allocated = Decimal::ZERO;

        if lmm_total_quantity > Decimal::ZERO && lmm_allocation_qty > Decimal::ZERO {
            for (order_id, order_quantity) in lmm_orders.iter() {
                let allocation = (order_quantity / lmm_total_quantity) * lmm_allocation_qty;
                let allocation = allocation.floor(); // Round down

                allocations.push((*order_id, allocation));
                lmm_allocated += allocation;
            }

            // Handle LMM remainder - give to first LMM
            let lmm_remainder = lmm_allocation_qty - lmm_allocated;
            if lmm_remainder > Decimal::ZERO && !allocations.is_empty() {
                allocations[0].1 += lmm_remainder;
                lmm_allocated += lmm_remainder;
            }
        }

        // Step 2: Pro-rata allocation for remaining quantity among ALL eligible orders
        let remaining_qty = quantity_to_fill - lmm_allocated;

        if remaining_qty > Decimal::ZERO && total_eligible_quantity > Decimal::ZERO {
            let mut prorata_allocated = Decimal::ZERO;
            let mut prorata_allocs = Vec::new();

            for (order_id, order_quantity, _is_lmm) in all_eligible_orders.iter() {
                let allocation = (order_quantity / total_eligible_quantity) * remaining_qty;
                let allocation = allocation.floor();

                prorata_allocs.push((*order_id, allocation));
                prorata_allocated += allocation;
            }

            // Handle pro-rata remainder
            let prorata_remainder = remaining_qty - prorata_allocated;
            if prorata_remainder > Decimal::ZERO && !prorata_allocs.is_empty() {
                prorata_allocs[0].1 += prorata_remainder;
            }

            // Merge allocations (sum up for orders that appear in both lists)
            for (order_id, prorata_qty) in prorata_allocs {
                if let Some(existing) = allocations.iter_mut().find(|(id, _)| *id == order_id) {
                    existing.1 += prorata_qty;
                } else {
                    allocations.push((order_id, prorata_qty));
                }
            }
        }

        allocations
    }
}

impl MatchingAlgorithm for LmmPriority {
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

            // Calculate allocation (LMM priority + pro-rata)
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
        "LMM-Priority"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, Side, TimeInForce};

    #[test]
    fn test_lmm_priority_allocation() {
        // Setup: 40% LMM allocation, 10 BTC minimum
        let algo = LmmPriority::new(
            vec!["mm1".to_string(), "mm2".to_string()],
            Decimal::from_str_exact("0.4").unwrap(), // 40%
            Decimal::from(10),
        );

        let side = OrderBookSide::new(Side::Sell);

        // Add orders: 2 LMM orders and 2 regular orders
        let sell_mm1 = Arc::new(Order::new(
            "mm1".to_string(), // LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100), // 100 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell_user1 = Arc::new(Order::new(
            "user1".to_string(), // Regular user
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(150), // 150 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell_mm2 = Arc::new(Order::new(
            "mm2".to_string(), // LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(50), // 50 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell_user2 = Arc::new(Order::new(
            "user2".to_string(), // Regular user
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(200), // 200 BTC
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_mm1.clone());
        side.add_order(sell_user1.clone());
        side.add_order(sell_mm2.clone());
        side.add_order(sell_user2.clone());

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

        // Verify total filled
        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(200));

        // LMMs should get preferential treatment
        let mm1_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_mm1.id)
            .map(|t| t.quantity)
            .sum();

        let mm2_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_mm2.id)
            .map(|t| t.quantity)
            .sum();

        // LMMs should get more than their proportional share due to 40% allocation
        // mm1 has 100/500 = 20% of total, but should get more due to LMM priority
        // mm2 has 50/500 = 10% of total, but should get more due to LMM priority
        let lmm_total = mm1_filled + mm2_filled;

        // LMMs should collectively get at least 40% of 200 = 80 BTC
        assert!(lmm_total >= Decimal::from(80), "LMMs should get at least their priority allocation");
    }

    #[test]
    fn test_lmm_only_orders() {
        // Only LMM orders in book
        let algo = LmmPriority::new(
            vec!["mm1".to_string()],
            Decimal::from_str_exact("0.5").unwrap(), // 50%
            Decimal::ZERO,
        );

        let side = OrderBookSide::new(Side::Sell);

        let sell_mm = Arc::new(Order::new(
            "mm1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_mm.clone());

        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(50),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Should match entire quantity with LMM
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, Decimal::from(50));
        assert_eq!(trades[0].maker_order_id, sell_mm.id);
    }

    #[test]
    fn test_no_lmm_orders() {
        // No LMM orders - should behave like regular pro-rata
        let algo = LmmPriority::new(
            vec!["mm1".to_string()],
            Decimal::from_str_exact("0.4").unwrap(),
            Decimal::ZERO,
        );

        let side = OrderBookSide::new(Side::Sell);

        let sell1 = Arc::new(Order::new(
            "user1".to_string(), // Not an LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(), // Not an LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(200),
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
            Decimal::from(150),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(150));

        // Should be pro-rata allocation (no LMM priority)
        // sell1: 100/300 * 150 = 50
        // sell2: 200/300 * 150 = 100
    }

    #[test]
    fn test_lmm_minimum_quantity_filter() {
        let algo = LmmPriority::new(
            vec!["mm1".to_string()],
            Decimal::from_str_exact("0.5").unwrap(),
            Decimal::from(50), // 50 BTC minimum
        );

        let side = OrderBookSide::new(Side::Sell);

        // Small LMM order (below minimum)
        let sell_mm_small = Arc::new(Order::new(
            "mm1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(10), // Below minimum
            TimeInForce::GoodTillCancel,
        ));

        // Large regular order (above minimum)
        let sell_user = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100), // Above minimum
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_mm_small);
        side.add_order(sell_user.clone());

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

        // Should only match with user order (mm order below minimum)
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].maker_order_id, sell_user.id);
        assert_eq!(trades[0].quantity, Decimal::from(100));
    }

    #[test]
    fn test_lmm_empty_book() {
        let algo = LmmPriority::new(
            vec!["mm1".to_string()],
            Decimal::from_str_exact("0.4").unwrap(),
            Decimal::ZERO,
        );

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
    fn test_lmm_multiple_market_makers() {
        // Test with multiple LMMs sharing allocation
        let algo = LmmPriority::new(
            vec!["mm1".to_string(), "mm2".to_string(), "mm3".to_string()],
            Decimal::from_str_exact("0.6").unwrap(), // 60% LMM allocation
            Decimal::ZERO,
        );

        let side = OrderBookSide::new(Side::Sell);

        let sell_mm1 = Arc::new(Order::new(
            "mm1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        let sell_mm2 = Arc::new(Order::new(
            "mm2".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        let sell_mm3 = Arc::new(Order::new(
            "mm3".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(100),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_mm1.clone());
        side.add_order(sell_mm2.clone());
        side.add_order(sell_mm3.clone());

        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(300),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Verify total filled (all orders should be filled)
        let total_filled: Decimal = trades.iter().map(|t| t.quantity).sum();
        assert_eq!(total_filled, Decimal::from(300), "All 300 BTC should be filled");

        // Verify each LMM got their share
        let mm1_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_mm1.id)
            .map(|t| t.quantity)
            .sum();

        let mm2_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_mm2.id)
            .map(|t| t.quantity)
            .sum();

        let mm3_filled: Decimal = trades.iter()
            .filter(|t| t.maker_order_id == sell_mm3.id)
            .map(|t| t.quantity)
            .sum();

        // Each MM should get 100 BTC (their full order size)
        assert_eq!(mm1_filled, Decimal::from(100));
        assert_eq!(mm2_filled, Decimal::from(100));
        assert_eq!(mm3_filled, Decimal::from(100));
    }
}
