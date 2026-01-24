// ============================================================================
// Fixed-Point Decimal
// High-performance fixed-point arithmetic with compile-time precision
// ============================================================================

use super::errors::{NumericError, NumericResult};
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Add, Neg, Sub};

/// Fixed-point decimal number with compile-time precision.
///
/// Internally stores `value × 10^DECIMALS` as an i64.
///
/// # Type Parameter
/// - `DECIMALS`: Number of decimal places (0-18). Default is 9.
///
/// # Value Range
/// With DECIMALS=9 (default):
/// - Minimum: -9,223,372,036.854775808
/// - Maximum: +9,223,372,036.854775807
/// - Precision: 0.000000001 (one nano-unit)
///
/// # Example
/// ```ignore
/// use matching_engine::numeric::FixedDecimal;
///
/// let price = FixedDecimal::<9>::from_integer(100)?;  // 100.000000000
/// let qty = FixedDecimal::<9>::from_str("2.5")?;      // 2.500000000
/// let total = price.checked_mul(qty)?;                 // 250.000000000
/// ```
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct FixedDecimal<const DECIMALS: u8 = 9>(i64);

// ============================================================================
// Scale Constants
// ============================================================================

/// Compute 10^n at compile time
const fn pow10(n: u8) -> i64 {
    let mut result: i64 = 1;
    let mut i = 0;
    while i < n {
        result *= 10;
        i += 1;
    }
    result
}

impl<const D: u8> FixedDecimal<D> {
    /// The scale factor (10^DECIMALS)
    pub const SCALE: i64 = pow10(D);

    /// Half scale for rounding (SCALE / 2)
    const HALF_SCALE: i64 = pow10(D) / 2;

    /// Zero value
    pub const ZERO: Self = Self(0);

    /// One (1.0)
    pub const ONE: Self = Self(pow10(D));

    /// Maximum representable value
    pub const MAX: Self = Self(i64::MAX);

    /// Minimum representable value
    pub const MIN: Self = Self(i64::MIN);

    // ========================================================================
    // Construction
    // ========================================================================

    /// Create from raw internal representation.
    ///
    /// Use this when you already have a scaled value (e.g., from SIMD operations).
    #[inline]
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Create from an integer value.
    ///
    /// # Errors
    /// Returns `Overflow` if the value is too large to represent.
    #[inline]
    pub fn from_integer(value: i64) -> NumericResult<Self> {
        value
            .checked_mul(Self::SCALE)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    /// Create from integer and fractional parts.
    ///
    /// # Arguments
    /// - `integer`: The integer part (can be negative)
    /// - `fraction`: The fractional part (must be < SCALE, always positive)
    ///
    /// # Example
    /// ```ignore
    /// // Create 123.456 with 9 decimals
    /// let x = FixedDecimal::<9>::from_parts(123, 456_000_000)?;
    /// ```
    #[inline]
    pub fn from_parts(integer: i64, fraction: u64) -> NumericResult<Self> {
        if fraction >= Self::SCALE as u64 {
            return Err(NumericError::InvalidInput);
        }

        let int_scaled = integer
            .checked_mul(Self::SCALE)
            .ok_or(NumericError::Overflow)?;

        let frac_signed = if integer < 0 {
            -(fraction as i64)
        } else {
            fraction as i64
        };

        int_scaled
            .checked_add(frac_signed)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    // ========================================================================
    // Accessors
    // ========================================================================

    /// Get the raw internal value (scaled).
    ///
    /// This is the value × 10^DECIMALS. Use this for SIMD operations.
    #[inline]
    pub const fn raw_value(self) -> i64 {
        self.0
    }

    /// Get the integer part (truncated toward zero).
    #[inline]
    pub const fn integer_part(self) -> i64 {
        self.0 / Self::SCALE
    }

    /// Get the fractional part as a positive value.
    #[inline]
    pub const fn fractional_part(self) -> u64 {
        (self.0 % Self::SCALE).unsigned_abs()
    }

    /// Check if value is zero.
    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Check if value is positive.
    #[inline]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// Check if value is negative.
    #[inline]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Get absolute value.
    #[inline]
    pub fn abs(self) -> NumericResult<Self> {
        if self.0 == i64::MIN {
            Err(NumericError::Overflow)
        } else {
            Ok(Self(self.0.abs()))
        }
    }

    // ========================================================================
    // Arithmetic Operations
    // ========================================================================

    /// Checked addition.
    ///
    /// # Errors
    /// Returns `Overflow` or `Underflow` if the result is out of range.
    #[inline]
    pub fn checked_add(self, rhs: Self) -> NumericResult<Self> {
        self.0.checked_add(rhs.0).map(Self).ok_or_else(|| {
            if rhs.0 > 0 {
                NumericError::Overflow
            } else {
                NumericError::Underflow
            }
        })
    }

    /// Checked subtraction.
    ///
    /// # Errors
    /// Returns `Overflow` or `Underflow` if the result is out of range.
    #[inline]
    pub fn checked_sub(self, rhs: Self) -> NumericResult<Self> {
        self.0.checked_sub(rhs.0).map(Self).ok_or_else(|| {
            if rhs.0 < 0 {
                NumericError::Overflow
            } else {
                NumericError::Underflow
            }
        })
    }

    /// Checked multiplication with round half-up.
    ///
    /// Uses i128 intermediate to prevent overflow during calculation,
    /// then rounds and scales back to i64.
    ///
    /// # Errors
    /// Returns `Overflow` or `Underflow` if the result is out of range.
    #[inline]
    pub fn checked_mul(self, rhs: Self) -> NumericResult<Self> {
        let scale = Self::SCALE as i128;
        let half_scale = Self::HALF_SCALE as i128;
        let product = (self.0 as i128) * (rhs.0 as i128);

        // Round half-up: add half scale before dividing (adjust sign for negative)
        let rounded = if product >= 0 {
            product + half_scale
        } else {
            product - half_scale
        };

        let result = rounded / scale;

        if result > i64::MAX as i128 {
            Err(NumericError::Overflow)
        } else if result < i64::MIN as i128 {
            Err(NumericError::Underflow)
        } else {
            Ok(Self(result as i64))
        }
    }

    /// Multiply by an integer (no scaling needed).
    ///
    /// More efficient than `checked_mul` when multiplying by a whole number.
    #[inline]
    pub fn checked_mul_int(self, rhs: i64) -> NumericResult<Self> {
        self.0
            .checked_mul(rhs)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    // ========================================================================
    // Comparison
    // ========================================================================

    /// Compare two values.
    #[inline]
    pub fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }

    /// Returns the minimum of two values.
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// Returns the maximum of two values.
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }
}

// ============================================================================
// Trait Implementations
// ============================================================================

impl<const D: u8> Default for FixedDecimal<D> {
    #[inline]
    fn default() -> Self {
        Self::ZERO
    }
}

impl<const D: u8> PartialEq for FixedDecimal<D> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<const D: u8> Eq for FixedDecimal<D> {}

impl<const D: u8> PartialOrd for FixedDecimal<D> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.0.cmp(&other.0))
    }
}

