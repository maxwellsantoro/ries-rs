//! Re-check an f64-exact candidate at the typed target.
//!
//! f64 evaluation can round a reciprocal or a large argument onto a special
//! value. The accurate trig reduction then agrees with a simple right-hand
//! side even though the real functions at that same f64 do not. Exact labels
//! require the two sides, recomputed in double-double, to agree to eight ulps.
//! A value this arithmetic cannot resolve is not labeled exact.

use crate::eval::EvalContext;
use crate::expr::Expression;
use crate::symbol::{Seft, Symbol};
use crate::udf::UdfOp;

/// Magnitude where one double-double ulp, times π, reaches one ulp of a
/// unit-magnitude sine or cosine.
///
/// A double-double ulp at magnitude `M` is `M * 2^-106`. Differentiating
/// `sin(πx)` multiplies that absolute error by π. One ulp of a result near 1
/// is `2^-52`, so `M = 2^54 / π` (about `5.86e15`, between `2^52` and `2^53`).
/// A computed fraction at or above this magnitude is not faithful, and the
/// result is loose. Exact integers and half-integers are returned first.
fn trig_loose_magnitude() -> f64 {
    2.0_f64.powi(54) / std::f64::consts::PI
}

#[derive(Clone, Copy)]
struct Dd {
    hi: f64,
    lo: f64,
    /// The fractional part of a transcendental argument was too large to trust.
    loose: bool,
    /// This value is an exact integer: an f64 integer, or a sum or product of
    /// those. The property survives magnitudes where a double-double can no
    /// longer see a fraction. A non-integer power, such as `(2^51+1/2)^3`,
    /// does not get the flag, so a dropped `0.125` is not called an integer.
    integral: bool,
}

impl Dd {
    fn new(hi: f64, lo: f64, loose: bool, integral: bool) -> Self {
        Self {
            hi,
            lo,
            loose,
            integral,
        }
    }

    fn zero() -> Self {
        Self::new(0.0, 0.0, false, true)
    }

    fn one() -> Self {
        Self::new(1.0, 0.0, false, true)
    }

    fn from_f64(value: f64) -> Self {
        let integral = value.is_finite() && value.fract() == 0.0;
        Self::new(value, 0.0, false, integral)
    }

    fn from_i32(value: i32) -> Self {
        Self::from_f64(f64::from(value))
    }

    fn pi() -> Self {
        // Extra word of π past the f64 constant. The high word is exactly
        // `f64::consts::PI`, so `x = pi` differs only by this word.
        Self::new(std::f64::consts::PI, 1.2246467991473532e-16, false, false)
    }

    fn ln2() -> Self {
        Self::new(std::f64::consts::LN_2, 2.3190468138462996e-17, false, false)
    }

    fn abs(self) -> Self {
        if self.hi < 0.0 || (self.hi == 0.0 && self.lo < 0.0) {
            -self
        } else {
            self
        }
    }

    fn floor(self) -> Self {
        if !self.hi.is_finite() {
            return self;
        }
        let hi_floor = self.hi.floor();
        if hi_floor == self.hi {
            return renormalize(self.hi, self.lo.floor(), self.loose, self.integral);
        }
        let frac = (self.hi - hi_floor) + self.lo;
        if frac >= 1.0 {
            Self::from_f64(hi_floor + 1.0)
                .flag(self.loose)
                .exact_if(self.integral)
        } else if frac < 0.0 {
            Self::from_f64(hi_floor - 1.0)
                .flag(self.loose)
                .exact_if(self.integral)
        } else {
            Self::from_f64(hi_floor)
                .flag(self.loose)
                .exact_if(self.integral)
        }
    }

    /// Round half away from zero, matching `f64::round`.
    fn round(self) -> Self {
        if self.hi < 0.0 || (self.hi == 0.0 && self.lo < 0.0) {
            -(-self).round()
        } else {
            let floor = self.floor();
            let frac = self - floor;
            if frac.hi > 0.5 || (frac.hi == 0.5 && frac.lo >= 0.0) {
                floor + Self::one()
            } else {
                floor
            }
        }
    }

    fn flag(mut self, loose: bool) -> Self {
        self.loose |= loose;
        self
    }

    /// `integral` stays set only when every operation that built this value was.
    fn exact_if(mut self, integral: bool) -> Self {
        self.integral &= integral;
        self
    }

    fn is_exact_integer(self) -> bool {
        let rounded = self.round();
        (self - rounded).hi == 0.0 && (self - rounded).lo == 0.0
    }

    fn small_int(self) -> Option<i32> {
        if !self.is_exact_integer() || self.hi.abs() >= 2.0_f64.powi(31) {
            return None;
        }
        if self.lo != 0.0 && self.hi.abs() < 2.0_f64.powi(52) {
            return None;
        }
        Some(self.hi as i32)
    }
}

impl std::ops::Neg for Dd {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.hi, -self.lo, self.loose, self.integral)
    }
}

impl std::ops::Add for Dd {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let (sum, err) = two_sum(self.hi, rhs.hi);
        renormalize(
            sum,
            err + self.lo + rhs.lo,
            self.loose || rhs.loose,
            self.integral && rhs.integral,
        )
    }
}

impl std::ops::Sub for Dd {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self + (-rhs)
    }
}

