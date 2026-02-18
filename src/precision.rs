//! Precision-generic numeric trait for RIES
//!
//! This module defines the `RiesFloat` trait that abstracts over different
//! numeric precisions (f64, arbitrary precision via rug).
//!
//! The trait supports both `f64` for fast standard-precision calculations and
//! `HighPrec` (wrapping `rug::Float`) for arbitrary precision when the
//! `highprec` feature is enabled.
//!
//! # Usage
//!
//! When the `highprec` feature is enabled, you can use `HighPrec` for
//! calculations requiring more than the ~15 decimal digits of precision
//! that `f64` provides:
//!
//! ```ignore
//! use ries_rs::precision::{HighPrec, RiesFloat, DEFAULT_PRECISION};
//!
//! // Create with default precision (256 bits ≈ 77 decimal digits)
//! let pi = HighPrec::pi();
//! println!("π with high precision: {:.60}", pi.to_f64());
//!
//! // Or specify custom precision
//! let precise = HighPrec::from_f64_with_prec(2.0, 512);
//! let sqrt2 = precise.sqrt();
//! ```
//!
//! # Precision vs Performance
//!
//! | Type | Precision | Relative Speed |
//! |------|-----------|----------------|
//! | f64 | ~15 digits | 1x (baseline) |
//! | HighPrec (256 bits) | ~77 digits | ~10-50x slower |
//! | HighPrec (512 bits) | ~154 digits | ~20-100x slower |
//!
//! High-precision mode is intended for research and verification, not
//! interactive use.
#![allow(dead_code)]

use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Neg, Sub};

#[cfg(feature = "highprec")]
use rug::ops::Pow;

/// Default precision for high-precision calculations (bits)
///
/// 256 bits provides approximately 77 decimal digits of precision,
/// which is sufficient for most mathematical verification tasks.
#[cfg(feature = "highprec")]
pub const DEFAULT_PRECISION: u32 = 256;

/// A numeric type that can be used in RIES search.
///
/// This trait provides the mathematical operations needed for expression
/// evaluation and Newton-Raphson refinement.
///
/// Note: `Copy` is intentionally NOT required to support arbitrary-precision
/// types like `rug::Float` which allocate heap memory.
pub trait RiesFloat:
    Clone
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
    fn to_f64(&self) -> f64;

    /// Create from a small integer
    fn from_u8(v: u8) -> Self {
        Self::from_f64(v as f64)
    }

    /// Square root
    fn sqrt(self) -> Self;

    /// Square (self * self)
    fn square(self) -> Self
    where
        Self: Clone,
    {
        self.clone() * self
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
    fn is_nan(&self) -> bool;

    /// Check if infinite
    fn is_infinite(&self) -> bool;

    /// Check if finite (not NaN or infinite)
    fn is_finite(&self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }

    /// Compare for ordering, handling NaN
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering>
    where
        Self: Sized,
    {
        if self.is_nan() || other.is_nan() {
            None
        } else {
            PartialOrd::partial_cmp(self, other)
        }
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
    fn to_f64(&self) -> f64 {
        *self
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
    fn is_nan(&self) -> bool {
        f64::is_nan(*self)
    }

    #[inline]
    fn is_infinite(&self) -> bool {
        f64::is_infinite(*self)
    }
}

/// High-precision floating-point wrapper using rug::Float
///
/// This type provides arbitrary-precision arithmetic when the `highprec`
/// feature is enabled. It wraps `rug::Float` and implements the `RiesFloat`
/// trait for seamless integration with the RIES algorithms.
///
/// # Example
///
/// ```ignore
/// use ries_rs::precision::{HighPrec, RiesFloat};
///
/// let a = HighPrec::from_f64_with_prec(2.0, 256);
/// let b = a.sqrt();
/// assert!((b.to_f64() - 1.4142135623730951).abs() < 1e-15);
/// ```
#[cfg(feature = "highprec")]
#[derive(Clone, Debug)]
pub struct HighPrec {
    inner: rug::Float,
}

#[cfg(feature = "highprec")]
impl HighPrec {
    /// Create a new HighPrec with the specified precision (in bits)
    pub fn with_precision(precision: u32) -> Self {
        Self {
            inner: rug::Float::with_val(precision, 0),
        }
    }

    /// Create from an f64 with the default precision
    pub fn from_f64_default(v: f64) -> Self {
        Self::from_f64_with_prec(v, DEFAULT_PRECISION)
    }

    /// Create from an f64 with the specified precision
    pub fn from_f64_with_prec(v: f64, precision: u32) -> Self {
        Self {
            inner: rug::Float::with_val(precision, v),
        }
    }

    /// Get the current precision in bits
    pub fn precision(&self) -> u32 {
        self.inner.prec()
    }

    /// Get π (pi) at the current precision
    pub fn pi() -> Self {
        Self {
            inner: rug::Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi),
        }
    }

    /// Get e (Euler's number) at the current precision
    pub fn e() -> Self {
        Self {
            inner: rug::Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Euler),
        }
    }

    /// Get the golden ratio φ = (1 + √5) / 2
    pub fn phi() -> Self {
        let five = Self::from_f64_default(5.0);
        let sqrt5 = five.sqrt();
        let one = Self::one();
        (one + sqrt5) / Self::from_f64_default(2.0)
    }

    /// Format the number with the specified number of decimal places
    pub fn format(&self, decimal_places: u32) -> String {
        format!("{:.1$}", self.inner, decimal_places as usize)
    }
}

