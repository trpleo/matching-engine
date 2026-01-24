// ============================================================================
// x86_64 AVX-512 Implementation
// SIMD acceleration using AVX-512 instructions (512-bit, 8x i64)
// ============================================================================

#![cfg(target_arch = "x86_64")]

use super::traits::SimdMatcher;

/// AVX-512 implementation of price matching.
///
/// Uses 512-bit AVX-512 registers to process 8 i64 values per iteration.
/// Requires runtime detection of AVX-512F support.
///
/// AVX-512 provides native mask-based comparisons, making the implementation
/// cleaner and potentially faster than AVX2.
#[derive(Debug, Clone, Copy, Default)]
pub struct Avx512Matcher;

impl Avx512Matcher {
    /// Create a new AVX-512 matcher.
    ///
    /// # Panics
    /// Panics if AVX-512F is not available on this CPU.
    /// Use `is_available()` to check before creating.
    pub fn new() -> Self {
        assert!(
            Self::is_available(),
            "AVX-512F is not available on this CPU"
        );
        Self
    }

    /// Check if AVX-512F is available on this CPU.
    #[inline]
    pub fn is_available() -> bool {
        is_x86_feature_detected!("avx512f")
    }
}

impl SimdMatcher for Avx512Matcher {
    fn find_crossing_buy_prices(&self, buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
        // Safety: We checked AVX-512F availability in new()
        unsafe { avx512_find_crossing_buy(buy_price, ask_prices) }
    }

    fn find_crossing_sell_prices(&self, sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
        unsafe { avx512_find_crossing_sell(sell_price, bid_prices) }
    }

    fn name(&self) -> &'static str {
        "AVX-512"
    }
}

/// AVX-512 accelerated buy order crossing detection.
///
/// Finds all indices where buy_price >= ask_prices[i].
///
/// # Safety
/// Caller must ensure AVX-512F is available.
#[target_feature(enable = "avx512f")]
unsafe fn avx512_find_crossing_buy(buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
    use std::arch::x86_64::*;

    let mut crossing_indices = Vec::new();

    // Broadcast buy price to all 8 lanes
    let buy_vec = _mm512_set1_epi64(buy_price);

    let chunks = ask_prices.chunks_exact(8);
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        // Load 8 ask prices
        let ask_vec = _mm512_loadu_si512(chunk.as_ptr() as *const i32);

        // AVX-512 has native >= comparison returning a mask
        // _mm512_cmpge_epi64_mask: returns 8-bit mask where bit i is set if a[i] >= b[i]
        let mask = _mm512_cmpge_epi64_mask(buy_vec, ask_vec);

        // Check which lanes crossed (mask is 8 bits for 8 lanes)
        let base = chunk_idx * 8;
        for i in 0..8 {
            if (mask & (1 << i)) != 0 {
                crossing_indices.push(base + i);
            }
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

/// AVX-512 accelerated sell order crossing detection.
///
/// Finds all indices where sell_price <= bid_prices[i].
///
/// # Safety
/// Caller must ensure AVX-512F is available.
#[target_feature(enable = "avx512f")]
unsafe fn avx512_find_crossing_sell(sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
    use std::arch::x86_64::*;

    let mut crossing_indices = Vec::new();

    // Broadcast sell price to all 8 lanes
    let sell_vec = _mm512_set1_epi64(sell_price);

    let chunks = bid_prices.chunks_exact(8);
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        // Load 8 bid prices
        let bid_vec = _mm512_loadu_si512(chunk.as_ptr() as *const i32);

        // _mm512_cmple_epi64_mask: returns 8-bit mask where bit i is set if a[i] <= b[i]
        let mask = _mm512_cmple_epi64_mask(sell_vec, bid_vec);

        // Check which lanes crossed
        let base = chunk_idx * 8;
        for i in 0..8 {
            if (mask & (1 << i)) != 0 {
                crossing_indices.push(base + i);
            }
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

    fn skip_if_no_avx512() -> bool {
        !Avx512Matcher::is_available()
    }

    #[test]
    fn test_avx512_availability() {
        // This just checks the detection works, doesn't require AVX-512
        let _ = Avx512Matcher::is_available();
    }

    #[test]
    fn test_avx512_buy_crossing() {
        if skip_if_no_avx512() {
            return;
        }

        let matcher = Avx512Matcher::new();
        let asks = vec![
            100_000_000_000i64,
            110_000_000_000,
            120_000_000_000,
            130_000_000_000,
            140_000_000_000,
            150_000_000_000,
            160_000_000_000,
            170_000_000_000,
            180_000_000_000,
        ];
        let buy = 145_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_avx512_sell_crossing() {
        if skip_if_no_avx512() {
            return;
        }

        let matcher = Avx512Matcher::new();
        let bids = vec![
            180_000_000_000i64,
            170_000_000_000,
            160_000_000_000,
            150_000_000_000,
            140_000_000_000,
            130_000_000_000,
            120_000_000_000,
            110_000_000_000,
            100_000_000_000,
        ];
        let sell = 145_000_000_000i64;

        let result = matcher.find_crossing_sell_prices(sell, &bids);
        assert_eq!(result, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_avx512_no_crossing() {
        if skip_if_no_avx512() {
            return;
        }

        let matcher = Avx512Matcher::new();
        let asks = vec![200_000_000_000i64; 10];
        let buy = 100_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert!(result.is_empty());
    }

    #[test]
    fn test_avx512_all_crossing() {
        if skip_if_no_avx512() {
            return;
        }

        let matcher = Avx512Matcher::new();
        let asks = vec![100_000_000_000i64; 10];
        let buy = 200_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn test_avx512_empty_prices() {
        if skip_if_no_avx512() {
            return;
        }

        let matcher = Avx512Matcher::new();
        let result = matcher.find_crossing_buy_prices(100, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_avx512_various_sizes() {
        if skip_if_no_avx512() {
            return;
        }

        let matcher = Avx512Matcher::new();

        // Test sizes that exercise different remainder cases (8 per chunk)
        for size in [1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 24, 25] {
            let asks: Vec<i64> = (0..size).map(|i| 100 + i * 10).collect();
            let buy = 200i64;

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
    fn test_avx512_name() {
        if skip_if_no_avx512() {
            return;
        }

        let matcher = Avx512Matcher::new();
        assert_eq!(matcher.name(), "AVX-512");
    }
}