impl std::ops::Mul for Dd {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let (prod, err) = two_prod(self.hi, rhs.hi);
        let cross = self.hi * rhs.lo + self.lo * rhs.hi + self.lo * rhs.lo;
        let overflow = self.hi.abs() > 1e290 || rhs.hi.abs() > 1e290 || !prod.is_finite();
        let integral = self.integral && rhs.integral && prod.is_finite();
        // A product of exact integrals is still an integer when it overflows the
        // Dekker split: every f64 at this magnitude is an integer, and the
        // rounded product of integers is an integer. Other overflows stay loose.
        let loose = self.loose || rhs.loose || (overflow && !integral);
        // A flushed product of two nonzero values is not a proof of 0.
        // `x/2` and `x*(1/2)` at the smallest subnormal are about `2.5e-324`.
        flushed_nonzero(renormalize(prod, err + cross, loose, integral), self, rhs)
    }
}

fn is_nonzero(value: Dd) -> bool {
    value.hi != 0.0 || value.lo != 0.0
}

/// A zero computed from nonzero inputs underflowed. It is not an exact 0.
fn flushed_nonzero(mut value: Dd, lhs: Dd, rhs: Dd) -> Dd {
    if is_nonzero(lhs) && is_nonzero(rhs) && !is_nonzero(value) {
        value.loose = true;
    }
    value
}

impl std::ops::Div for Dd {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let q = self.hi / rhs.hi;
        let (prod, err) = two_prod(q, rhs.hi);
        let remainder = (((self.hi - prod) - err) + self.lo) - q * rhs.lo;
        let loose = self.loose || rhs.loose || !q.is_finite();
        // Dividing an integer by a power of two that divides it stays an
        // integer (`x/2` at `1e300`). Any other quotient does not.
        let integral = q.is_finite() && integer_divided_by_power_of_two(self, rhs);
        flushed_nonzero(
            renormalize(q, remainder / rhs.hi, loose, integral),
            self,
            rhs,
        )
    }
}

/// Powers of two in the exact integer `value`, or `u32::MAX` for 0.
fn two_valuation(value: Dd) -> u32 {
    fn f64_valuation(n: f64) -> u32 {
        if n == 0.0 {
            return u32::MAX;
        }
        let bits = n.abs().to_bits();
        let exp = ((bits >> 52) & 0x7ff) as i32;
        let frac = bits & 0x000f_ffff_ffff_ffff;
        if exp == 0 {
            return frac.trailing_zeros();
        }
        let significand = frac | (1_u64 << 52);
        let total = significand.trailing_zeros() as i32 + (exp - 1023 - 52);
        if total < 0 {
            0
        } else {
            total as u32
        }
    }
    if value.lo != 0.0 {
        f64_valuation(value.lo)
    } else {
        f64_valuation(value.hi)
    }
}

fn integer_divided_by_power_of_two(lhs: Dd, rhs: Dd) -> bool {
    if !lhs.integral || !rhs.integral || rhs.lo != 0.0 || rhs.hi == 0.0 {
        return false;
    }
    let mag = rhs.hi.abs();
    let power_of_two = mag.is_finite() && (mag.to_bits() & 0x000f_ffff_ffff_ffff) == 0;
    power_of_two && two_valuation(lhs) >= two_valuation(rhs)
}

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let sum = a + b;
    let bb = sum - a;
    let err = (a - (sum - bb)) + (b - bb);
    (sum, err)
}

fn renormalize(a: f64, b: f64, loose: bool, integral: bool) -> Dd {
    let (hi, lo) = two_sum(a, b);
    Dd::new(hi, lo, loose || !hi.is_finite(), integral && hi.is_finite())
}

/// Dekker product. Exact error term without assuming a fused multiply-add,
/// which WASM and some soft-float targets do not provide.
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let prod = a * b;
    if !prod.is_finite() || a.abs() > 1e290 || b.abs() > 1e290 {
        return (prod, 0.0);
    }
    let (a_hi, a_lo) = split(a);
    let (b_hi, b_lo) = split(b);
    let err = ((a_hi * b_hi - prod) + a_hi * b_lo + a_lo * b_hi) + a_lo * b_lo;
    (prod, err)
}

fn split(value: f64) -> (f64, f64) {
    let shifted = 134_217_729.0 * value; // 2^27 + 1
    let hi = shifted - (shifted - value);
    (hi, value - hi)
}

fn is_even_integer(n: Dd) -> bool {
    if n.hi.abs() < 2.0_f64.powi(53) {
        if n.lo != 0.0 {
            return is_even_integer(Dd::from_f64(n.lo).round());
        }
        return n.hi.rem_euclid(2.0) == 0.0;
    }
    // Above 2^53 every f64 integer is even. The low bit lives in `lo`.
    is_even_integer(Dd::from_f64(n.lo).round())
}

fn scale2(value: Dd, exp: i32) -> Dd {
    if exp == 0 {
        return value;
    }
    let factor = 2.0_f64.powi(exp);
    renormalize(
        value.hi * factor,
        value.lo * factor,
        value.loose || !factor.is_finite(),
        value.integral && factor.is_finite(),
    )
}

