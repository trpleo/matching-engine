// ============================================================================
// Scalar (Non-SIMD) Implementation
// Fallback implementation using standard scalar operations
// ============================================================================

use super::traits::SimdMatcher;

/// Scalar implementation of price matching.
///
/// This is the fallback implementation that works on all platforms.
/// It uses simple iteration without SIMD instructions.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScalarMatcher;

impl ScalarMatcher {
    /// Create a new scalar matcher.
    pub fn new() -> Self {
        Self
    }
}

impl SimdMatcher for ScalarMatcher {
    fn find_crossing_buy_prices(&self, buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
        ask_prices
            .iter()
            .enumerate()
            .filter_map(|(idx, &ask_price)| {
                if buy_price >= ask_price {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    fn find_crossing_sell_prices(&self, sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
        bid_prices
            .iter()
            .enumerate()
            .filter_map(|(idx, &bid_price)| {
                if sell_price <= bid_price {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "Scalar"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scalar_buy_crossing() {
        let matcher = ScalarMatcher::new();
        let asks = vec![
            100_000_000_000i64, // 100.0
            110_000_000_000,    // 110.0
            120_000_000_000,    // 120.0
            130_000_000_000,    // 130.0
        ];
        let buy = 115_000_000_000i64; // 115.0

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1]); // Crosses 100 and 110
    }

    #[test]
    fn test_scalar_sell_crossing() {
        let matcher = ScalarMatcher::new();
        let bids = vec![
            130_000_000_000i64, // 130.0
            120_000_000_000,    // 120.0
            110_000_000_000,    // 110.0
            100_000_000_000,    // 100.0
        ];
        let sell = 115_000_000_000i64; // 115.0

        let result = matcher.find_crossing_sell_prices(sell, &bids);
        assert_eq!(result, vec![0, 1]); // Crosses 130 and 120
    }

    #[test]
    fn test_scalar_no_crossing() {
        let matcher = ScalarMatcher::new();
        let asks = vec![200_000_000_000i64, 210_000_000_000];
        let buy = 100_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert!(result.is_empty());
    }

    #[test]
    fn test_scalar_all_crossing() {
        let matcher = ScalarMatcher::new();
        let asks = vec![100_000_000_000i64, 110_000_000_000, 120_000_000_000];
        let buy = 200_000_000_000i64;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_scalar_empty_prices() {
        let matcher = ScalarMatcher::new();
        let result = matcher.find_crossing_buy_prices(100, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_scalar_name() {
        let matcher = ScalarMatcher::new();
        assert_eq!(matcher.name(), "Scalar");
    }
}
