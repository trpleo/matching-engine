// ============================================================================
// SIMD Price Matcher
// Vectorized price comparison for high-performance matching
// Supports: AVX2 (x86_64), NEON (aarch64), with scalar fallback
// ============================================================================

use crate::domain::Side;

/// SIMD-accelerated price matching
pub struct SimdPriceMatcher;

// ============================================================================
// x86_64 Implementation (AVX2)
// Processes 4 f64 values per iteration using 256-bit registers
// ============================================================================
#[cfg(target_arch = "x86_64")]
impl SimdPriceMatcher {
    /// Find indices of prices that can cross with the incoming order
    /// Uses AVX2 for 4x parallel comparison when available
    pub fn find_crossing_prices(side: Side, price: f64, opposite_prices: &[f64]) -> Vec<usize> {
        if is_x86_feature_detected!("avx2") {
            unsafe {
                match side {
                    Side::Buy => Self::simd_find_crossing_buy(price, opposite_prices),
                    Side::Sell => Self::simd_find_crossing_sell(price, opposite_prices),
                }
            }
        } else {
            Self::scalar_find_crossing(side, price, opposite_prices)
        }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn simd_find_crossing_buy(buy_price: f64, ask_prices: &[f64]) -> Vec<usize> {
        use std::arch::x86_64::*;

        let mut crossing_indices = Vec::new();
        let buy_price_vec = _mm256_set1_pd(buy_price);

        let chunks = ask_prices.chunks_exact(4);
        let remainder = chunks.remainder();

        for (chunk_idx, chunk) in chunks.enumerate() {
            // Load 4 ask prices
            let ask_vec = _mm256_loadu_pd(chunk.as_ptr());

            // Compare: buy_price >= ask_price (can cross)
            let cmp = _mm256_cmp_pd(buy_price_vec, ask_vec, _CMP_GE_OQ);

            // Extract comparison mask
            let mask = _mm256_movemask_pd(cmp);

            // Check which prices crossed
            for i in 0..4 {
                if (mask & (1 << i)) != 0 {
                    crossing_indices.push(chunk_idx * 4 + i);
                }
            }
        }

        // Handle remainder with scalar code
        for (i, &ask_price) in remainder.iter().enumerate() {
            if buy_price >= ask_price {
                crossing_indices.push(ask_prices.len() - remainder.len() + i);
            }
        }

        crossing_indices
    }

    #[target_feature(enable = "avx2")]
    unsafe fn simd_find_crossing_sell(sell_price: f64, bid_prices: &[f64]) -> Vec<usize> {
        use std::arch::x86_64::*;

        let mut crossing_indices = Vec::new();
        let sell_price_vec = _mm256_set1_pd(sell_price);

        let chunks = bid_prices.chunks_exact(4);
        let remainder = chunks.remainder();

        for (chunk_idx, chunk) in chunks.enumerate() {
            // Load 4 bid prices
            let bid_vec = _mm256_loadu_pd(chunk.as_ptr());

            // Compare: sell_price <= bid_price (can cross)
            let cmp = _mm256_cmp_pd(sell_price_vec, bid_vec, _CMP_LE_OQ);

            // Extract comparison mask
            let mask = _mm256_movemask_pd(cmp);

            // Check which prices crossed
            for i in 0..4 {
                if (mask & (1 << i)) != 0 {
                    crossing_indices.push(chunk_idx * 4 + i);
                }
            }
        }

        // Handle remainder
        for (i, &bid_price) in remainder.iter().enumerate() {
            if sell_price <= bid_price {
                crossing_indices.push(bid_prices.len() - remainder.len() + i);
            }
        }

        crossing_indices
    }

    fn scalar_find_crossing(side: Side, price: f64, opposite_prices: &[f64]) -> Vec<usize> {
        opposite_prices
            .iter()
            .enumerate()
            .filter_map(|(idx, &opp_price)| {
                let crosses = match side {
                    Side::Buy => price >= opp_price,
                    Side::Sell => price <= opp_price,
                };
                if crosses {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }
}

// ============================================================================
// aarch64 Implementation (ARM NEON)
// Processes 2 f64 values per iteration using 128-bit registers
// NEON is always available on aarch64 (ARMv8-A baseline)
// ============================================================================
#[cfg(target_arch = "aarch64")]
impl SimdPriceMatcher {
    /// Find indices of prices that can cross with the incoming order
    /// Uses ARM NEON for 2x parallel comparison
    pub fn find_crossing_prices(side: Side, price: f64, opposite_prices: &[f64]) -> Vec<usize> {
        // NEON is always available on aarch64, no runtime detection needed
        unsafe {
            match side {
                Side::Buy => Self::neon_find_crossing_buy(price, opposite_prices),
                Side::Sell => Self::neon_find_crossing_sell(price, opposite_prices),
            }
        }
    }

    /// NEON-accelerated buy order crossing detection
    /// Compares buy_price >= ask_price for each ask in the book
    unsafe fn neon_find_crossing_buy(buy_price: f64, ask_prices: &[f64]) -> Vec<usize> {
        use std::arch::aarch64::*;

        let mut crossing_indices = Vec::new();

        // Broadcast buy price to both lanes of 128-bit register
        let buy_price_vec = vdupq_n_f64(buy_price);

        let chunks = ask_prices.chunks_exact(2);
        let remainder = chunks.remainder();

        for (chunk_idx, chunk) in chunks.enumerate() {
            // Load 2 ask prices into NEON register
            let ask_vec = vld1q_f64(chunk.as_ptr());

            // Compare: buy_price >= ask_price (can cross)
            // Result is all 1s (0xFFFFFFFFFFFFFFFF) if true, all 0s if false
            let cmp = vcgeq_f64(buy_price_vec, ask_vec);

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
        for (i, &ask_price) in remainder.iter().enumerate() {
            if buy_price >= ask_price {
                crossing_indices.push(ask_prices.len() - remainder.len() + i);
            }
        }

        crossing_indices
    }

    /// NEON-accelerated sell order crossing detection
    /// Compares sell_price <= bid_price for each bid in the book
    unsafe fn neon_find_crossing_sell(sell_price: f64, bid_prices: &[f64]) -> Vec<usize> {
        use std::arch::aarch64::*;

        let mut crossing_indices = Vec::new();

        // Broadcast sell price to both lanes
        let sell_price_vec = vdupq_n_f64(sell_price);

        let chunks = bid_prices.chunks_exact(2);
        let remainder = chunks.remainder();

        for (chunk_idx, chunk) in chunks.enumerate() {
            // Load 2 bid prices into NEON register
            let bid_vec = vld1q_f64(chunk.as_ptr());

            // Compare: sell_price <= bid_price (can cross)
            let cmp = vcleq_f64(sell_price_vec, bid_vec);

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
        for (i, &bid_price) in remainder.iter().enumerate() {
            if sell_price <= bid_price {
                crossing_indices.push(bid_prices.len() - remainder.len() + i);
            }
        }

        crossing_indices
    }

    /// Scalar fallback (exposed for benchmarking comparison)
    #[allow(dead_code)]
    pub fn scalar_find_crossing(side: Side, price: f64, opposite_prices: &[f64]) -> Vec<usize> {
        opposite_prices
            .iter()
            .enumerate()
            .filter_map(|(idx, &opp_price)| {
                let crosses = match side {
                    Side::Buy => price >= opp_price,
                    Side::Sell => price <= opp_price,
                };
                if crosses {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }
}

// ============================================================================
// Fallback Implementation (other architectures)
// Pure scalar implementation for platforms without SIMD support
// ============================================================================
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
impl SimdPriceMatcher {
    pub fn find_crossing_prices(side: Side, price: f64, opposite_prices: &[f64]) -> Vec<usize> {
        opposite_prices
            .iter()
            .enumerate()
            .filter_map(|(idx, &opp_price)| {
                let crosses = match side {
                    Side::Buy => price >= opp_price,
                    Side::Sell => price <= opp_price,
                };
                if crosses {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_buy_crossing() {
        let ask_prices = vec![50100.0, 50200.0, 50300.0, 50400.0, 50500.0];
        let buy_price = 50250.0;

        let crossing = SimdPriceMatcher::find_crossing_prices(Side::Buy, buy_price, &ask_prices);

        // Should cross with 50100 and 50200
        assert_eq!(crossing, vec![0, 1]);
    }

    #[test]
    fn test_simd_sell_crossing() {
        let bid_prices = vec![50000.0, 49900.0, 49800.0, 49700.0];
        let sell_price = 49850.0;

        let crossing = SimdPriceMatcher::find_crossing_prices(Side::Sell, sell_price, &bid_prices);

        // Should cross with 50000 and 49900
        assert_eq!(crossing, vec![0, 1]);
    }

    #[test]
    fn test_no_crossing() {
        let ask_prices = vec![50100.0, 50200.0];
        let buy_price = 50000.0;

        let crossing = SimdPriceMatcher::find_crossing_prices(Side::Buy, buy_price, &ask_prices);

        assert!(crossing.is_empty());
    }

    #[test]
    fn test_all_crossing() {
        let ask_prices = vec![50000.0, 50100.0, 50200.0, 50300.0, 50400.0];
        let buy_price = 50500.0;

        let crossing = SimdPriceMatcher::find_crossing_prices(Side::Buy, buy_price, &ask_prices);

        assert_eq!(crossing, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_single_element() {
        let ask_prices = vec![50000.0];
        let buy_price = 50100.0;

        let crossing = SimdPriceMatcher::find_crossing_prices(Side::Buy, buy_price, &ask_prices);

        assert_eq!(crossing, vec![0]);
    }

    #[test]
    fn test_empty_prices() {
        let ask_prices: Vec<f64> = vec![];
        let buy_price = 50000.0;

        let crossing = SimdPriceMatcher::find_crossing_prices(Side::Buy, buy_price, &ask_prices);

        assert!(crossing.is_empty());
    }

    #[test]
    fn test_large_price_list() {
        // Test with more prices than SIMD register width to ensure remainder handling
        let ask_prices: Vec<f64> = (0..17).map(|i| 50000.0 + i as f64 * 100.0).collect();
        let buy_price = 50550.0; // Should cross with first 6 prices

        let crossing = SimdPriceMatcher::find_crossing_prices(Side::Buy, buy_price, &ask_prices);

        assert_eq!(crossing, vec![0, 1, 2, 3, 4, 5]);
    }

    // Test scalar implementation on aarch64 for comparison
    #[cfg(target_arch = "aarch64")]
    #[test]
    fn test_scalar_vs_neon_consistency() {
        let ask_prices: Vec<f64> = (0..100).map(|i| 50000.0 + i as f64 * 10.0).collect();
        let buy_price = 50250.0;

        let neon_result = SimdPriceMatcher::find_crossing_prices(Side::Buy, buy_price, &ask_prices);
        let scalar_result = SimdPriceMatcher::scalar_find_crossing(Side::Buy, buy_price, &ask_prices);

        assert_eq!(neon_result, scalar_result);
    }
}
