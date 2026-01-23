// ============================================================================
// Interfaces Module
// Contains all trait definitions and contracts
// ============================================================================

mod event_handler;
mod matching_algorithm;

pub use event_handler::{EventHandler, LoggingEventHandler, NoOpEventHandler, OrderEvent};
pub use matching_algorithm::{MatchingAlgorithm, MatchingConfig};
