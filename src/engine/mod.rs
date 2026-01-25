// ============================================================================
// Engine Module
// Contains the core matching engine business logic
// ============================================================================

mod lmm_priority;
mod matching_engine;
mod price_time;
mod pro_rata;
mod pro_rata_tob_fifo;
mod threshold_pro_rata;

pub mod factory;

pub use factory::{create_from_config, MatchingEngineBuilder};
pub use lmm_priority::LmmPriority;
pub use matching_engine::MatchingEngine;
pub use price_time::PriceTimePriority;
pub use pro_rata::ProRata;
pub use pro_rata_tob_fifo::ProRataTobFifo;
pub use threshold_pro_rata::ThresholdProRata;