fn frexp(value: f64) -> (f64, i32) {
    if value == 0.0 || !value.is_finite() {
        return (value, 0);
    }
    let bits = value.to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i32;
    if exp_field == 0 {
        // Subnormals have no implicit 1, so the zero exponent field is not
        // -1022. Scale into the normal range and correct the power of two.
        // 2^54 lifts the smallest subnormal (2^-1074) to 2^-1020.
        let (mantissa, exponent) = frexp(value * 2.0_f64.powi(54));
        return (mantissa, exponent - 54);
    }
    let exponent = exp_field - 1022;
    let mantissa = f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000);
    (mantissa, exponent)
}

/// `n` mod 4 for an exact integer, in `0..=3`.
fn integer_mod4(n: Dd) -> i32 {
    fn mod4(x: f64) -> f64 {
        x.rem_euclid(4.0)
    }
    if n.hi.abs() < 2.0_f64.powi(53) {
        let exact = if n.lo == 0.0 { n.hi } else { n.hi + n.lo };
        return mod4(exact) as i32;
    }
    // At and above 2^54 every f64 integer is a multiple of 4, so the residue
    // lives in the low word.
    if n.hi.abs() >= 2.0_f64.powi(54) {
        return integer_mod4(Dd::from_f64(n.lo).round());
    }
    mod4(mod4(n.hi) + mod4(n.lo)) as i32
}

/// Sine of π times an exact half-integer: `sin(π(k+1/2)) = (-1)^k`.
fn half_integer_sin_sign(value: Dd) -> Option<f64> {
    let doubled = value * Dd::from_f64(2.0);
    if !doubled.is_exact_integer() || is_even_integer(doubled) {
        return None;
    }
    match integer_mod4(doubled) {
        1 => Some(1.0),
        3 => Some(-1.0),
        _ => None,
    }
}

fn sin_cos(theta: Dd) -> (Dd, Dd) {
    let square = theta * theta;
    let mut sin_term = theta;
    let mut cos_term = Dd::one();
    let mut sin = sin_term;
    let mut cos = cos_term;
    for n in 1..24 {
        sin_term = -sin_term * square / Dd::from_f64(f64::from((2 * n) * (2 * n + 1)));
        sin = sin + sin_term;
        cos_term = -cos_term * square / Dd::from_f64(f64::from((2 * n - 1) * (2 * n)));
        cos = cos + cos_term;
        if sin_term.hi.abs() <= 1e-30 && cos_term.hi.abs() <= 1e-30 {
            break;
        }
    }
    (sin, cos)
}

fn sin_cos_pi(value: Dd) -> Option<(Dd, Dd)> {
    if !value.hi.is_finite() {
        return None;
    }
    let nearest = value.round();
    let fraction = value - nearest;
    // A computed value whose magnitude exceeds 2^105 has a double-double ulp
    // coarser than 1/2, so both words can be integers while the true value is
    // not. An exact integral (literals combined by + and *) is exact at every
    // magnitude: `sinpi(1 + x)` at `1e300` really is an integer.
    let fraction_resolved =
        !value.loose && (value.integral || value.hi.abs() < 0.5 * 2.0_f64.powi(106));
    if fraction_resolved && fraction.hi == 0.0 && fraction.lo == 0.0 {
        let sign = if is_even_integer(nearest) { 1.0 } else { -1.0 };
        return Some((Dd::zero(), Dd::from_f64(sign)));
    }
    if fraction_resolved {
        if let Some(sign) = half_integer_sin_sign(value) {
            return Some((Dd::from_f64(sign), Dd::zero()));
        }
    }
    let loose = !fraction_resolved || value.hi.abs() >= trig_loose_magnitude();
    let (mut sin, mut cos) = sin_cos(fraction * Dd::pi());
    if !is_even_integer(nearest) {
        sin = -sin;
        cos = -cos;
    }
    Some((sin.flag(loose), cos.flag(loose)))
}

fn exp_reduced(value: Dd) -> Dd {
    let mut sum = Dd::one();
    let mut term = Dd::one();
    for n in 1..32 {
        term = term * value / Dd::from_f64(f64::from(n));
        sum = sum + term;
        if term.hi.abs() <= 1e-30 * sum.hi.abs().max(1.0) {
            break;
        }
    }
    sum
}

fn exp(value: Dd) -> Option<Dd> {
    if !value.hi.is_finite() {
        return None;
    }
    if value.hi.abs() < 1e-16 && value.lo.abs() < 1e-16 {
        return Some((Dd::one() + value).flag(value.loose));
    }
    if value.hi > 709.0 || value.hi < -745.0 {
        return None;
    }
    let quotient = (value / Dd::ln2()).round();
    let n = quotient.small_int()?;
    let reduced = value - Dd::from_i32(n) * Dd::ln2();
    Some(scale2(exp_reduced(reduced), n).flag(value.loose))
}

