// ============================================================================
// Order Book Configuration
// Comprehensive configuration for order book type and matching behavior
// ============================================================================

use rust_decimal::Decimal;
use std::collections::HashSet;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ============================================================================
// Order Book Type
// ============================================================================

/// Defines the type of order book and its visibility characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum OrderBookType {
    /// Standard visible limit order book (L1/L2/L3 data available)
    /// - Pre-trade transparency: Full order book visible
    /// - Post-trade transparency: All trades published
    /// - Use case: Traditional exchanges (NASDAQ, CME, etc.)
    Transparent,

    /// Dark pool - no pre-trade transparency
    /// - Pre-trade transparency: None (orders hidden)
    /// - Post-trade transparency: Trades published after execution
    /// - Use case: Institutional block trading, minimize market impact
    DarkPool,

    /// Hybrid - mixed visibility (e.g., iceberg orders, hidden quantity)
    /// - Pre-trade transparency: Partial (visible quantity only)
    /// - Post-trade transparency: All trades published
    /// - Use case: Large orders with display quantity
    Hybrid,
}

// ============================================================================
// Matching Algorithm Type
// ============================================================================

/// Defines the matching algorithm to use for order execution
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MatchingAlgorithmType {
    /// Price/Time Priority (FIFO) - First-In-First-Out at each price level
    /// Use case: Equity markets (NASDAQ, NYSE, etc.)
    PriceTime {
        /// Enable SIMD optimizations for price crossing checks
        use_simd: bool,
    },

    /// Pro-Rata - Size-proportional allocation at each price level
    /// Use case: Derivatives markets (CME, Eurex)
    ProRata {
        /// Minimum order size to participate in pro-rata allocation
        minimum_quantity: Decimal,
        /// Whether to give FIFO priority to top-of-book order
        top_of_book_fifo: bool,
    },

    /// Pro-Rata with Top-of-Book FIFO
    /// First order gets FIFO, remaining quantity distributed pro-rata
    /// Use case: Eurex, ICE Futures
    ProRataTobFifo {
        /// Minimum order size for pro-rata participation
        minimum_quantity: Decimal,
    },

    /// Lead Market Maker Priority
    /// LMMs get priority allocation, then pro-rata for remaining
    /// Use case: Exchanges with market maker programs
    LmmPriority {
        /// Set of account IDs designated as LMMs
        lmm_accounts: HashSet<String>,
        /// Percentage of incoming order allocated to LMMs first (0.0 - 1.0)
        lmm_allocation_pct: Decimal,
        /// Minimum order size for non-LMM pro-rata participation
        minimum_quantity: Decimal,
    },

    /// Threshold Pro-Rata
    /// Small orders get FIFO, large orders get pro-rata
    /// Use case: Protecting retail traders while serving institutions
    ThresholdProRata {
        /// Order size threshold (orders < threshold get FIFO)
        threshold: Decimal,
        /// Minimum size for pro-rata participation (only for large orders)
        minimum_quantity: Decimal,
    },
}

// ============================================================================
// Complete Order Book Configuration
// ============================================================================

/// Comprehensive configuration for creating an order book
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OrderBookConfig {
    /// The trading instrument (e.g., "BTC-USD", "AAPL", "ES-202503")
    pub instrument: String,

    /// Type of order book (transparency level)
    pub order_book_type: OrderBookType,

    /// Matching algorithm configuration
    pub matching_algorithm: MatchingAlgorithmType,

    /// Optional: Maximum order book depth to maintain (for memory optimization)
    /// None means unlimited depth
    pub max_depth: Option<usize>,

    /// Optional: Price tick size (minimum price increment)
    /// None means no tick size enforcement
    pub tick_size: Option<Decimal>,

    /// Optional: Lot size (minimum quantity increment)
    /// None means no lot size enforcement
    pub lot_size: Option<Decimal>,
}

impl OrderBookConfig {
    /// Create a new configuration with required parameters
    pub fn new(
        instrument: String,
        order_book_type: OrderBookType,
        matching_algorithm: MatchingAlgorithmType,
    ) -> Self {
        Self {
            instrument,
            order_book_type,
            matching_algorithm,
            max_depth: None,
            tick_size: None,
            lot_size: None,
        }
    }

    /// Builder method: Set maximum order book depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Builder method: Set price tick size
    pub fn with_tick_size(mut self, tick: Decimal) -> Self {
        self.tick_size = Some(tick);
        self
    }

