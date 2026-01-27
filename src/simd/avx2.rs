// ============================================================================
// x86_64 AVX2 Implementation
// SIMD acceleration using AVX2 instructions (256-bit, 4x i64)
// ============================================================================

use super::traits::SimdMatcher;

/// AVX2 implementation of price matching.
///
/// Uses 256-bit AVX2 registers to process 4 i64 values per iteration.
/// Requires runtime detection of AVX2 support.
#[derive(Debug, Clone, Copy, Default)]
pub struct Avx2Matcher;

impl Avx2Matcher {
    /// Create a new AVX2 matcher.
    ///
    /// # Panics
    /// Panics if AVX2 is not available on this CPU.
    /// Use `is_available()` to check before creating.
    pub fn new() -> Self {
        assert!(Self::is_available(), "AVX2 is not available on this CPU");
        Self
    }

    /// Check if AVX2 is available on this CPU.
    #[inline]
    pub fn is_available() -> bool {
        is_x86_feature_detected!("avx2")
    }
}

impl SimdMatcher for Avx2Matcher {
    fn find_crossing_buy_prices(&self, buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
        // Safety: We checked AVX2 availability in new()
        unsafe { avx2_find_crossing_buy(buy_price, ask_prices) }
    }

    fn find_crossing_sell_prices(&self, sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
        unsafe { avx2_find_crossing_sell(sell_price, bid_prices) }
    }

    fn name(&self) -> &'static str {
        "AVX2"
    }
}

/// AVX2-accelerated buy order crossing detection.
///
/// Finds all indices where buy_price >= ask_prices[i].
///
/// # Safety
/// Caller must ensure AVX2 is available.
#[target_feature(enable = "avx2")]
unsafe fn avx2_find_crossing_buy(buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
    use std::arch::x86_64::*;

    let mut crossing_indices = Vec::new();

    // Broadcast buy price to all 4 lanes
    let buy_vec = _mm256_set1_epi64x(buy_price);

    let chunks = ask_prices.chunks_exact(4);
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        // Load 4 ask prices
        let ask_vec = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);

        // We want: buy_price >= ask_price
        // This is equivalent to: NOT(ask_price > buy_price)
        let gt = _mm256_cmpgt_epi64(ask_vec, buy_vec); // ask > buy
        let ge = _mm256_xor_si256(gt, _mm256_set1_epi64x(-1)); // NOT(ask > buy) = buy >= ask

        // Extract mask: cast to double and use movemask
        // Each bit in the 4-bit result corresponds to one i64 lane
        let mask = _mm256_movemask_pd(_mm256_castsi256_pd(ge));

        // Check which lanes crossed
        let base = chunk_idx * 4;
        if (mask & 1) != 0 {
            crossing_indices.push(base);
        }
        if (mask & 2) != 0 {
            crossing_indices.push(base + 1);
        }
        if (mask & 4) != 0 {
            crossing_indices.push(base + 2);
        }
        if (mask & 8) != 0 {
            crossing_indices.push(base + 3);
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

/// AVX2-accelerated sell order crossing detection.
///
/// Finds all indices where sell_price <= bid_prices[i].
///
/// # Safety
/// Caller must ensure AVX2 is available.
#[target_feature(enable = "avx2")]
unsafe fn avx2_find_crossing_sell(sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
    use std::arch::x86_64::*;

    let mut crossing_indices = Vec::new();

    // Broadcast sell price to all 4 lanes
    let sell_vec = _mm256_set1_epi64x(sell_price);

    let chunks = bid_prices.chunks_exact(4);
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        // Load 4 bid prices
        let bid_vec = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);

        // We want: sell_price <= bid_price
        // This is equivalent to: NOT(sell_price > bid_price)
        let gt = _mm256_cmpgt_epi64(sell_vec, bid_vec); // sell > bid
        let le = _mm256_xor_si256(gt, _mm256_set1_epi64x(-1)); // NOT(sell > bid) = sell <= bid

        // Extract mask
        let mask = _mm256_movemask_pd(_mm256_castsi256_pd(le));

        // Check which lanes crossed
        let base = chunk_idx * 4;
        if (mask & 1) != 0 {
            crossing_indices.push(base);
        }
        if (mask & 2) != 0 {
            crossing_indices.push(base + 1);
        }
        if (mask & 4) != 0 {
            crossing_indices.push(base + 2);
        }
        if (mask & 8) != 0 {
            crossing_indices.push(base + 3);
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

    fn skip_if_no_avx2() -> bool {
        !Avx2Matcher::is_available()
    }

    #[test]
    fn test_avx2_availability() {
        // This just checks the detection works, doesn't require AVX2
        let _ = Avx2Matcher::is_available();
    }

    #[test]
    fn test_avx2_buy_crossing() {
        if skip_if_no_avx2() {
            return;
        }

        let matcher = Avx2Matcher::new();
        let asks = vec![
            100_000_000_000i64,
            110_000_000_000,
            120_000_000_000,
            130_000_000_000,
            140_000_000_000,
        ];
        let buy = 125_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_avx2_sell_crossing() {
        if skip_if_no_avx2() {
            return;
        }

        let matcher = Avx2Matcher::new();
        let bids = vec![
            150_000_000_000i64,
            140_000_000_000,
            130_000_000_000,
            120_000_000_000,
            110_000_000_000,
        ];
        let sell = 125_000_000_000i64;

        let result = matcher.find_crossing_sell_prices(sell, &bids);
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_avx2_no_crossing() {
        if skip_if_no_avx2() {
            return;
        }

        let matcher = Avx2Matcher::new();
        let asks = vec![200_000_000_000i64, 210_000_000_000];
        let buy = 100_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert!(result.is_empty());
    }

    #[test]
    fn test_avx2_all_crossing() {
        if skip_if_no_avx2() {
            return;
        }

        let matcher = Avx2Matcher::new();
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
    fn test_avx2_empty_prices() {
        if skip_if_no_avx2() {
            return;
        }

        let matcher = Avx2Matcher::new();
        let result = matcher.find_crossing_buy_prices(100, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_avx2_various_sizes() {
        if skip_if_no_avx2() {
            return;
        }

        let matcher = Avx2Matcher::new();

        // Test sizes that exercise different remainder cases
        for size in [1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17] {
            let asks: Vec<i64> = (0..size).map(|i| 100 + i * 10).collect();
            let buy = 150i64;

            let result = matcher.find_crossing_buy_prices(buy, &asks);

            // Verify against scalar
            let expected: Vec<usize> = asks
                .iter()
                .enumerate()
                .filter_map(|(i, &a)| if buy >= a { Some(i) } else { None })
                .collect();

            assert_eq!(result, expected, "Mismatch for size {}", size);
        }
    }

    #[test]
    fn test_avx2_name() {
        if skip_if_no_avx2() {
            return;
        }

        let matcher = Avx2Matcher::new();
        assert_eq!(matcher.name(), "AVX2");
    }
}
