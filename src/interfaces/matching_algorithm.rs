// ============================================================================
// Matching Algorithm Interface
// Defines the contract for pluggable matching algorithms
// ============================================================================

use crate::domain::{Order, OrderBookSide, Trade};
use std::sync::Arc;

/// Strategy pattern interface for matching algorithms
/// Implementations: PriceTime (FIFO), ProRata, SizeProRata, LMM Priority, etc.
pub trait MatchingAlgorithm: Send + Sync {
    /// Match an incoming order against the opposite side of the book
    ///
    /// # Arguments
    /// * `incoming_order` - The new order to match
    /// * `opposite_side` - The opposite side of the order book
    ///
    /// # Returns
    /// Vector of trades generated from matching
    fn match_order(&self, incoming_order: Arc<Order>, opposite_side: &OrderBookSide) -> Vec<Trade>;

    /// Get the algorithm name for logging/metrics
    fn name(&self) -> &str;

    /// Optional: Check if two prices can cross
    /// Default implementation handles buy/sell logic
    fn prices_cross(&self, incoming: &Order, book_price: rust_decimal::Decimal) -> bool {
        use crate::domain::Side;
        use rust_decimal::Decimal;

        let incoming_price = incoming.price.unwrap_or(match incoming.side {
            Side::Buy => Decimal::MAX,
            Side::Sell => Decimal::ZERO,
        });

        match incoming.side {
            Side::Buy => incoming_price >= book_price,
            Side::Sell => incoming_price <= book_price,
        }
    }
}

/// Configuration for matching algorithms
#[derive(Debug, Clone)]
pub struct MatchingConfig {
    /// Minimum quantity for pro-rata allocation
    pub min_quantity: rust_decimal::Decimal,

    /// Whether to use SIMD optimizations
    pub use_simd: bool,

    /// Whether to give priority to top-of-book order
    pub top_of_book_fifo: bool,

    /// Lead market maker accounts (for LMM priority)
    pub lmm_accounts: Vec<String>,

    /// LMM allocation percentage (e.g., 0.4 for 40%)
    pub lmm_allocation_pct: rust_decimal::Decimal,
}

impl Default for MatchingConfig {
    fn default() -> Self {
        use rust_decimal::Decimal;

        Self {
            min_quantity: Decimal::ZERO,
            use_simd: true,
            top_of_book_fifo: false,
            lmm_accounts: Vec::new(),
            lmm_allocation_pct: Decimal::ZERO,
        }
    }
}
