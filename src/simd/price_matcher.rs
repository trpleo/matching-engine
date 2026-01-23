// ============================================================================
// SIMD Price Matcher
// Vectorized price comparison for high-performance matching
// ============================================================================

use crate::domain::Side;

/// SIMD-accelerated price matching
pub struct SimdPriceMatcher;

#[cfg(target_arch = "x86_64")]
impl SimdPriceMatcher {
    /// Find indices of prices that can cross with the incoming order
    /// Uses AVX2 for 4x parallel comparison
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

// Non-x86_64 fallback
#[cfg(not(target_arch = "x86_64"))]
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
}