fn ln(value: Dd) -> Option<Dd> {
    if value.hi <= 0.0 || !value.hi.is_finite() {
        return None;
    }
    if value.hi == 1.0 && value.lo == 0.0 {
        return Some(Dd::zero().flag(value.loose));
    }
    let (mantissa_hi, mut exponent) = frexp(value.hi);
    // A zero low word must not be scaled by a subnormal power of two: that
    // division is 0/0. A nonzero low word is scaled in two steps so the
    // exponent of a subnormal cannot underflow the factor itself.
    let lo_scaled = if value.lo == 0.0 {
        0.0
    } else if exponent >= 0 {
        value.lo / 2.0_f64.powi(exponent)
    } else {
        value.lo * 2.0_f64.powi((-exponent).min(1023)) * 2.0_f64.powi((-exponent - 1023).max(0))
    };
    let mut mantissa = renormalize(mantissa_hi, lo_scaled, value.loose, false);
    if mantissa.hi < std::f64::consts::FRAC_1_SQRT_2 {
        mantissa = mantissa * Dd::from_f64(2.0);
        exponent -= 1;
    }
    let z = (mantissa - Dd::one()) / (mantissa + Dd::one());
    let z2 = z * z;
    let mut sum = z;
    let mut term = z;
    for n in 1..32 {
        term = term * z2;
        sum = sum + term / Dd::from_f64(f64::from(2 * n + 1));
        if term.hi.abs() < 1e-30 {
            break;
        }
    }
    Some((sum * Dd::from_f64(2.0) + Dd::from_i32(exponent) * Dd::ln2()).flag(value.loose))
}

fn atan_euler(value: Dd) -> Dd {
    // Euler's series on an argument already reduced below 1/2.
    let square = value * value;
    let unit = square / (Dd::one() + square);
    let mut sum = Dd::one();
    let mut term = Dd::one();
    for n in 1..48 {
        term = term * unit * Dd::from_f64(f64::from(2 * n)) / Dd::from_f64(f64::from(2 * n + 1));
        sum = sum + term;
        if term.hi.abs() < 1e-30 {
            break;
        }
    }
    value / (Dd::one() + square) * sum
}

fn atan(value: Dd) -> Option<Dd> {
    if !value.hi.is_finite() {
        return None;
    }
    if value.hi == 0.0 && value.lo == 0.0 {
        return Some(Dd::zero().flag(value.loose));
    }
    let negative = value.hi < 0.0 || (value.hi == 0.0 && value.lo < 0.0);
    let mut reduced = value.abs();
    let mut right_angle = false;
    if reduced.hi > 1.0 || (reduced.hi == 1.0 && reduced.lo > 0.0) {
        // atan(|x|) = π/2 - atan(1/|x|), so the series sees |z| <= 1.
        reduced = Dd::one() / reduced;
        right_angle = true;
    }
    // Half-angle: atan(z) = 2 atan(z / (1 + sqrt(1+z²))). One step takes
    // |z| <= 1 down to at most √2 - 1 ≈ 0.414, where the series is faithful.
    let mut doublings = 0_i32;
    for _ in 0..4 {
        if reduced.hi <= 0.5 {
            break;
        }
        let hypotenuse = sqrt(Dd::one() + reduced * reduced)?;
        reduced = reduced / (Dd::one() + hypotenuse);
        doublings += 1;
    }
    let mut angle = atan_euler(reduced) * Dd::from_f64(2.0_f64.powi(doublings));
    if right_angle {
        angle = Dd::pi() * Dd::from_f64(0.5) - angle;
    }
    let signed = if negative { -angle } else { angle };
    base_finite(signed.flag(value.loose))
}

fn atan2(y: Dd, x: Dd) -> Option<Dd> {
    let x_neg = x.hi < 0.0 || (x.hi == 0.0 && x.lo < 0.0);
    let y_neg = y.hi < 0.0 || (y.hi == 0.0 && y.lo < 0.0);
    let x_zero = x.hi == 0.0 && x.lo == 0.0;
    let y_zero = y.hi == 0.0 && y.lo == 0.0;
    if x_zero && y_zero {
        return None;
    }
    if x_zero {
        let half = Dd::pi() * Dd::from_f64(0.5);
        return Some(if y_neg { -half } else { half });
    }
    let base = atan(y / x)?;
    if !x_neg {
        return Some(base);
    }
    Some(if y_neg {
        base - Dd::pi()
    } else {
        base + Dd::pi()
    })
}

fn pow_int(mut base: Dd, exp: i32) -> Option<Dd> {
    if exp == 0 {
        return if base.hi == 0.0 && base.lo == 0.0 {
            None
        } else {
            Some(Dd::one().flag(base.loose))
        };
    }
    if exp < 0 {
        if base.hi == 0.0 && base.lo == 0.0 {
            return None;
        }
        base = Dd::one() / base;
        if !base.hi.is_finite() {
            return None;
        }
    }
    let mut exponent = exp.unsigned_abs();
    let mut result = Dd::one().flag(base.loose);
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result * base;
        }
        exponent >>= 1;
        if exponent > 0 {
            base = base * base;
        }
    }
    base_finite(result)
}

fn pow(base: Dd, exponent: Dd) -> Option<Dd> {
    if let Some(n) = exponent.small_int() {
        return pow_int(base, n).map(|value| value.flag(exponent.loose));
    }
    if base.hi < 0.0 || (base.hi == 0.0 && base.lo <= 0.0) {
        return None;
    }
    exp(exponent * ln(base)?)
}

fn sqrt(value: Dd) -> Option<Dd> {
    if value.hi < 0.0 || (value.hi == 0.0 && value.lo < 0.0) {
        return None;
    }
    if value.hi == 0.0 && value.lo == 0.0 {
        return Some(Dd::zero().flag(value.loose));
    }
    let mut guess = Dd::from_f64(value.hi.sqrt()).flag(value.loose);
    for _ in 0..3 {
        guess = guess + (value - guess * guess) / (Dd::from_f64(2.0) * guess);
    }
    base_finite(guess)
}

