// ============================================================================
// Order Book Factory
// Creates matching engines with proper configuration
// ============================================================================

use crate::domain::config::{MatchingAlgorithmType, OrderBookConfig, OrderBookType};
use crate::engine::{
    LmmPriority, MatchingEngine, PriceTimePriority, ProRata, ProRataTobFifo, ThresholdProRata,
};
use crate::interfaces::{EventHandler, MatchingAlgorithm};
use std::sync::Arc;

// ============================================================================
// Factory Functions
// ============================================================================

/// Creates a matching engine from configuration
///
/// # Arguments
/// * `config` - Order book configuration
/// * `event_handler` - Event handler for order and trade events
///
/// # Returns
/// * `Result<MatchingEngine, String>` - Configured matching engine or error
///
/// # Example
/// ```
/// use matching_engine::prelude::*;
/// use matching_engine::engine::factory::create_from_config;
/// use std::sync::Arc;
///
/// let config = OrderBookConfig::nasdaq_style("AAPL".to_string());
/// let engine = create_from_config(config, Arc::new(NoOpEventHandler)).unwrap();
/// ```
pub fn create_from_config(
    config: OrderBookConfig,
    event_handler: Arc<dyn EventHandler>,
) -> Result<MatchingEngine, String> {
    // Validate configuration first
    config.validate()?;

    // Create the matching algorithm based on configuration
    let algorithm = create_matching_algorithm(&config.matching_algorithm)?;

    // Create the matching engine
    let engine = MatchingEngine::new(config.instrument.clone(), algorithm, event_handler);

    // Note: Dark pool visibility is handled at the snapshot/query level
    // The order book type is stored in the config but enforcement happens
    // when clients request order book data

    Ok(engine)
}

/// Creates the appropriate matching algorithm from configuration
fn create_matching_algorithm(
    algo_type: &MatchingAlgorithmType,
) -> Result<Box<dyn MatchingAlgorithm>, String> {
    match algo_type {
        MatchingAlgorithmType::PriceTime { use_simd } => {
            Ok(Box::new(PriceTimePriority::new(*use_simd)))
        }

        MatchingAlgorithmType::ProRata {
            minimum_quantity,
            top_of_book_fifo,
        } => Ok(Box::new(ProRata::new(*minimum_quantity, *top_of_book_fifo))),

        MatchingAlgorithmType::ProRataTobFifo { minimum_quantity } => {
            Ok(Box::new(ProRataTobFifo::new(*minimum_quantity)))
        }

        MatchingAlgorithmType::LmmPriority {
            lmm_accounts,
            lmm_allocation_pct,
            minimum_quantity,
        } => Ok(Box::new(LmmPriority::new(
            lmm_accounts.iter().cloned().collect(),
            *lmm_allocation_pct,
            *minimum_quantity,
        ))),

        MatchingAlgorithmType::ThresholdProRata {
            threshold,
            minimum_quantity,
        } => Ok(Box::new(ThresholdProRata::new(*threshold, *minimum_quantity))),
    }
}

// ============================================================================
// Builder Pattern for Advanced Configuration
// ============================================================================

/// Builder for creating matching engines with fluent API
///
/// # Example
/// ```
/// use matching_engine::prelude::*;
/// use matching_engine::engine::factory::MatchingEngineBuilder;
/// use std::sync::Arc;
/// use rust_decimal::Decimal;
///
/// let engine = MatchingEngineBuilder::new("BTC-USD")
///     .transparent_order_book()
///     .price_time_matching(true)
///     .with_tick_size(Decimal::new(1, 2))
///     .build(Arc::new(NoOpEventHandler))
///     .unwrap();
/// ```
pub struct MatchingEngineBuilder {
    config: OrderBookConfig,
}

impl MatchingEngineBuilder {
    /// Create a new builder for the specified instrument
    pub fn new(instrument: impl Into<String>) -> Self {
        Self {
            config: OrderBookConfig::new(
                instrument.into(),
                OrderBookType::Transparent,
                MatchingAlgorithmType::PriceTime { use_simd: true },
            ),
        }
    }

    // ========================================================================
    // Order Book Type Configuration
    // ========================================================================

    /// Configure as a transparent order book (default)
    pub fn transparent_order_book(mut self) -> Self {
        self.config.order_book_type = OrderBookType::Transparent;
        self
    }

    /// Configure as a dark pool
    pub fn dark_pool(mut self) -> Self {
        self.config.order_book_type = OrderBookType::DarkPool;
        self
    }

    /// Configure as a hybrid order book
    pub fn hybrid_order_book(mut self) -> Self {
        self.config.order_book_type = OrderBookType::Hybrid;
        self
    }

    // ========================================================================
    // Matching Algorithm Configuration
    // ========================================================================

    /// Configure price/time priority (FIFO) matching
    pub fn price_time_matching(mut self, use_simd: bool) -> Self {
        self.config.matching_algorithm = MatchingAlgorithmType::PriceTime { use_simd };
        self
    }

    /// Configure pro-rata matching
    pub fn pro_rata_matching(
        mut self,
        minimum_quantity: rust_decimal::Decimal,
        top_of_book_fifo: bool,
    ) -> Self {
        self.config.matching_algorithm = MatchingAlgorithmType::ProRata {
            minimum_quantity,
            top_of_book_fifo,
        };
        self
    }

    /// Configure pro-rata with top-of-book FIFO
    pub fn pro_rata_tob_fifo_matching(mut self, minimum_quantity: rust_decimal::Decimal) -> Self {
        self.config.matching_algorithm = MatchingAlgorithmType::ProRataTobFifo { minimum_quantity };
        self
    }

