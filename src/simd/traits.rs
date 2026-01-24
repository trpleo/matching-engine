// ============================================================================
// SIMD Matcher Trait
// Abstract interface for SIMD-accelerated price matching
// ============================================================================

/// Trait for SIMD-accelerated price matching operations.
///
/// Implementations provide vectorized comparisons for finding
/// crossing prices in the order book.
///
/// # Thread Safety
/// All implementations must be `Send + Sync` to allow sharing
/// across threads in the matching engine.
///
/// # Raw Values
/// All prices are passed as raw i64 values (the internal representation
/// of `FixedDecimal`). This allows direct SIMD operations without
/// additional conversions.
pub trait SimdMatcher: Send + Sync {
    /// Find indices where buy_price >= ask_prices[i] (prices that can cross).
    ///
    /// For a buy order to match with a sell order, the buy price must be
    /// greater than or equal to the ask (sell) price.
    ///
    /// # Arguments
    /// - `buy_price`: The raw i64 price of the incoming buy order
    /// - `ask_prices`: Slice of raw i64 ask prices from the order book
    ///
    /// # Returns
    /// Vector of indices where crossing is possible
    fn find_crossing_buy_prices(&self, buy_price: i64, ask_prices: &[i64]) -> Vec<usize>;

    /// Find indices where sell_price <= bid_prices[i] (prices that can cross).
    ///
    /// For a sell order to match with a buy order, the sell price must be
    /// less than or equal to the bid (buy) price.
    ///
    /// # Arguments
    /// - `sell_price`: The raw i64 price of the incoming sell order
    /// - `bid_prices`: Slice of raw i64 bid prices from the order book
    ///
    /// # Returns
    /// Vector of indices where crossing is possible
    fn find_crossing_sell_prices(&self, sell_price: i64, bid_prices: &[i64]) -> Vec<usize>;

    /// Get the name of this SIMD implementation.
    ///
    /// Used for logging, debugging, and benchmarking.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementation for testing the trait
    struct MockMatcher;

    impl SimdMatcher for MockMatcher {
        fn find_crossing_buy_prices(&self, buy_price: i64, ask_prices: &[i64]) -> Vec<usize> {
            ask_prices
                .iter()
                .enumerate()
                .filter_map(|(i, &ask)| if buy_price >= ask { Some(i) } else { None })
                .collect()
        }

        fn find_crossing_sell_prices(&self, sell_price: i64, bid_prices: &[i64]) -> Vec<usize> {
            bid_prices
                .iter()
                .enumerate()
                .filter_map(|(i, &bid)| if sell_price <= bid { Some(i) } else { None })
                .collect()
        }

        fn name(&self) -> &'static str {
            "Mock"
        }
    }

    #[test]
    fn test_trait_can_be_implemented() {
        let matcher = MockMatcher;
        assert_eq!(matcher.name(), "Mock");
    }

    #[test]
    fn test_mock_buy_crossing() {
        let matcher = MockMatcher;
        let asks = vec![100, 110, 120, 130];
        let buy = 115;

        let result = matcher.find_crossing_buy_prices(buy, &asks);
        assert_eq!(result, vec![0, 1]); // 115 >= 100, 115 >= 110
    }

    #[test]
    fn test_mock_sell_crossing() {
        let matcher = MockMatcher;
        let bids = vec![130, 120, 110, 100];
        let sell = 115;

        let result = matcher.find_crossing_sell_prices(sell, &bids);
        assert_eq!(result, vec![0, 1]); // 115 <= 130, 115 <= 120
    }
}
