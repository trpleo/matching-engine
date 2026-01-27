// ============================================================================
// Utilities Module
// Helper functions and utilities for performance optimization
// ============================================================================

mod numa_detection;

// Re-export NUMA utilities
pub use numa_detection::{
    get_available_cores, pin_current_thread_to_core, pin_current_thread_to_node, NumaNode,
    NumaTopology,
};
