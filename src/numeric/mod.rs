// ============================================================================
// Numeric Module
// Fixed-point arithmetic for high-performance financial calculations
// ============================================================================
//
// This module provides:
// - FixedDecimal<D>: Fixed-point decimal with compile-time precision
// - NumericError: Error types for arithmetic operations
// - Price/Quantity type aliases for common use cases
//
// Design principles:
// - No floating-point operations
// - All arithmetic returns Result (no panics)
// - SIMD-friendly internal representation (i64)
// - Compile-time configurable precision via const generics

mod errors;
mod fixed_decimal;

pub use errors::{NumericError, NumericResult};
pub use fixed_decimal::{FixedDecimal, Price, Quantity};
