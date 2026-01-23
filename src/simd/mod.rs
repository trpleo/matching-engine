// ============================================================================
// SIMD Optimizations Module
// Platform-specific SIMD implementations for price matching
//
// Supported architectures:
// - x86_64: AVX2 (256-bit registers, 4x f64 parallel)
// - aarch64: NEON (128-bit registers, 2x f64 parallel)
// - Other: Scalar fallback
// ============================================================================

pub mod price_matcher;

pub use price_matcher::SimdPriceMatcher;
