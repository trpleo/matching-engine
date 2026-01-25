// ============================================================================
// SIMD Optimizations Module
// Platform-specific SIMD implementations for price matching
//
// Supported architectures:
// - x86_64: AVX-512 (512-bit, 8x i64) or AVX2 (256-bit, 4x i64)
// - aarch64: NEON (128-bit, 2x i64)
// - Other: Scalar fallback
//
// Usage:
// ```ignore
// use matching_engine::simd::{create_simd_matcher, SimdMatcher, CpuCapabilities};
//
// // Detect CPU at startup
// let caps = CpuCapabilities::detect();
// println!("Running on: {}", caps);
//
// // Create optimal matcher
// let matcher = create_simd_matcher();
// let crossing = matcher.find_crossing_buy_prices(price, &ask_prices);
// ```
// ============================================================================

mod detector;
mod scalar;
mod traits;

#[cfg(target_arch = "aarch64")]
mod neon;

#[cfg(target_arch = "x86_64")]
mod avx2;

#[cfg(target_arch = "x86_64")]
mod avx512;

// Public exports
pub use detector::{
    create_scalar_matcher, create_simd_matcher, Architecture, CpuCapabilities, SimdLevel,
};
pub use scalar::ScalarMatcher;
pub use traits::SimdMatcher;

#[cfg(target_arch = "aarch64")]
pub use neon::NeonMatcher;

#[cfg(target_arch = "x86_64")]
pub use avx2::Avx2Matcher;

#[cfg(target_arch = "x86_64")]
pub use avx512::Avx512Matcher;

// Keep old export for backwards compatibility during migration
// TODO: Remove after migration is complete
#[deprecated(since = "0.2.0", note = "Use create_simd_matcher() instead")]
pub use scalar::ScalarMatcher as SimdPriceMatcher;