impl<const D: u8> Ord for FixedDecimal<D> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<const D: u8> Hash for FixedDecimal<D> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<const D: u8> Neg for FixedDecimal<D> {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

// Infallible Add/Sub for ergonomics (panics on overflow - use checked_* in production)
impl<const D: u8> Add for FixedDecimal<D> {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        self.checked_add(rhs).expect("FixedDecimal addition overflow")
    }
}

impl<const D: u8> Sub for FixedDecimal<D> {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        self.checked_sub(rhs).expect("FixedDecimal subtraction overflow")
    }
}

// ============================================================================
// Display and Debug
// ============================================================================

impl<const D: u8> fmt::Debug for FixedDecimal<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FixedDecimal<{}>({}, raw={})", D, self, self.0)
    }
}

impl<const D: u8> fmt::Display for FixedDecimal<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let int_part = self.integer_part();
        let frac_part = self.fractional_part();

        if D == 0 {
            write!(f, "{}", int_part)
        } else if self.0 < 0 && int_part == 0 {
            // Handle -0.xxx case
            write!(f, "-0.{:0>width$}", frac_part, width = D as usize)
        } else {
            write!(f, "{}.{:0>width$}", int_part, frac_part, width = D as usize)
        }
    }
}

// ============================================================================
// Conversion from rust_decimal (for API boundaries)
// ============================================================================

impl<const D: u8> FixedDecimal<D> {
    /// Convert from rust_decimal::Decimal.
    ///
    /// This is intended for API boundaries only (parsing user input).
    /// The conversion normalizes the scale to match DECIMALS.
    ///
    /// # Errors
    /// - `PrecisionLoss` if significant digits would be lost
    /// - `Overflow` if the value is too large
    pub fn from_decimal(d: rust_decimal::Decimal) -> NumericResult<Self> {
        use rust_decimal::prelude::ToPrimitive;

        // Get the scale (number of decimal places in the Decimal)
        let decimal_scale = d.scale();
        let target_scale = D as u32;

        // Multiply to get the raw integer representation at target scale
        let multiplier = rust_decimal::Decimal::from(Self::SCALE);
        let scaled = d * multiplier;

        // Convert to i64
        let raw = scaled.to_i64().ok_or(NumericError::Overflow)?;

        // Check for precision loss: if decimal has more precision than target
        if decimal_scale > target_scale {
            // Reconstruct and compare
            let reconstructed = rust_decimal::Decimal::from(raw)
                / rust_decimal::Decimal::from(Self::SCALE);
            if reconstructed != d {
                return Err(NumericError::PrecisionLoss);
            }
        }

        Ok(Self(raw))
    }

