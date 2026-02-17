//! Precision-generic numeric trait for RIES
//!
//! This module defines the `RiesFloat` trait that abstracts over different
//! numeric precisions (f64, arbitrary precision via rug).
//!
//! Currently only f64 is implemented. Future versions will add rug::Float
//! for high-precision calculations.

use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// A numeric type that can be used in RIES search.
///
/// This trait provides the mathematical operations needed for expression
/// evaluation and Newton-Raphson refinement.
pub trait RiesFloat:
    Clone
    + Copy
    + PartialOrd
    + Debug
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    /// The zero value
    fn zero() -> Self;

    /// The one value
    fn one() -> Self;

    /// Create from an f64 value
    fn from_f64(v: f64) -> Self;

    /// Convert to f64 (may lose precision for arbitrary precision types)
    fn to_f64(self) -> f64;

    /// Create from a small integer
    fn from_u8(v: u8) -> Self {
        Self::from_f64(v as f64)
    }

    /// Square root
    fn sqrt(self) -> Self;

    /// Square (self * self)
    fn square(self) -> Self {
        self * self
    }

    /// Natural logarithm
    fn ln(self) -> Self;

    /// Exponential (e^self)
    fn exp(self) -> Self;

    /// Sine
    fn sin(self) -> Self;

    /// Cosine
    fn cos(self) -> Self;

    /// Tangent
    fn tan(self) -> Self;

    /// Power (self^exp)
    fn pow(self, exp: Self) -> Self;

    /// Absolute value
    fn abs(self) -> Self;

    /// Check if NaN
    fn is_nan(self) -> bool;

    /// Check if infinite
    fn is_infinite(self) -> bool;

    /// Check if finite (not NaN or infinite)
    fn is_finite(self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }
}

// Implement RiesFloat for f64
impl RiesFloat for f64 {
    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn one() -> Self {
        1.0
    }

    #[inline]
    fn from_f64(v: f64) -> Self {
        v
    }

    #[inline]
    fn to_f64(self) -> f64 {
        self
    }

    #[inline]
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }

    #[inline]
    fn ln(self) -> Self {
        f64::ln(self)
    }

    #[inline]
    fn exp(self) -> Self {
        f64::exp(self)
    }

    #[inline]
    fn sin(self) -> Self {
        f64::sin(self)
    }

    #[inline]
    fn cos(self) -> Self {
        f64::cos(self)
    }

    #[inline]
    fn tan(self) -> Self {
        f64::tan(self)
    }

    #[inline]
    fn pow(self, exp: Self) -> Self {
        f64::powf(self, exp)
    }

    #[inline]
    fn abs(self) -> Self {
        f64::abs(self)
    }

    #[inline]
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }

    #[inline]
    fn is_infinite(self) -> bool {
        f64::is_infinite(self)
    }
}

// Placeholder for rug::Float implementation
// This will be implemented when full high-precision support is added
//
// #[cfg(feature = "highprec")]
// impl RiesFloat for rug::Float {
//     // ... implementation using rug operations
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f64_ries_float() {
        let x: f64 = RiesFloat::from_f64(4.0);
        assert!((x.sqrt() - 2.0).abs() < 1e-10);
        assert!((x.ln() - 4.0_f64.ln()).abs() < 1e-10);
        assert!((x.exp() - 4.0_f64.exp()).abs() < 1e-10);
    }

    #[test]
    fn test_f64_arithmetic() {
        let a: f64 = RiesFloat::from_f64(3.0);
        let b: f64 = RiesFloat::from_f64(2.0);
        assert!((a + b - 5.0).abs() < 1e-10);
        assert!((a - b - 1.0).abs() < 1e-10);
        assert!((a * b - 6.0).abs() < 1e-10);
        assert!((a / b - 1.5).abs() < 1e-10);
    }
}