fn lambert_w(value: Dd) -> Option<Dd> {
    if value.hi == 0.0 && value.lo == 0.0 {
        return Some(Dd::zero());
    }
    // Same branch point as `eval::lambert_w`. The f64 `-1/e` sits one ulp
    // below the real value; arguments strictly below that float are outside
    // the principal branch, and the float itself is W = -1.
    let branch = -1.0 / std::f64::consts::E;
    if value.hi < branch || (value.hi == branch && value.lo < 0.0) {
        return None;
    }
    if value.hi == branch && value.lo == 0.0 {
        return Some(Dd::from_f64(-1.0).flag(value.loose));
    }
    if value.hi == std::f64::consts::E && value.lo == 0.0 {
        return Some(Dd::one().flag(value.loose));
    }
    let mut w = if value.hi < 0.0 {
        Dd::from_f64(-0.5)
    } else if value.hi < 1.0 || (value.hi == 1.0 && value.lo < 0.0) {
        value
    } else if value.hi == 1.0 && value.lo == 0.0 {
        // ln(ln(1)) is ln(0). W(1) is the omega constant; 1/2 converges to it.
        Dd::from_f64(0.5)
    } else {
        let log = ln(value)?;
        if log.hi <= 0.0 {
            value
        } else {
            log - ln(log)?
        }
    };
    for _ in 0..24 {
        let ew = exp(w)?;
        let residual = w * ew - value;
        if residual.hi.abs() <= 1e-24 * value.hi.abs().max(1.0) {
            return base_finite(w.flag(value.loose));
        }
        let denom = ew * (w + Dd::one())
            - (w + Dd::from_f64(2.0)) * residual / (Dd::from_f64(2.0) * w + Dd::from_f64(2.0));
        if denom.hi == 0.0 && denom.lo == 0.0 {
            return None;
        }
        w = w - residual / denom;
    }
    None
}

fn base_finite(value: Dd) -> Option<Dd> {
    value.hi.is_finite().then_some(value)
}

fn ulp_of(mag: f64) -> f64 {
    if mag <= 0.0 || !mag.is_finite() {
        return f64::from_bits(1);
    }
    let up = mag.next_up();
    if up.is_finite() {
        up - mag
    } else {
        mag - mag.next_down()
    }
}

fn evaluate(expr: &Expression, target: f64, context: &EvalContext<'_>) -> Option<Dd> {
    if expr.has_decimal_literals() || !target.is_finite() {
        return None;
    }
    let mut stack = Vec::with_capacity(8);
    for &sym in expr.symbols() {
        match sym.seft() {
            Seft::A => stack.push(eval_constant(sym, target, context)?),
            Seft::B => {
                let arg = stack.pop()?;
                if let Some(index) = sym.user_function_index() {
                    stack.push(eval_user_function(index as usize, arg, target, context)?);
                } else {
                    stack.push(eval_unary(sym, arg, context)?);
                }
            }
            Seft::C => {
                let rhs = stack.pop()?;
                let lhs = stack.pop()?;
                stack.push(eval_binary(sym, lhs, rhs)?);
            }
        }
    }
    if stack.len() == 1 {
        base_finite(stack.pop()?)
    } else {
        None
    }
}

fn eval_constant(sym: Symbol, target: f64, context: &EvalContext<'_>) -> Option<Dd> {
    use crate::eval::constants::{APERY, CATALAN, GAMMA, PLASTIC};
    let value = match sym {
        Symbol::One => 1.0,
        Symbol::Two => 2.0,
        Symbol::Three => 3.0,
        Symbol::Four => 4.0,
        Symbol::Five => 5.0,
        Symbol::Six => 6.0,
        Symbol::Seven => 7.0,
        Symbol::Eight => 8.0,
        Symbol::Nine => 9.0,
        Symbol::Pi => {
            // The extra word of π is what stops `π * 2^52` from looking like an
            // integer. `x = pi` still agrees: the low word is under eight ulps.
            return Some(Dd::pi());
        }
        Symbol::E => return exp(Dd::one()),
        Symbol::Phi => {
            return sqrt(Dd::from_f64(5.0)).map(|root| (Dd::one() + root) / Dd::from_f64(2.0));
        }
        Symbol::Gamma => GAMMA,
        Symbol::Plastic => PLASTIC,
        Symbol::Apery => APERY,
        Symbol::Catalan => CATALAN,
        Symbol::X => target,
        other => {
            let index = other.user_constant_index()? as usize;
            context.user_constants.get(index)?.value
        }
    };
    value.is_finite().then(|| Dd::from_f64(value))
}

fn eval_unary(sym: Symbol, arg: Dd, context: &EvalContext<'_>) -> Option<Dd> {
    let default_scale = context.trig_argument_scale == crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE;
    match sym {
        Symbol::Neg => Some(-arg),
        Symbol::Recip => {
            if arg.hi == 0.0 && arg.lo == 0.0 {
                None
            } else {
                base_finite(Dd::one() / arg)
            }
        }
        Symbol::Sqrt => sqrt(arg),
        Symbol::Square => base_finite(arg * arg),
        Symbol::Ln => ln(arg),
        Symbol::Exp => exp(arg),
        Symbol::SinPi if default_scale => sin_cos_pi(arg).map(|(sin, _)| sin),
        Symbol::CosPi if default_scale => sin_cos_pi(arg).map(|(_, cos)| cos),
        Symbol::TanPi if default_scale => {
            let (sin, cos) = sin_cos_pi(arg)?;
            if cos.hi.abs() < 1e-18 {
                None
            } else {
                base_finite(sin / cos)
            }
        }
        Symbol::SinPi | Symbol::CosPi | Symbol::TanPi => {
            let turns = arg * Dd::from_f64(context.trig_argument_scale) / Dd::pi();
            let (sin, cos) = sin_cos_pi(turns.flag(arg.loose))?;
            match sym {
                Symbol::SinPi => Some(sin),
                Symbol::CosPi => Some(cos),
                _ => {
                    if cos.hi.abs() < 1e-18 {
                        None
                    } else {
                        base_finite(sin / cos)
                    }
                }
            }
        }
        Symbol::LambertW => lambert_w(arg),
        _ => None,
    }
}