#[cfg(feature = "highprec")]
impl PartialEq for HighPrec {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

#[cfg(feature = "highprec")]
impl PartialOrd for HighPrec {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

#[cfg(feature = "highprec")]
impl Add for HighPrec {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner + rhs.inner,
        }
    }
}

#[cfg(feature = "highprec")]
impl Sub for HighPrec {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner - rhs.inner,
        }
    }
}

#[cfg(feature = "highprec")]
impl Mul for HighPrec {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner * rhs.inner,
        }
    }
}

#[cfg(feature = "highprec")]
impl Div for HighPrec {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            inner: self.inner / rhs.inner,
        }
    }
}

#[cfg(feature = "highprec")]
impl Neg for HighPrec {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self { inner: -self.inner }
    }
}

#[cfg(feature = "highprec")]
impl RiesFloat for HighPrec {
    fn zero() -> Self {
        Self::from_f64_default(0.0)
    }

    fn one() -> Self {
        Self::from_f64_default(1.0)
    }

    fn from_f64(v: f64) -> Self {
        Self::from_f64_default(v)
    }

    fn to_f64(&self) -> f64 {
        self.inner.to_f64()
    }

    fn sqrt(self) -> Self {
        Self {
            inner: self.inner.sqrt(),
        }
    }

    fn ln(self) -> Self {
        Self {
            inner: self.inner.ln(),
        }
    }

    fn exp(self) -> Self {
        Self {
            inner: self.inner.exp(),
        }
    }

    fn sin(self) -> Self {
        Self {
            inner: self.inner.sin(),
        }
    }

    fn cos(self) -> Self {
        Self {
            inner: self.inner.cos(),
        }
    }

    fn tan(self) -> Self {
        Self {
            inner: self.inner.tan(),
        }
    }

    fn pow(self, exp: Self) -> Self {
        Self {
            inner: self.inner.pow(&exp.inner),
        }
    }

    fn abs(self) -> Self {
        Self {
            inner: self.inner.abs(),
        }
    }

    fn is_nan(&self) -> bool {
        self.inner.is_nan()
    }

    fn is_infinite(&self) -> bool {
        self.inner.is_infinite()
    }
}

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

    #[test]
    fn test_f64_is_finite() {
        let x: f64 = 1.0;
        assert!(x.is_finite());

        let x: f64 = f64::NAN;
        assert!(!x.is_finite());

        let x: f64 = f64::INFINITY;
        assert!(!x.is_finite());
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_highprec_basic() {
        let a = HighPrec::from_f64(4.0);
        let b = a.sqrt();
        assert!((b.to_f64() - 2.0).abs() < 1e-50);
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_highprec_arithmetic() {
        let a = HighPrec::from_f64(3.0);
        let b = HighPrec::from_f64(2.0);

        let sum = a.clone() + b.clone();
        assert!((sum.to_f64() - 5.0).abs() < 1e-50);

        let diff = a.clone() - b.clone();
        assert!((diff.to_f64() - 1.0).abs() < 1e-50);

        let prod = a.clone() * b.clone();
        assert!((prod.to_f64() - 6.0).abs() < 1e-50);

        let quot = a / b;
        assert!((quot.to_f64() - 1.5).abs() < 1e-50);
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_highprec_transcendental() {
        let e = HighPrec::from_f64(1.0).exp();
        let expected_e = std::f64::consts::E;
        // Higher precision should be closer to true value
        assert!((e.to_f64() - expected_e).abs() < 1e-15);

        let ln2 = HighPrec::from_f64(2.0).ln();
        assert!((ln2.to_f64() - 2.0_f64.ln()).abs() < 1e-15);
    }
}
