// ============================================================================
// Platform Module
// Hardware-specific optimizations for performance-critical operations
//
// This module contains:
// - SIMD: Vectorized price matching (AVX2, AVX-512, NEON)
// - NUMA: Non-Uniform Memory Access topology detection and CPU affinity
//
// Usage:
// ```ignore
// use matching_engine::platform::{
//     // SIMD
//     create_simd_matcher, SimdMatcher, CpuCapabilities,
//     // NUMA (requires "numa" feature)
//     NumaTopology, NumaNode, pin_current_thread_to_core,
// };
// ```
// ============================================================================

pub mod simd;

mod numa;

// Re-export SIMD types at platform level for convenience
pub use simd::{
    create_scalar_matcher, create_simd_matcher, Architecture, CpuCapabilities, ScalarMatcher,
    SimdLevel, SimdMatcher,
};

#[cfg(target_arch = "aarch64")]
pub use simd::NeonMatcher;

#[cfg(target_arch = "x86_64")]
pub use simd::Avx2Matcher;

#[cfg(all(target_arch = "x86_64", feature = "avx512"))]
pub use simd::Avx512Matcher;

// Re-export NUMA types at platform level
pub use numa::{get_available_cores, pin_current_thread_to_core, pin_current_thread_to_node};
pub use numa::{NumaNode, NumaTopology};
