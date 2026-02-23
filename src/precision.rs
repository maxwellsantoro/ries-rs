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

/// Error type for parsing high-precision floats from strings
#[cfg(feature = "highprec")]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Invalid float literal - the string could not be parsed as a valid number
    #[error("Invalid float literal: {0}")]
    InvalidFloatLiteral(String),
}

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
    ///
    /// Note: `rug::float::Constant::Euler` is the Euler-Mascheroni constant γ, not e.
    /// We compute e as exp(1) instead.
    pub fn e() -> Self {
        let one = rug::Float::with_val(DEFAULT_PRECISION, 1u32);
        Self { inner: one.exp() }
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

    // =========================================================================
    // Precision-aware constant constructors
    // =========================================================================
    // These methods create constants at a specified precision without
    // seeding from f64 (which would limit precision to ~16 decimal digits).

    /// Get π (pi) at the specified precision (in bits)
    ///
    /// This uses rug's built-in constant computation to achieve full precision
    /// rather than seeding from an f64 value.
    pub fn pi_with_prec(prec_bits: u32) -> Self {
        Self {
            inner: rug::Float::with_val(prec_bits, rug::float::Constant::Pi),
        }
    }

    /// Get e (Euler's number) at the specified precision (in bits)
    ///
    /// Note: `rug::float::Constant::Euler` is the Euler-Mascheroni constant γ, not e.
    /// We compute e as exp(1) at the requested precision instead.
    pub fn e_with_prec(prec_bits: u32) -> Self {
        let one = rug::Float::with_val(prec_bits, 1u32);
        Self { inner: one.exp() }
    }

    /// Create from a decimal string at the specified precision (in bits)
    ///
    /// This allows creating constants with more precision than f64 allows
    /// by parsing a long decimal string directly.
    ///
    /// # Panics
    ///
    /// Panics if the input string is not a valid float literal.
    /// Use `try_from_str_with_prec` for a non-panicking version.
    pub fn from_str_with_prec(s: &str, prec_bits: u32) -> Self {
        Self::try_from_str_with_prec(s, prec_bits).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Create from a decimal string at the specified precision (in bits)
    ///
    /// This allows creating constants with more precision than f64 allows
    /// by parsing a long decimal string directly.
    ///
    /// Returns a `Result` for graceful error handling.
    pub fn try_from_str_with_prec(s: &str, prec_bits: u32) -> Result<Self, ParseError> {
        let parsed = rug::Float::parse(s)
            .map_err(|e| ParseError::InvalidFloatLiteral(format!("{s:?}: {e}")))?;
        Ok(Self {
            inner: rug::Float::with_val(prec_bits, parsed),
        })
    }

    /// Get the golden ratio φ = (1 + √5) / 2 at the specified precision
    ///
    /// Computed using high-precision arithmetic to avoid f64 seeding.
    pub fn phi_with_prec(prec_bits: u32) -> Self {
        let five = Self::from_str_with_prec("5", prec_bits);
        let two = Self::from_str_with_prec("2", prec_bits);
        let one = Self::one_with_prec(prec_bits);
        let sqrt5 = five.sqrt();
        (one + sqrt5) / two
    }

    /// Get 1 at the specified precision
    pub fn one_with_prec(prec_bits: u32) -> Self {
        Self::from_str_with_prec("1", prec_bits)
    }

    /// Get 0 at the specified precision
    pub fn zero_with_prec(prec_bits: u32) -> Self {
        Self::from_str_with_prec("0", prec_bits)
    }

    /// Get the Euler-Mascheroni constant γ ≈ 0.5772... at the specified precision
    ///
    /// Uses a 120-digit decimal string for full precision beyond f64 limits.
    /// γ is the limiting difference between the harmonic series and natural logarithm.
    pub fn gamma_with_prec(prec_bits: u32) -> Self {
        // 120+ digits of Euler-Mascheroni constant γ
        // Source: https://oeis.org/A001620
        Self::from_str_with_prec(
            "0.5772156649015328606065120900824024310421593359399235988057672348848677267776646709369470632917467495",
            prec_bits,
        )
    }

    /// Get Apéry's constant ζ(3) ≈ 1.2020... at the specified precision
    ///
    /// Uses a 120-digit decimal string for full precision beyond f64 limits.
    /// ζ(3) = Σ(1/n³) is the value of the Riemann zeta function at 3.
    pub fn apery_with_prec(prec_bits: u32) -> Self {
        // 120+ digits of Apéry's constant ζ(3)
        // Source: https://oeis.org/A002117
        Self::from_str_with_prec(
            "1.2020569031595942853997381615114499907649862923404988817922715553418382057863130901864558736093352581",
            prec_bits,
        )
    }

    /// Get Catalan's constant G ≈ 0.9159... at the specified precision
    ///
    /// Uses a 120-digit decimal string for full precision beyond f64 limits.
    /// G = Σ((-1)^n / (2n+1)²) is a constant appearing in combinatorics.
    pub fn catalan_with_prec(prec_bits: u32) -> Self {
        // 120+ digits of Catalan's constant G
        // Source: https://oeis.org/A006752
        Self::from_str_with_prec(
            "0.9159655941772190150546035149323841107741493742816721342664981196217630197762547694793565129261151062",
            prec_bits,
        )
    }

    /// Get the plastic constant ρ ≈ 1.3247... at the specified precision
    ///
    /// Uses a 120-digit decimal string for full precision beyond f64 limits.
    /// ρ is the real root of x³ = x + 1, related to the Padovan sequence.
    pub fn plastic_with_prec(prec_bits: u32) -> Self {
        // 120+ digits of plastic constant ρ
        // Source: https://oeis.org/A060006
        Self::from_str_with_prec(
            "1.3247179572447460259609088544780973407344040569017333645340150503028278512455475940546993479817872807",
            prec_bits,
        )
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

    // ========================================================================
    // Precision-aware constant tests
    // These tests verify that precision-aware constructors provide
    // accuracy beyond f64's ~16 decimal digit limit.
    // ========================================================================

    #[cfg(feature = "highprec")]
    #[test]
    fn test_pi_precision_exceeds_f64() {
        // π at 256-bit precision should differ from f64-seeded value beyond 16 digits
        let pi_hp = HighPrec::pi_with_prec(256);
        let pi_f64 = HighPrec::from_f64_with_prec(std::f64::consts::PI, 256);

        // Format both to 40 decimal places
        let hp_str = format!("{:.40}", pi_hp.inner);
        let f64_str = format!("{:.40}", pi_f64.inner);

        // The high-precision π should start with known digits
        // π = 3.1415926535897932384626433832795028841971...
        assert!(hp_str.starts_with("3.14159265358979323846"));

        // After ~16 digits, f64 version loses accuracy but high-prec continues
        // The two strings should be identical up to ~16 digits
        let common_prefix_len = hp_str
            .chars()
            .zip(f64_str.chars())
            .take_while(|(a, b)| a == b)
            .count();
        // f64 has ~15-16 significant digits, so they match up to about 17 chars (including "3.")
        assert!((17..25).contains(&common_prefix_len));
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_e_precision_exceeds_f64() {
        // e at 256-bit precision should differ from f64-seeded value beyond 16 digits
        let e_hp = HighPrec::e_with_prec(256);
        let e_f64 = HighPrec::from_f64_with_prec(std::f64::consts::E, 256);

        let hp_str = format!("{:.40}", e_hp.inner);
        let f64_str = format!("{:.40}", e_f64.inner);

        // e = 2.71828182845904523536028747135266249775724709...
        assert!(
            hp_str.starts_with("2.71828182845904523536"),
            "e hp_str was: {hp_str}"
        );

        let common_prefix_len = hp_str
            .chars()
            .zip(f64_str.chars())
            .take_while(|(a, b)| a == b)
            .count();
        assert!((17..25).contains(&common_prefix_len));
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_gamma_precision_exceeds_f64() {
        // Euler-Mascheroni constant at 256-bit precision
        let gamma_hp = HighPrec::gamma_with_prec(256);
        // f64 only has about 16 digits of this constant
        let gamma_f64_approx = 0.5772156649015329;
        let gamma_f64 = HighPrec::from_f64_with_prec(gamma_f64_approx, 256);

        let hp_str = format!("{:.40}", gamma_hp.inner);
        let f64_str = format!("{:.40}", gamma_f64.inner);

        // γ = 0.57721566490153286060651209008240243104215933...
        // rug formats values < 1 in scientific notation with {:.N}: "5.772...e-1"
        assert!(
            hp_str.starts_with("5.7721566490153286060"),
            "gamma hp_str was: {hp_str}"
        );

        // The high-prec version should have more accurate digits
        let common_prefix_len = hp_str
            .chars()
            .zip(f64_str.chars())
            .take_while(|(a, b)| a == b)
            .count();
        // They should diverge somewhere after 16-17 digits
        assert!((16..22).contains(&common_prefix_len));
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_apery_precision_exceeds_f64() {
        // Apéry's constant ζ(3) at 256-bit precision
        let apery_hp = HighPrec::apery_with_prec(256);
        let apery_f64_approx = 1.2020569031595942;
        let apery_f64 = HighPrec::from_f64_with_prec(apery_f64_approx, 256);

        let hp_str = format!("{:.40}", apery_hp.inner);
        let f64_str = format!("{:.40}", apery_f64.inner);

        // ζ(3) = 1.2020569031595942853997381615114499907649...
        assert!(hp_str.starts_with("1.2020569031595942853"));

        let common_prefix_len = hp_str
            .chars()
            .zip(f64_str.chars())
            .take_while(|(a, b)| a == b)
            .count();
        assert!((16..22).contains(&common_prefix_len));
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_catalan_precision() {
        let catalan_hp = HighPrec::catalan_with_prec(256);
        let hp_str = format!("{:.40}", catalan_hp.inner);

        // Catalan's constant G = 0.91596559417721901505460351493238411077414937...
        // rug formats values < 1 in scientific notation with {:.N}: "9.159...e-1"
        assert!(
            hp_str.starts_with("9.1596559417721901505"),
            "catalan hp_str was: {hp_str}"
        );
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_plastic_precision() {
        let plastic_hp = HighPrec::plastic_with_prec(256);
        let hp_str = format!("{:.40}", plastic_hp.inner);

        // Plastic constant ρ = 1.3247179572447460259609088544780973407344...
        assert!(hp_str.starts_with("1.32471795724474602596"));
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_phi_with_prec() {
        let phi_hp = HighPrec::phi_with_prec(256);
        let hp_str = format!("{:.40}", phi_hp.inner);

        // Golden ratio φ = (1 + √5) / 2 = 1.6180339887498948482045868343656381177203...
        assert!(hp_str.starts_with("1.61803398874989484820"));
    }

    // =========================================================================
    // Error handling tests
    // =========================================================================

    #[cfg(feature = "highprec")]
    #[test]
    fn test_from_str_with_prec_invalid_input() {
        // This test demonstrates that from_str_with_prec should gracefully
        // handle invalid input rather than panicking
        let result = HighPrec::try_from_str_with_prec("not_a_number", 256);

        assert!(
            result.is_err(),
            "Should return error for invalid float literal"
        );
        if let Err(e) = result {
            assert!(e.to_string().contains("Invalid float literal"));
        }
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_from_str_with_prec_valid_input() {
        // Valid input should work
        let result = HighPrec::try_from_str_with_prec("3.14159", 256);
        assert!(result.is_ok(), "Should succeed for valid float literal");

        let hp = result.unwrap();
        let value = hp.to_f64();
        assert!((value - 3.14159).abs() < 1e-10);
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_from_str_with_prec_empty_string() {
        // Empty string should be an error
        let result = HighPrec::try_from_str_with_prec("", 256);
        assert!(result.is_err(), "Should return error for empty string");
    }
}