    /// Convert to rust_decimal::Decimal.
    ///
    /// This is intended for display/debugging only.
    pub fn to_decimal(self) -> rust_decimal::Decimal {
        let mut d = rust_decimal::Decimal::from(self.0);
        d.set_scale(D as u32).expect("valid scale");
        d
    }
}

// ============================================================================
// String Parsing
// ============================================================================

impl<const D: u8> std::str::FromStr for FixedDecimal<D> {
    type Err = NumericError;

    /// Parse from a decimal string.
    ///
    /// # Examples
    /// - "123" -> 123.000000000
    /// - "123.456" -> 123.456000000
    /// - "-0.001" -> -0.001000000
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(NumericError::InvalidInput);
        }

        // Check for negative
        let (is_negative, s) = if let Some(rest) = s.strip_prefix('-') {
            (true, rest)
        } else {
            (false, s)
        };

        // Split on decimal point
        let (int_str, frac_str) = if let Some(pos) = s.find('.') {
            (&s[..pos], Some(&s[pos + 1..]))
        } else {
            (s, None)
        };

        // Parse integer part
        let int_val: i64 = if int_str.is_empty() {
            0
        } else {
            int_str.parse().map_err(|_| NumericError::InvalidInput)?
        };

        // Parse fractional part
        let frac_val: u64 = if let Some(frac) = frac_str {
            if frac.is_empty() {
                0
            } else if frac.len() > D as usize {
                return Err(NumericError::PrecisionLoss);
            } else {
                // Pad with zeros to reach DECIMALS length
                let padded = format!("{:0<width$}", frac, width = D as usize);
                padded.parse().map_err(|_| NumericError::InvalidInput)?
            }
        } else {
            0
        };

        // Combine
        let mut result = Self::from_parts(int_val, frac_val)?;
        if is_negative {
            result = -result;
        }

        Ok(result)
    }
}

// ============================================================================
// Type Aliases for Common Use Cases
// ============================================================================

/// Price with 9 decimal places (nano-precision)
pub type Price = FixedDecimal<9>;