    /// Builder method: Set lot size
    pub fn with_lot_size(mut self, lot: Decimal) -> Self {
        self.lot_size = Some(lot);
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate instrument name
        if self.instrument.is_empty() {
            return Err("Instrument cannot be empty".to_string());
        }

        // Validate tick size
        if let Some(tick) = self.tick_size {
            if tick <= Decimal::ZERO {
                return Err("Tick size must be positive".to_string());
            }
        }

        // Validate lot size
        if let Some(lot) = self.lot_size {
            if lot <= Decimal::ZERO {
                return Err("Lot size must be positive".to_string());
            }
        }

        // Validate matching algorithm parameters
        match &self.matching_algorithm {
            MatchingAlgorithmType::ProRata {
                minimum_quantity, ..
            } => {
                if *minimum_quantity < Decimal::ZERO {
                    return Err("Minimum quantity cannot be negative".to_string());
                }
            }
            MatchingAlgorithmType::ProRataTobFifo { minimum_quantity } => {
                if *minimum_quantity < Decimal::ZERO {
                    return Err("Minimum quantity cannot be negative".to_string());
                }
            }
            MatchingAlgorithmType::LmmPriority {
                lmm_allocation_pct,
                minimum_quantity,
                ..
            } => {
                if *lmm_allocation_pct < Decimal::ZERO || *lmm_allocation_pct > Decimal::ONE {
                    return Err("LMM allocation percentage must be between 0 and 1".to_string());
                }
                if *minimum_quantity < Decimal::ZERO {
                    return Err("Minimum quantity cannot be negative".to_string());
                }
            }
            MatchingAlgorithmType::ThresholdProRata {
                threshold,
                minimum_quantity,
            } => {
                if *threshold <= Decimal::ZERO {
                    return Err("Threshold must be positive".to_string());
                }
                if *minimum_quantity < Decimal::ZERO {
                    return Err("Minimum quantity cannot be negative".to_string());
                }
            }
            _ => {}
        }

        Ok(())
    }
}

// ============================================================================
// Preset Configurations (Factory Methods)
// ============================================================================

impl OrderBookConfig {
    /// NASDAQ-style configuration
    /// - Transparent order book
    /// - Price/Time priority (FIFO)
    /// - Tick size: $0.01
    pub fn nasdaq_style(instrument: String) -> Self {
        Self::new(
            instrument,
            OrderBookType::Transparent,
            MatchingAlgorithmType::PriceTime { use_simd: true },
        )
        .with_tick_size(Decimal::new(1, 2)) // $0.01
    }

    /// CME-style futures configuration
    /// - Transparent order book
    /// - Pro-rata matching
    /// - Configurable minimum quantity
    pub fn cme_style(instrument: String, minimum_quantity: Decimal) -> Self {
        Self::new(
            instrument,
            OrderBookType::Transparent,
            MatchingAlgorithmType::ProRata {
                minimum_quantity,
                top_of_book_fifo: false,
            },
        )
    }

    /// Eurex-style futures configuration
    /// - Transparent order book
    /// - Pro-rata with top-of-book FIFO
    pub fn eurex_style(instrument: String, minimum_quantity: Decimal) -> Self {
        Self::new(
            instrument,
            OrderBookType::Transparent,
            MatchingAlgorithmType::ProRataTobFifo { minimum_quantity },
        )
    }

    /// Dark pool configuration
    /// - Hidden order book (no pre-trade transparency)
    /// - Price/Time priority matching
    pub fn dark_pool(instrument: String) -> Self {
        Self::new(
            instrument,
            OrderBookType::DarkPool,
            MatchingAlgorithmType::PriceTime { use_simd: true },
        )
    }

    /// Crypto exchange configuration with LMM priority
    /// - Transparent order book
    /// - LMM priority matching
    pub fn crypto_with_lmm(
        instrument: String,
        lmm_accounts: HashSet<String>,
        lmm_allocation_pct: Decimal,
    ) -> Self {
        Self::new(
            instrument,
            OrderBookType::Transparent,
            MatchingAlgorithmType::LmmPriority {
                lmm_accounts,
                lmm_allocation_pct,
                minimum_quantity: Decimal::ZERO,
            },
        )
    }

    /// Retail-friendly configuration with threshold pro-rata
    /// - Transparent order book
    /// - Small orders get FIFO protection
    pub fn retail_friendly(instrument: String, threshold: Decimal) -> Self {
        Self::new(
            instrument,
            OrderBookType::Transparent,
            MatchingAlgorithmType::ThresholdProRata {
                threshold,
                minimum_quantity: Decimal::ZERO,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = OrderBookConfig::new(
            "BTC-USD".to_string(),
            OrderBookType::Transparent,
            MatchingAlgorithmType::PriceTime { use_simd: true },
        );

        assert_eq!(config.instrument, "BTC-USD");
        assert_eq!(config.order_book_type, OrderBookType::Transparent);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_builder_pattern() {
        let config = OrderBookConfig::nasdaq_style("AAPL".to_string())
            .with_max_depth(100)
            .with_lot_size(Decimal::from(1));

        assert_eq!(config.max_depth, Some(100));
        assert_eq!(config.lot_size, Some(Decimal::from(1)));
    }

    #[test]
    fn test_validation() {
        let config = OrderBookConfig::new(
            "".to_string(),
            OrderBookType::Transparent,
            MatchingAlgorithmType::PriceTime { use_simd: true },
        );

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_preset_configs() {
        let nasdaq = OrderBookConfig::nasdaq_style("AAPL".to_string());
        assert!(matches!(nasdaq.order_book_type, OrderBookType::Transparent));

        let dark = OrderBookConfig::dark_pool("BLOCK-TRADE".to_string());
        assert!(matches!(dark.order_book_type, OrderBookType::DarkPool));
    }
}
