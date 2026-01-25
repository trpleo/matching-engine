// ============================================================================
// Matching Engine Library
// High-performance lock-free order matching engine with pluggable algorithms
// ============================================================================

//! # Matching Engine
//!
//! A high-performance, lock-free matching engine for financial order books.
//!
//! ## Features
//!
//! - **Lock-free concurrent data structures** using atomic operations
//! - **Pluggable matching algorithms** (Price/Time, Pro-Rata, etc.)
//! - **SIMD optimizations** for price matching (AVX2 on x86_64, NEON on aarch64)
//! - **Event sourcing** for audit trail and compliance
//! - **Sub-microsecond latency** for order matching
//!
//! ## Example
//!
//! ```rust
//! use matching_engine::prelude::*;
//! use matching_engine::numeric::{Price, Quantity};
//! use std::sync::Arc;
//!
//! // Create matching engine with Price/Time algorithm
//! let engine = MatchingEngine::new(
//!     "BTC-USD".to_string(),
//!     Box::new(PriceTimePriority::new(true)), // Enable SIMD
//!     Arc::new(NoOpEventHandler),
//! );
//!
//! // Create and submit orders
//! let sell_order = Arc::new(Order::new(
//!     "user1".to_string(),
//!     "BTC-USD".to_string(),
//!     Side::Sell,
//!     OrderType::Limit,
//!     Some(Price::from_integer(50000).unwrap()),
//!     Quantity::from_integer(1).unwrap(),
//!     TimeInForce::GoodTillCancel,
//! ));
//!
//! engine.submit_order(sell_order);
//!
//! // Get order book snapshot
//! let snapshot = engine.get_snapshot(10);
//! println!("Best bid: {:?}", snapshot.best_bid());
//! println!("Best ask: {:?}", snapshot.best_ask());
//! println!("Spread: {:?}", snapshot.spread);
//! ```

pub mod domain;
pub mod engine;
pub mod interfaces;
pub mod numeric;
pub mod simd;
pub mod utils;

// Re-exports for convenience
pub mod prelude {
    pub use crate::domain::order::state::{OrderState, OrderStateTransition};
    pub use crate::domain::{
        MatchingAlgorithmType, Order, OrderBookConfig, OrderBookSide, OrderBookSnapshot,
        OrderBookType, OrderId, OrderType, Side, TimeInForce, Trade,
    };
    pub use crate::engine::{
        create_from_config, LmmPriority, MatchingEngine, MatchingEngineBuilder, PriceTimePriority,
        ProRata, ProRataTobFifo, ThresholdProRata,
    };
    pub use crate::interfaces::{
        EventHandler, LoggingEventHandler, MatchingAlgorithm, MatchingConfig, NoOpEventHandler,
        OrderEvent,
    };
    pub use crate::simd::SimdPriceMatcher;
}

#[cfg(test)]
mod integration_tests {
    use super::prelude::*;
    use crate::numeric::{Price, Quantity};
    use std::sync::Arc;

    #[test]
    fn test_end_to_end_matching() {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(PriceTimePriority::new(false)),
            Arc::new(NoOpEventHandler),
        );

        // Add sell order
        let sell = Arc::new(Order::new(
            "seller".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let sell_events = engine.submit_order(sell.clone());
        assert!(sell_events
            .iter()
            .any(|e| matches!(e, OrderEvent::OrderAddedToBook { .. })));

        // Add matching buy order
        let buy = Arc::new(Order::new(
            "buyer".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let buy_events = engine.submit_order(buy);

        // Verify trade occurred
        assert!(buy_events
            .iter()
            .any(|e| matches!(e, OrderEvent::OrderMatched { .. })));
        assert!(buy_events
            .iter()
            .any(|e| matches!(e, OrderEvent::OrderFilled { .. })));

        // Verify book is empty
        let snapshot = engine.get_snapshot(10);
        assert_eq!(snapshot.bids.len(), 0);
        assert_eq!(snapshot.asks.len(), 0);
    }
}