fn eval_binary(sym: Symbol, lhs: Dd, rhs: Dd) -> Option<Dd> {
    match sym {
        Symbol::Add => base_finite(lhs + rhs),
        Symbol::Sub => base_finite(lhs - rhs),
        Symbol::Mul => base_finite(lhs * rhs),
        Symbol::Div => {
            if rhs.hi == 0.0 && rhs.lo == 0.0 {
                None
            } else {
                base_finite(lhs / rhs)
            }
        }
        Symbol::Pow => pow(lhs, rhs),
        Symbol::Root => {
            if lhs.hi == 0.0 && lhs.lo == 0.0 {
                return None;
            }
            if rhs.hi < 0.0 || (rhs.hi == 0.0 && rhs.lo < 0.0) {
                let index = lhs.small_int()?;
                if index == 0 || index % 2 == 0 {
                    return None;
                }
                return pow(-rhs, Dd::one() / lhs).map(|root| -root);
            }
            pow(rhs, Dd::one() / lhs)
        }
        Symbol::Log => {
            let denom = ln(lhs)?;
            if denom.hi == 0.0 && denom.lo == 0.0 {
                None
            } else {
                base_finite(ln(rhs)? / denom)
            }
        }
        Symbol::Atan2 => atan2(lhs, rhs),
        _ => None,
    }
}

fn eval_user_function(index: usize, arg: Dd, target: f64, context: &EvalContext<'_>) -> Option<Dd> {
    let udf = context.user_functions.get(index)?;
    let mut stack = vec![arg];
    for op in &udf.body {
        match op {
            UdfOp::Symbol(Symbol::X) => return None,
            UdfOp::Symbol(sym) if sym.user_function_index().is_some() => return None,
            UdfOp::Symbol(sym) => match sym.seft() {
                Seft::A => stack.push(eval_constant(*sym, target, context)?),
                Seft::B => {
                    let value = stack.pop()?;
                    stack.push(eval_unary(*sym, value, context)?);
                }
                Seft::C => {
                    let rhs = stack.pop()?;
                    let lhs = stack.pop()?;
                    stack.push(eval_binary(*sym, lhs, rhs)?);
                }
            },
            UdfOp::Dup => {
                let top = *stack.last()?;
                stack.push(top);
            }
            UdfOp::Swap => {
                let len = stack.len();
                if len < 2 {
                    return None;
                }
                stack.swap(len - 1, len - 2);
            }
        }
    }
    if stack.len() == 1 {
        base_finite(stack.pop()?)
    } else {
        None
    }
}

