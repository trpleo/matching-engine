// ============================================================================
// Domain Models Module
// Contains all core domain entities and value objects
// ============================================================================

pub mod config;
pub mod order;
pub mod order_book;
pub mod trade;

pub use config::{MatchingAlgorithmType, OrderBookConfig, OrderBookType};
pub use order::{Order, OrderId, OrderType, Side, TimeInForce};
pub use order_book::{OrderBookLevel, OrderBookSide, OrderBookSnapshot};
pub use trade::Trade;

// Re-export state machine
pub use order::state::{OrderState, OrderStateTransition};