/// Quantity with 9 decimal places
pub type Quantity = FixedDecimal<9>;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    type FD9 = FixedDecimal<9>;

    #[test]
    fn test_constants() {
        assert_eq!(FD9::SCALE, 1_000_000_000);
        assert_eq!(FD9::ZERO.raw_value(), 0);
        assert_eq!(FD9::ONE.raw_value(), 1_000_000_000);
    }

    #[test]
    fn test_from_integer() {
        let x = FD9::from_integer(100).unwrap();
        assert_eq!(x.raw_value(), 100_000_000_000);
        assert_eq!(x.integer_part(), 100);
        assert_eq!(x.fractional_part(), 0);
    }

    #[test]
    fn test_from_parts() {
        // 123.456
        let x = FD9::from_parts(123, 456_000_000).unwrap();
        assert_eq!(x.integer_part(), 123);
        assert_eq!(x.fractional_part(), 456_000_000);
        assert_eq!(x.to_string(), "123.456000000");

        // -5.5
        let y = FD9::from_parts(-5, 500_000_000).unwrap();
        assert_eq!(y.integer_part(), -5);
        assert_eq!(y.fractional_part(), 500_000_000);
        assert!(y.is_negative());
    }

    #[test]
    fn test_from_parts_invalid() {
        // Fraction >= SCALE should fail
        let result = FD9::from_parts(1, 1_000_000_000);
        assert_eq!(result, Err(NumericError::InvalidInput));
    }

    #[test]
    fn test_checked_add() {
        let a = FD9::from_integer(100).unwrap();
        let b = FD9::from_integer(50).unwrap();
        let c = a.checked_add(b).unwrap();
        assert_eq!(c.integer_part(), 150);

        // Overflow
        let max = FD9::MAX;
        let result = max.checked_add(FD9::ONE);
        assert_eq!(result, Err(NumericError::Overflow));
    }

    #[test]
    fn test_checked_sub() {
        let a = FD9::from_integer(100).unwrap();
        let b = FD9::from_integer(30).unwrap();
        let c = a.checked_sub(b).unwrap();
        assert_eq!(c.integer_part(), 70);

        // Negative result
        let d = b.checked_sub(a).unwrap();
        assert_eq!(d.integer_part(), -70);

        // Underflow
        let min = FD9::MIN;
        let result = min.checked_sub(FD9::ONE);
        assert_eq!(result, Err(NumericError::Underflow));
    }

    #[test]
    fn test_checked_mul() {
        // 2.5 * 4.0 = 10.0
        let a = FD9::from_parts(2, 500_000_000).unwrap();
        let b = FD9::from_integer(4).unwrap();
        let c = a.checked_mul(b).unwrap();
        assert_eq!(c.integer_part(), 10);
        assert_eq!(c.fractional_part(), 0);

        // 1.5 * 1.5 = 2.25
        let x = FD9::from_parts(1, 500_000_000).unwrap();
        let y = x.checked_mul(x).unwrap();
        assert_eq!(y.integer_part(), 2);
        assert_eq!(y.fractional_part(), 250_000_000);
    }

    #[test]
    fn test_checked_mul_rounding() {
        // Test round half-up: 0.333333333 * 3 should round
        let third = FD9::from_raw(333_333_333); // ~0.333333333
        let three = FD9::from_integer(3).unwrap();
        let result = third.checked_mul(three).unwrap();
        // 333_333_333 * 3_000_000_000 / 1_000_000_000 = 999_999_999
        assert_eq!(result.raw_value(), 999_999_999);
    }

    #[test]
    fn test_checked_mul_overflow() {
        let large = FD9::from_integer(1_000_000_000).unwrap();
        let result = large.checked_mul(large);
        assert_eq!(result, Err(NumericError::Overflow));
    }

    #[test]
    fn test_comparison() {
        let a = FD9::from_integer(100).unwrap();
        let b = FD9::from_integer(50).unwrap();

        assert!(a > b);
        assert!(b < a);
        assert_eq!(a, a);
        assert_ne!(a, b);
        assert_eq!(a.min(b), b);
        assert_eq!(a.max(b), a);
    }

    #[test]
    fn test_display() {
        let x = FD9::from_parts(123, 456_000_000).unwrap();
        assert_eq!(x.to_string(), "123.456000000");

        let y = FD9::from_integer(0).unwrap();
        assert_eq!(y.to_string(), "0.000000000");

        let z = FD9::from_parts(0, 100_000_000).unwrap();
        assert_eq!(z.to_string(), "0.100000000");

        let neg = -FD9::from_parts(0, 100_000_000).unwrap();
        assert_eq!(neg.to_string(), "-0.100000000");
    }

    #[test]
    fn test_from_str() {
        let x: FD9 = "123.456".parse().unwrap();
        assert_eq!(x.integer_part(), 123);
        assert_eq!(x.fractional_part(), 456_000_000);

        let y: FD9 = "-0.001".parse().unwrap();
        assert!(y.is_negative());
        assert_eq!(y.fractional_part(), 1_000_000);

        let z: FD9 = "42".parse().unwrap();
        assert_eq!(z.integer_part(), 42);
        assert_eq!(z.fractional_part(), 0);
    }

    #[test]
    fn test_from_str_invalid() {
        let result: Result<FD9, _> = "not_a_number".parse();
        assert_eq!(result, Err(NumericError::InvalidInput));

        // Too many decimals
        let result: Result<FD9, _> = "1.1234567890".parse(); // 10 decimals
        assert_eq!(result, Err(NumericError::PrecisionLoss));
    }

    #[test]
    fn test_from_decimal() {
        use rust_decimal::Decimal;

        let d = Decimal::new(12345, 2); // 123.45
        let x = FD9::from_decimal(d).unwrap();
        assert_eq!(x.integer_part(), 123);
        assert_eq!(x.fractional_part(), 450_000_000);
    }

    #[test]
    fn test_to_decimal() {
        let x = FD9::from_parts(123, 456_000_000).unwrap();
        let d = x.to_decimal();
        assert_eq!(d.to_string(), "123.456000000");
    }

    #[test]
    fn test_negation() {
        let x = FD9::from_integer(100).unwrap();
        let neg_x = -x;
        assert_eq!(neg_x.integer_part(), -100);
        assert_eq!((-neg_x).integer_part(), 100);
    }

    #[test]
    fn test_abs() {
        let x = FD9::from_integer(-100).unwrap();
        assert_eq!(x.abs().unwrap().integer_part(), 100);

        let y = FD9::from_integer(100).unwrap();
        assert_eq!(y.abs().unwrap().integer_part(), 100);
    }

    #[test]
    fn test_different_decimal_places() {
        type FD4 = FixedDecimal<4>;

        assert_eq!(FD4::SCALE, 10_000);

        let x = FD4::from_parts(123, 4567).unwrap();
        assert_eq!(x.to_string(), "123.4567");
    }

    #[test]
    fn test_zero_operations() {
        let zero = FD9::ZERO;
        let one = FD9::ONE;

        assert_eq!(zero.checked_add(one).unwrap(), one);
        assert_eq!(one.checked_sub(one).unwrap(), zero);
        assert_eq!(zero.checked_mul(one).unwrap(), zero);
    }
}