/// True when the real values of both expressions at `target` agree to eight ulps.
pub(crate) fn certified_equal(
    lhs: &Expression,
    rhs: &Expression,
    target: f64,
    context: &EvalContext<'_>,
) -> bool {
    let (Some(lhs), Some(rhs)) = (
        evaluate(lhs, target, context),
        evaluate(rhs, target, context),
    ) else {
        return false;
    };
    if lhs.loose || rhs.loose || !lhs.hi.is_finite() || !rhs.hi.is_finite() {
        return false;
    }
    let diff = (lhs - rhs).abs();
    // An exact 0 is only the pair of zero words. A nonzero residual compared
    // with 0 is larger than eight ulps of itself, so it is not exact:
    // `sinpi(cospi(x))` at `1e-16` is about `1.55e-31`, not 0. Half-integer
    // `cospi` and integer `sinpi` return an exact 0 from the snap, so those
    // identities do not need a residual allowance.
    if diff.hi == 0.0 && diff.lo == 0.0 {
        return true;
    }
    let scale = lhs.abs().hi.abs().max(rhs.abs().hi.abs());
    scale > 0.0 && diff.hi.abs() <= 8.0 * ulp_of(scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;

    fn agrees(lhs: &str, rhs: &str, target: f64) -> bool {
        let lhs = Expression::parse(lhs).unwrap();
        let rhs = Expression::parse(rhs).unwrap();
        certified_equal(&lhs, &rhs, target, &EvalContext::new())
    }

    #[test]
    fn rounded_reciprocal_trig_is_not_exact() {
        let tiny = 1e-15;
        assert!(!agrees("xrC", "8rC", tiny), "cospi(1/x) = cospi(1/8)");
        assert!(!agrees("xrC", "8rC", -tiny));
        assert!(!agrees("xp+C", "8rCn", 1e15), "cospi(pi+x) = -cospi(1/8)");
        assert!(!agrees("xrSE", "1", 1e-12), "e^sinpi(1/x) = 1 at 1e-12");
    }

    #[test]
    fn tiny_nonzero_residual_is_not_exact_zero() {
        // True zeros at the origin stay exact: the snap and exp(0) are exact.
        assert!(agrees("xCS", "1l", 0.0), "sinpi(cospi(0)) = 0");
        assert!(agrees("xE1-", "1l", 0.0), "e^0 - 1 = 0");
        assert!(agrees("xES", "1l", 0.0), "sinpi(e^0) = 0");
        // The same shapes at a tiny target are a real gap, not a rounding.
        assert!(!agrees("xCS", "1l", 1e-16), "sinpi(cospi(x)) = 0 at 1e-16");
        assert!(!agrees("1xC-", "1l", 1e-16), "1 - cospi(x) = 0 at 1e-16");
        assert!(!agrees("xCS", "1S", 2e-16), "sinpi(cospi(x)) = 0 at 2e-16");
        assert!(!agrees("xE1-", "1l", 1e-31), "e^x - 1 = 0 at 1e-31");
        assert!(!agrees("xES", "1l", 1e-31), "sinpi(e^x) = 0 at 1e-31");
        assert!(
            !agrees("xsCS", "1l", 1e-15),
            "sinpi(cospi(x^2)) = 0 at 1e-15"
        );
        assert!(
            !agrees("xsCS", "1l", 1e-12),
            "sinpi(cospi(x^2)) = 0 at 1e-12"
        );
        // Underflow is not a proof of zero. The smallest subnormal divided
        // by two is a nonzero subnormal, and `x*1` at 0 really is zero.
        assert!(!agrees("x2/", "1l", 5e-324), "x/2 = 0 at 5e-324");
        assert!(!agrees("x2r*", "1l", 5e-324), "x*(1/2) = 0 at 5e-324");
        assert!(agrees("x1*", "1l", 0.0), "x*1 = 0 at 0");
    }

    #[test]
    fn genuine_exact_identities_stay_exact() {
        assert!(agrees("2x*", "5", 2.5));
        assert!(agrees("x", "52/", 2.5));
        assert!(agrees("x", "32/", 1.5));
        assert!(agrees("x", "2", 2.0));
        assert!(agrees("x", "p", std::f64::consts::PI));
        assert!(agrees("x", "e", std::f64::consts::E));
        assert!(agrees("x", "f", crate::eval::constants::PHI));
        assert!(agrees("x", "2q", 2.0_f64.sqrt()));
        assert!(agrees("x", "1l", 0.0));
        assert!(agrees("x", "1S", 0.0));
        assert!(agrees("x", "1l", -0.0));
        assert!(agrees("5sx*", "38r-", 0.115));
        assert!(agrees("xS", "1l", 2.0_f64.powi(53)));
        assert!(agrees("1l", "1S", 1.0));
    }

    #[test]
    fn double_double_matches_known_constants() {
        let pi = Dd::pi();
        let (sin, cos) = sin_cos_pi(Dd::from_f64(0.125)).unwrap();
        assert!(cos.hi > 0.9 && (cos.hi - 0.9238795325112867).abs() < 1e-15);
        assert!((sin.hi - 0.3826834323650898).abs() < 1e-15);
        let e = exp(Dd::one()).unwrap();
        assert!((e.hi - std::f64::consts::E).abs() < 1e-15);
        let ln2 = ln(Dd::from_f64(2.0)).unwrap();
        assert!((ln2.hi - std::f64::consts::LN_2).abs() < 1e-15);
        let sqrt2 = sqrt(Dd::from_f64(2.0)).unwrap();
        assert!((sqrt2.hi - 2.0_f64.sqrt()).abs() < 1e-15);
        let quarter = atan2(Dd::one(), Dd::one()).unwrap();
        assert!((quarter.hi - pi.hi * 0.25).abs() < 1e-15);
        let _ = pi;
    }

    fn within_ulps(got: f64, expect: f64, ulps: f64) -> bool {
        let scale = got.abs().max(expect.abs()).max(1.0);
        (got - expect).abs() <= ulps * ulp_of(scale)
    }

    #[test]
    fn half_integer_cospi_is_exact_zero() {
        for target in [0.5, -0.5, 1.5, -1.5, 2.5, 2.0_f64.powi(50) + 0.5] {
            assert!(agrees("xC", "1l", target), "cospi({target}) = 0 via ln(1)");
            assert!(agrees("xC", "1S", target), "cospi({target}) = sinpi(1)");
        }
        assert!(agrees("xS", "1", 0.5), "sinpi(1/2) = 1");
        assert!(agrees("xS", "1n", -0.5), "sinpi(-1/2) = -1");
        assert!(agrees("xS", "1n", 1.5), "sinpi(3/2) = -1");
        // Integers stay exact, including magnitudes past the loose cutoff.
        assert!(agrees("xS", "1l", 2.0));
        assert!(agrees("xS", "1l", 2.0_f64.powi(53)));
        assert!(agrees("xS", "1l", 2.0_f64.powi(62)));
        let (sin, cos) = sin_cos_pi(Dd::from_f64(2.0_f64.powi(60))).unwrap();
        assert_eq!(sin.hi, 0.0);
        assert_eq!(cos.hi, 1.0);
        assert!(!sin.loose);
        // A non-integer at the conservative cutoff is not a faithful fraction.
        let cutoff = trig_loose_magnitude();
        let shaky = renormalize(cutoff, 0.125, false, false);
        assert!(sin_cos_pi(shaky).unwrap().0.loose);
        let below = Dd::from_f64(2.0_f64.powi(52) + 0.25);
        assert!(!sin_cos_pi(below).unwrap().0.loose);
    }

    #[test]
    fn atan_matches_f64_across_the_range_and_atan2_quadrants() {
        for value in [0.1, 0.5, 1.0, 1.2, 2.0, 10.0, 1e8] {
            let got = atan(Dd::from_f64(value)).unwrap();
            assert!(
                within_ulps(got.hi, value.atan(), 1.0),
                "atan({value}) got {} expect {}",
                got.hi,
                value.atan()
            );
            let neg = atan(Dd::from_f64(-value)).unwrap();
            assert!(within_ulps(neg.hi, (-value).atan(), 1.0));
        }
        let samples = [
            (1.0, 1.0),
            (1.0, -1.0),
            (-1.0, -1.0),
            (-1.0, 1.0),
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.0, -1.0),
            (10.0, -0.5),
        ];
        for (y, x) in samples {
            let got = atan2(Dd::from_f64(y), Dd::from_f64(x)).unwrap();
            assert!(
                within_ulps(got.hi, y.atan2(x), 1.0),
                "atan2({y}, {x}) got {} expect {}",
                got.hi,
                y.atan2(x)
            );
        }
        assert!(agrees("x1A", "p2/1xA-", 2.0));
        assert!(agrees("x1A", "p2/1xA-", 10.0));
    }

    #[test]
    fn lambert_w_covers_its_domain() {
        let branch = -1.0 / std::f64::consts::E;
        let at_branch = lambert_w(Dd::from_f64(branch)).unwrap();
        assert_eq!(at_branch.hi, -1.0);
        assert!(lambert_w(Dd::from_f64(branch.next_down())).is_none());
        let omega = 0.5671432904097838_f64;
        let w1 = lambert_w(Dd::one()).unwrap();
        assert!(
            within_ulps(w1.hi, omega, 1.0),
            "W(1) got {} expect {omega}",
            w1.hi
        );
        assert!(agrees("x", "1W", omega));
        assert!(agrees("xxE*", "1", omega));
        assert!(agrees("xW", "1", std::f64::consts::E));
        for value in [-0.2, 0.5, 2.0, 10.0] {
            let got = lambert_w(Dd::from_f64(value)).unwrap();
            let back = got * exp(got).unwrap();
            assert!(
                within_ulps(back.hi, value, 8.0),
                "W({value})*exp(W) got {} ",
                back.hi
            );
        }
    }

    #[test]
    fn computed_huge_trig_is_not_an_integer_identity() {
        assert!(
            !agrees("xES", "1l", 123.456789),
            "sinpi(e^x) = 0 is not exact at 123.456789"
        );
        assert!(
            !agrees("xET", "1l", 123.456789),
            "tanpi(e^x) = 0 is not exact at 123.456789"
        );
        assert!(
            !agrees("px*S", "1S", 2.0_f64.powi(52)),
            "sinpi(pi*x) = 0 is not exact at 2^52"
        );
        assert!(
            !agrees("xp*S", "1S", 2.0_f64.powi(53)),
            "sinpi(x*pi) = 0 is not exact at 2^53"
        );
        assert!(
            !agrees("ex*S", "1S", 2.0_f64.powi(52)),
            "sinpi(e*x) = 0 is not exact at 2^52"
        );
        assert!(
            !agrees("fx*S", "1S", 2.0_f64.powi(52)),
            "sinpi(phi*x) = 0 is not exact at 2^52"
        );
        assert!(agrees("1x+S", "1S", 1e300), "sinpi(1+x) = 0 at 1e300");
        assert!(agrees("2x*S", "1S", 1e300), "sinpi(2*x) = 0 at 1e300");
        assert!(
            !agrees("px*S", "1S", 1e300),
            "sinpi(pi*x) = 0 is not exact at 1e300"
        );
        let half_past_mantissa = 2.0_f64.powi(51) + 0.5;
        assert!(
            !agrees("x3^S", "1S", half_past_mantissa),
            "sinpi(x^3) = 0 is not exact at 2^51+1/2"
        );
        assert!(
            !agrees("x4^S", "1S", half_past_mantissa),
            "sinpi(x^4) = 0 is not exact at 2^51+1/2"
        );
        assert!(agrees("x2/S", "1S", 1e300), "sinpi(x/2) = 0 at 1e300");
        assert!(agrees("9x^", "4rT", 0.0), "9^x = tanpi(1/4) at 0");
        assert!(agrees("2rx+", "6rS", 0.0), "1/2+x = sinpi(1/6) at 0");
    }

    #[test]
    fn ln_of_smallest_subnormal_is_the_power_of_two() {
        let got = ln(Dd::from_f64(5e-324)).unwrap();
        let expect = Dd::from_i32(-1074) * Dd::ln2();
        let diff = (got - expect).abs();
        let scale = expect.abs().hi.abs();
        assert!(
            diff.hi.abs() <= 8.0 * ulp_of(scale),
            "ln(5e-324) got {} expect {}",
            got.hi,
            expect.hi
        );
        let (mantissa, exponent) = frexp(5e-324);
        assert_eq!(mantissa, 0.5);
        assert_eq!(exponent, -1073);
    }
}
