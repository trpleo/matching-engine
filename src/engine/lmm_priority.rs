// ============================================================================
// LMM Priority (Lead Market Maker) Matching Algorithm
// Used by many derivatives exchanges to incentivize market makers
// ============================================================================

use crate::domain::{Order, OrderBookLevel, OrderBookSide, OrderId, Trade};
use crate::interfaces::MatchingAlgorithm;
use crate::numeric::Quantity;
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
    pub lmm_allocation_pct: Quantity,

    /// Minimum order size to participate in pro-rata allocation
    pub minimum_quantity: Quantity,
}

impl LmmPriority {
    pub fn new(
        lmm_accounts: Vec<String>,
        lmm_allocation_pct: Quantity,
        minimum_quantity: Quantity,
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

        // Put all orders back
        for order in all_orders.iter().rev() {
            level.orders.push(Arc::clone(order));
        }

        // Separate LMM and non-LMM orders, filter by minimum quantity
        let mut lmm_orders = Vec::new();
        let mut all_eligible_orders = Vec::new();
        let mut lmm_total_quantity = Quantity::ZERO;
        let mut total_eligible_quantity = Quantity::ZERO;

        for order in all_orders.iter() {
            let remaining = order.get_remaining_quantity();

            if remaining >= self.minimum_quantity {
                total_eligible_quantity = total_eligible_quantity + remaining;
                all_eligible_orders.push((order.id, remaining, self.is_lmm(&order.user_id)));

                if self.is_lmm(&order.user_id) {
                    lmm_total_quantity = lmm_total_quantity + remaining;
                    lmm_orders.push((order.id, remaining));
                }
            }
        }

        if total_eligible_quantity == Quantity::ZERO {
            return allocations;
        }

        // Step 1: LMM allocation
        // lmm_allocation_qty = quantity_to_fill * lmm_allocation_pct
        // Since lmm_allocation_pct is stored as a Quantity (e.g., 0.4 = 400_000_000 raw),
        // we need to multiply and then divide by 10^9
        let lmm_allocation_qty = Quantity::from_raw(
            (quantity_to_fill.raw_value() as i128 * self.lmm_allocation_pct.raw_value() as i128
                / 1_000_000_000) as i64,
        );
        let mut lmm_allocated = Quantity::ZERO;

        if lmm_total_quantity > Quantity::ZERO && lmm_allocation_qty > Quantity::ZERO {
            let lmm_total_raw = lmm_total_quantity.raw_value();
            let lmm_alloc_raw = lmm_allocation_qty.raw_value();

            for (order_id, order_quantity) in lmm_orders.iter() {
                let order_raw = order_quantity.raw_value();
                let allocation_raw =
                    ((order_raw as i128 * lmm_alloc_raw as i128) / lmm_total_raw as i128) as i64;
                let allocation = Quantity::from_raw(allocation_raw);

                allocations.push((*order_id, allocation));
                lmm_allocated = lmm_allocated + allocation;
            }

            // Handle LMM remainder - give to first LMM
            let lmm_remainder = lmm_allocation_qty - lmm_allocated;
            if lmm_remainder > Quantity::ZERO && !allocations.is_empty() {
                allocations[0].1 = allocations[0].1 + lmm_remainder;
                lmm_allocated = lmm_allocated + lmm_remainder;
            }
        }

        // Step 2: Pro-rata allocation for remaining quantity among ALL eligible orders
        let remaining_qty = quantity_to_fill - lmm_allocated;

        if remaining_qty > Quantity::ZERO && total_eligible_quantity > Quantity::ZERO {
            let mut prorata_allocated = Quantity::ZERO;
            let mut prorata_allocs = Vec::new();
            let total_eligible_raw = total_eligible_quantity.raw_value();
            let remaining_raw = remaining_qty.raw_value();

            for (order_id, order_quantity, _is_lmm) in all_eligible_orders.iter() {
                let order_raw = order_quantity.raw_value();
                let allocation_raw = ((order_raw as i128 * remaining_raw as i128)
                    / total_eligible_raw as i128) as i64;
                let allocation = Quantity::from_raw(allocation_raw);

                prorata_allocs.push((*order_id, allocation));
                prorata_allocated = prorata_allocated + allocation;
            }

            // Handle pro-rata remainder
            let prorata_remainder = remaining_qty - prorata_allocated;
            if prorata_remainder > Quantity::ZERO && !prorata_allocs.is_empty() {
                prorata_allocs[0].1 = prorata_allocs[0].1 + prorata_remainder;
            }

            // Merge allocations (sum up for orders that appear in both lists)
            for (order_id, prorata_qty) in prorata_allocs {
                if let Some(existing) = allocations.iter_mut().find(|(id, _)| *id == order_id) {
                    existing.1 = existing.1 + prorata_qty;
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

        while incoming_order.get_remaining_quantity() > Quantity::ZERO {
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
        "LMM-Priority"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, Side, TimeInForce};
    use crate::numeric::Price;

    #[test]
    fn test_lmm_priority_allocation() {
        // Setup: 40% LMM allocation, 10 BTC minimum
        let algo = LmmPriority::new(
            vec!["mm1".to_string(), "mm2".to_string()],
            Quantity::from_parts(0, 400_000_000).unwrap(), // 0.4 = 40%
            Quantity::from_integer(10).unwrap(),
        );

        let side = OrderBookSide::new(Side::Sell);

        // Add orders: 2 LMM orders and 2 regular orders
        let sell_mm1 = Arc::new(Order::new(
            "mm1".to_string(), // LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(), // 100 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell_user1 = Arc::new(Order::new(
            "user1".to_string(), // Regular user
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(150).unwrap(), // 150 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell_mm2 = Arc::new(Order::new(
            "mm2".to_string(), // LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(50).unwrap(), // 50 BTC
            TimeInForce::GoodTillCancel,
        ));

        let sell_user2 = Arc::new(Order::new(
            "user2".to_string(), // Regular user
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(200).unwrap(), // 200 BTC
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
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(200).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let trades = algo.match_order(buy, &side);

        // Verify total filled
        let total_filled: Quantity = trades
            .iter()
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);
        assert_eq!(total_filled, Quantity::from_integer(200).unwrap());

        // LMMs should get preferential treatment
        let mm1_filled: Quantity = trades
            .iter()
            .filter(|t| t.maker_order_id == sell_mm1.id)
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);

        let mm2_filled: Quantity = trades
            .iter()
            .filter(|t| t.maker_order_id == sell_mm2.id)
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);

        // LMMs should get more than their proportional share due to 40% allocation
        let lmm_total = mm1_filled + mm2_filled;

        // LMMs should collectively get at least 40% of 200 = 80 BTC
        assert!(
            lmm_total >= Quantity::from_integer(80).unwrap(),
            "LMMs should get at least their priority allocation"
        );
    }

    #[test]
    fn test_lmm_only_orders() {
        // Only LMM orders in book
        let algo = LmmPriority::new(
            vec!["mm1".to_string()],
            Quantity::from_parts(0, 500_000_000).unwrap(), // 0.5 = 50%
            Quantity::ZERO,
        );

        let side = OrderBookSide::new(Side::Sell);

        let sell_mm = Arc::new(Order::new(
            "mm1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell_mm.clone());

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

        // Should match entire quantity with LMM
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, Quantity::from_integer(50).unwrap());
        assert_eq!(trades[0].maker_order_id, sell_mm.id);
    }

    #[test]
    fn test_no_lmm_orders() {
        // No LMM orders - should behave like regular pro-rata
        let algo = LmmPriority::new(
            vec!["mm1".to_string()],
            Quantity::from_parts(0, 400_000_000).unwrap(),
            Quantity::ZERO,
        );

        let side = OrderBookSide::new(Side::Sell);

        let sell1 = Arc::new(Order::new(
            "user1".to_string(), // Not an LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(100).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let sell2 = Arc::new(Order::new(
            "user2".to_string(), // Not an LMM
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(200).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(sell1.clone());
        side.add_order(sell2.clone());

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

        let total_filled: Quantity = trades
            .iter()
            .map(|t| t.quantity)
            .fold(Quantity::ZERO, |a, b| a + b);
        assert_eq!(total_filled, Quantity::from_integer(150).unwrap());
    }

    #[test]
    fn test_lmm_empty_book() {
        let algo = LmmPriority::new(
            vec!["mm1".to_string()],
            Quantity::from_parts(0, 400_000_000).unwrap(),
            Quantity::ZERO,
        );

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
