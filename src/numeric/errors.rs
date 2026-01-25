// ============================================================================
// Numeric Errors
// Error types for fixed-point arithmetic operations
// ============================================================================

use std::fmt;

/// Errors that can occur during fixed-point arithmetic operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumericError {
    /// Result exceeded i64::MAX
    Overflow,
    /// Result below i64::MIN
    Underflow,
    /// Attempted division by zero
    DivisionByZero,
    /// Conversion would lose significant digits
    PrecisionLoss,
    /// Input string or value is invalid
    InvalidInput,
    /// Scale mismatch between operands
    ScaleMismatch,
}

impl fmt::Display for NumericError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumericError::Overflow => {
                write!(f, "arithmetic overflow: result exceeded maximum value")
            },
            NumericError::Underflow => {
                write!(f, "arithmetic underflow: result below minimum value")
            },
            NumericError::DivisionByZero => write!(f, "division by zero"),
            NumericError::PrecisionLoss => write!(
                f,
                "precision loss: conversion would lose significant digits"
            ),
            NumericError::InvalidInput => write!(f, "invalid input: could not parse value"),
            NumericError::ScaleMismatch => write!(f, "scale mismatch between operands"),
        }
    }
}

impl std::error::Error for NumericError {}

/// Result type alias for numeric operations
pub type NumericResult<T> = Result<T, NumericError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        assert_eq!(
            NumericError::Overflow.to_string(),
            "arithmetic overflow: result exceeded maximum value"
        );
        assert_eq!(NumericError::DivisionByZero.to_string(), "division by zero");
    }

    #[test]
    fn test_error_equality() {
        assert_eq!(NumericError::Overflow, NumericError::Overflow);
        assert_ne!(NumericError::Overflow, NumericError::Underflow);
    }
}