    /// Configure LMM priority matching
    pub fn lmm_priority_matching(
        mut self,
        lmm_accounts: std::collections::HashSet<String>,
        lmm_allocation_pct: rust_decimal::Decimal,
        minimum_quantity: rust_decimal::Decimal,
    ) -> Self {
        self.config.matching_algorithm = MatchingAlgorithmType::LmmPriority {
            lmm_accounts,
            lmm_allocation_pct,
            minimum_quantity,
        };
        self
    }

    /// Configure threshold pro-rata matching
    pub fn threshold_pro_rata_matching(
        mut self,
        threshold: rust_decimal::Decimal,
        minimum_quantity: rust_decimal::Decimal,
    ) -> Self {
        self.config.matching_algorithm = MatchingAlgorithmType::ThresholdProRata {
            threshold,
            minimum_quantity,
        };
        self
    }

    // ========================================================================
    // Additional Configuration
    // ========================================================================

    /// Set maximum order book depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.config.max_depth = Some(depth);
        self
    }

    /// Set price tick size
    pub fn with_tick_size(mut self, tick_size: rust_decimal::Decimal) -> Self {
        self.config.tick_size = Some(tick_size);
        self
    }

    /// Set lot size
    pub fn with_lot_size(mut self, lot_size: rust_decimal::Decimal) -> Self {
        self.config.lot_size = Some(lot_size);
        self
    }

    // ========================================================================
    // Preset Configurations
    // ========================================================================

    /// Apply NASDAQ-style configuration
    pub fn nasdaq_style(instrument: impl Into<String>) -> Self {
        Self {
            config: OrderBookConfig::nasdaq_style(instrument.into()),
        }
    }

    /// Apply CME-style configuration
    pub fn cme_style(instrument: impl Into<String>, minimum_quantity: rust_decimal::Decimal) -> Self {
        Self {
            config: OrderBookConfig::cme_style(instrument.into(), minimum_quantity),
        }
    }

    /// Apply Eurex-style configuration
    pub fn eurex_style(instrument: impl Into<String>, minimum_quantity: rust_decimal::Decimal) -> Self {
        Self {
            config: OrderBookConfig::eurex_style(instrument.into(), minimum_quantity),
        }
    }

    /// Apply dark pool configuration
    pub fn dark_pool_preset(instrument: impl Into<String>) -> Self {
        Self {
            config: OrderBookConfig::dark_pool(instrument.into()),
        }
    }

    // ========================================================================
    // Build
    // ========================================================================

    /// Build the matching engine
    pub fn build(self, event_handler: Arc<dyn EventHandler>) -> Result<MatchingEngine, String> {
        create_from_config(self.config, event_handler)
    }

    /// Get the configuration without building (for inspection)
    pub fn get_config(&self) -> &OrderBookConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interfaces::NoOpEventHandler;
    use rust_decimal::Decimal;
    use std::collections::HashSet;

    #[test]
    fn test_create_price_time_engine() {
        let config = OrderBookConfig::nasdaq_style("AAPL".to_string());
        let engine = create_from_config(config, Arc::new(NoOpEventHandler)).unwrap();
        assert_eq!(engine.get_instrument(), "AAPL");
    }

    #[test]
    fn test_create_pro_rata_engine() {
        let config = OrderBookConfig::cme_style("ES".to_string(), Decimal::from(10));
        let engine = create_from_config(config, Arc::new(NoOpEventHandler)).unwrap();
        assert_eq!(engine.get_instrument(), "ES");
    }

    #[test]
    fn test_create_dark_pool_engine() {
        let config = OrderBookConfig::dark_pool("DARK-POOL".to_string());
        let engine = create_from_config(config, Arc::new(NoOpEventHandler)).unwrap();
        assert_eq!(engine.get_instrument(), "DARK-POOL");
    }

    #[test]
    fn test_builder_pattern() {
        let engine = MatchingEngineBuilder::new("BTC-USD")
            .transparent_order_book()
            .price_time_matching(true)
            .with_tick_size(Decimal::new(1, 2))
            .build(Arc::new(NoOpEventHandler))
            .unwrap();

        assert_eq!(engine.get_instrument(), "BTC-USD");
    }

    #[test]
    fn test_builder_pro_rata() {
        let engine = MatchingEngineBuilder::new("ETH-USD")
            .pro_rata_matching(Decimal::from(5), false)
            .build(Arc::new(NoOpEventHandler))
            .unwrap();

        assert_eq!(engine.get_instrument(), "ETH-USD");
    }

    #[test]
    fn test_builder_dark_pool() {
        let engine = MatchingEngineBuilder::new("BLOCK-TRADE")
            .dark_pool()
            .price_time_matching(true)
            .build(Arc::new(NoOpEventHandler))
            .unwrap();

        assert_eq!(engine.get_instrument(), "BLOCK-TRADE");
    }

    #[test]
    fn test_builder_lmm_priority() {
        let mut lmm_accounts = HashSet::new();
        lmm_accounts.insert("lmm1".to_string());
        lmm_accounts.insert("lmm2".to_string());

        let engine = MatchingEngineBuilder::new("BTC-USD")
            .lmm_priority_matching(lmm_accounts, Decimal::new(4, 1), Decimal::from(10))
            .build(Arc::new(NoOpEventHandler))
            .unwrap();

        assert_eq!(engine.get_instrument(), "BTC-USD");
    }

    #[test]
    fn test_preset_builders() {
        let nasdaq = MatchingEngineBuilder::nasdaq_style("AAPL")
            .build(Arc::new(NoOpEventHandler))
            .unwrap();
        assert_eq!(nasdaq.get_instrument(), "AAPL");

        let dark = MatchingEngineBuilder::dark_pool_preset("DARK")
            .build(Arc::new(NoOpEventHandler))
            .unwrap();
        assert_eq!(dark.get_instrument(), "DARK");
    }
}
