// ============================================================================
// ARM NEON Implementation
// SIMD acceleration using ARM NEON instructions (128-bit, 2x i64)
// ============================================================================

#![cfg(target_arch = "aarch64")]

use super::traits::SimdMatcher;

/// ARM NEON implementation of price matching.
///
/// Uses 128-bit NEON registers to process 2 i64 values per iteration.
/// NEON is always available on aarch64 (ARMv8-A baseline).
#[derive(Debug, Clone, Copy, Default)]
pub struct NeonMatcher;

impl NeonMatcher {
    /// Create a new NEON matcher.
    pub fn new() -> Self {
        Self
    }
}

impl SimdMatcher for NeonMatcher {
    fn find_crossing_buy_prices(&self, buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
        // NEON is always available on aarch64
        unsafe { neon_find_crossing_buy(buy_price, ask_prices) }
    }

    fn find_crossing_sell_prices(&self, sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
        unsafe { neon_find_crossing_sell(sell_price, bid_prices) }
    }

    fn name(&self) -> &'static str {
        "NEON"
    }
}

/// NEON-accelerated buy order crossing detection.
///
/// Finds all indices where buy_price >= ask_prices[i].
///
/// # Safety
/// This function uses NEON intrinsics which are always available on aarch64.
#[inline]
unsafe fn neon_find_crossing_buy(buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
    use std::arch::aarch64::*;

    let mut crossing_indices = Vec::new();

    // Broadcast buy price to both lanes of 128-bit register
    let buy_vec = vdupq_n_s64(buy_price);

    let chunks = ask_prices.chunks_exact(2);
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        // Load 2 ask prices into NEON register
        let ask_vec = vld1q_s64(chunk.as_ptr());

        // Compare: buy_price >= ask_price
        // vcgeq_s64 returns all 1s if true, all 0s if false
        let cmp = vcgeq_s64(buy_vec, ask_vec);

        // Extract comparison results from each lane
        let lane0 = vgetq_lane_u64(cmp, 0);
        let lane1 = vgetq_lane_u64(cmp, 1);

        if lane0 != 0 {
            crossing_indices.push(chunk_idx * 2);
        }
        if lane1 != 0 {
            crossing_indices.push(chunk_idx * 2 + 1);
        }
    }

    // Handle remainder with scalar code
    let base_idx = ask_prices.len() - remainder.len();
    for (i, &ask_price) in remainder.iter().enumerate() {
        if buy_price >= ask_price {
            crossing_indices.push(base_idx + i);
        }
    }

    crossing_indices
}

/// NEON-accelerated sell order crossing detection.
///
/// Finds all indices where sell_price <= bid_prices[i].
///
/// # Safety
/// This function uses NEON intrinsics which are always available on aarch64.
#[inline]
unsafe fn neon_find_crossing_sell(sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
    use std::arch::aarch64::*;

    let mut crossing_indices = Vec::new();

    // Broadcast sell price to both lanes
    let sell_vec = vdupq_n_s64(sell_price);

    let chunks = bid_prices.chunks_exact(2);
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        // Load 2 bid prices into NEON register
        let bid_vec = vld1q_s64(chunk.as_ptr());

        // Compare: sell_price <= bid_price
        let cmp = vcleq_s64(sell_vec, bid_vec);

        // Extract comparison results
        let lane0 = vgetq_lane_u64(cmp, 0);
        let lane1 = vgetq_lane_u64(cmp, 1);

        if lane0 != 0 {
            crossing_indices.push(chunk_idx * 2);
        }
        if lane1 != 0 {
            crossing_indices.push(chunk_idx * 2 + 1);
        }
    }

    // Handle remainder
    let base_idx = bid_prices.len() - remainder.len();
    for (i, &bid_price) in remainder.iter().enumerate() {
        if sell_price <= bid_price {
            crossing_indices.push(base_idx + i);
        }
    }

    crossing_indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neon_buy_crossing() {
        let matcher = NeonMatcher::new();
        let asks = vec![
            100_000_000_000i64,
            110_000_000_000,
            120_000_000_000,
            130_000_000_000,
            140_000_000_000,
        ];
        let buy = 125_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1, 2]); // Crosses 100, 110, 120
    }

    #[test]
    fn test_neon_sell_crossing() {
        let matcher = NeonMatcher::new();
        let bids = vec![
            150_000_000_000i64,
            140_000_000_000,
            130_000_000_000,
            120_000_000_000,
            110_000_000_000,
        ];
        let sell = 125_000_000_000i64;

        let result = matcher.find_crossing_sell_prices(sell, &bids);
        assert_eq!(result, vec![0, 1, 2]); // Crosses 150, 140, 130
    }

    #[test]
    fn test_neon_no_crossing() {
        let matcher = NeonMatcher::new();
        let asks = vec![200_000_000_000i64, 210_000_000_000];
        let buy = 100_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert!(result.is_empty());
    }

    #[test]
    fn test_neon_all_crossing() {
        let matcher = NeonMatcher::new();
        let asks = vec![
            100_000_000_000i64,
            110_000_000_000,
            120_000_000_000,
            130_000_000_000,
        ];
        let buy = 200_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_neon_empty_prices() {
        let matcher = NeonMatcher::new();
        let result = matcher.find_crossing_buy_prices(100, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_neon_single_element() {
        let matcher = NeonMatcher::new();
        let asks = vec![100_000_000_000i64];
        let buy = 150_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_neon_odd_count() {
        let matcher = NeonMatcher::new();
        let asks = vec![100_000_000_000i64, 110_000_000_000, 120_000_000_000]; // 3 elements (odd)
        let buy = 115_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_neon_name() {
        let matcher = NeonMatcher::new();
        assert_eq!(matcher.name(), "NEON");
    }

    #[test]
    fn test_neon_consistency_with_scalar() {
        use crate::simd::scalar::ScalarMatcher;

        let neon = NeonMatcher::new();
        let scalar = ScalarMatcher::new();

        // Test with various sizes to exercise both SIMD and remainder paths
        for size in [1, 2, 3, 5, 7, 10, 15, 100] {
            let asks: Vec<i64> = (0..size).map(|i| 100 + i * 10).collect();
            let buy = 150i64;

            let neon_result = neon.find_crossing_buy_prices(buy, &asks);
            let scalar_result = scalar.find_crossing_buy_prices(buy, &asks);

            assert_eq!(
                neon_result, scalar_result,
                "Mismatch for size {}: NEON={:?}, Scalar={:?}",
                size, neon_result, scalar_result
            );
        }
    }
}
