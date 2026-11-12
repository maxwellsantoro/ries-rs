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

const TRIG_LOOSE_MAGNITUDE: f64 = 1_152_921_504_606_846_976.0; // 2^60

#[derive(Clone, Copy)]
struct Dd {
    hi: f64,
    lo: f64,
    /// The fractional part of a transcendental argument was too large to trust.
    loose: bool,
}

impl Dd {
    fn new(hi: f64, lo: f64, loose: bool) -> Self {
        Self { hi, lo, loose }
    }

    fn zero() -> Self {
        Self::new(0.0, 0.0, false)
    }

    fn one() -> Self {
        Self::new(1.0, 0.0, false)
    }

    fn from_f64(value: f64) -> Self {
        Self::new(value, 0.0, false)
    }

    fn from_i32(value: i32) -> Self {
        Self::from_f64(f64::from(value))
    }

    fn pi() -> Self {
        // Extra word of π past the f64 constant. The high word is exactly
        // `f64::consts::PI`, so `x = pi` differs only by this word.
        Self::new(std::f64::consts::PI, 1.2246467991473532e-16, false)
    }

    fn ln2() -> Self {
        Self::new(std::f64::consts::LN_2, 2.3190468138462996e-17, false)
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
            return renormalize(self.hi, self.lo.floor(), self.loose);
        }
        let frac = (self.hi - hi_floor) + self.lo;
        if frac >= 1.0 {
            Self::from_f64(hi_floor + 1.0).flag(self.loose)
        } else if frac < 0.0 {
            Self::from_f64(hi_floor - 1.0).flag(self.loose)
        } else {
            Self::from_f64(hi_floor).flag(self.loose)
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
        Self::new(-self.hi, -self.lo, self.loose)
    }
}

impl std::ops::Add for Dd {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let (sum, err) = two_sum(self.hi, rhs.hi);
        renormalize(sum, err + self.lo + rhs.lo, self.loose || rhs.loose)
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
        let loose = self.loose
            || rhs.loose
            || self.hi.abs() > 1e290
            || rhs.hi.abs() > 1e290
            || !prod.is_finite();
        renormalize(prod, err + cross, loose)
    }
}

impl std::ops::Div for Dd {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let q = self.hi / rhs.hi;
        let (prod, err) = two_prod(q, rhs.hi);
        let remainder = (((self.hi - prod) - err) + self.lo) - q * rhs.lo;
        let loose = self.loose || rhs.loose || !q.is_finite();
        renormalize(q, remainder / rhs.hi, loose)
    }
}

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let sum = a + b;
    let bb = sum - a;
    let err = (a - (sum - bb)) + (b - bb);
    (sum, err)
}

fn renormalize(a: f64, b: f64, loose: bool) -> Dd {
    let (hi, lo) = two_sum(a, b);
    Dd::new(hi, lo, loose || !hi.is_finite())
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
    )
}

fn frexp(value: f64) -> (f64, i32) {
    if value == 0.0 || !value.is_finite() {
        return (value, 0);
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1022;
    let mantissa = f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000);
    (mantissa, exponent)
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
    if fraction.hi == 0.0 && fraction.lo == 0.0 {
        let sign = if is_even_integer(nearest) { 1.0 } else { -1.0 };
        return Some((Dd::zero(), Dd::from_f64(sign)));
    }
    let loose = value.loose || value.hi.abs() > TRIG_LOOSE_MAGNITUDE;
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
    let mut mantissa = renormalize(mantissa_hi, value.lo / 2.0_f64.powi(exponent), value.loose);
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

fn atan(value: Dd) -> Option<Dd> {
    if !value.hi.is_finite() {
        return None;
    }
    if value.hi == 0.0 && value.lo == 0.0 {
        return Some(Dd::zero());
    }
    // Euler's series stays accurate at |z| = 1, where the Taylor series does not.
    let square = value * value;
    let unit = square / (Dd::one() + square);
    let mut sum = Dd::one();
    let mut term = Dd::one();
    for n in 1..48 {
        term = term * unit * Dd::from_f64(f64::from(2 * n)) / Dd::from_f64(f64::from(2 * n + 1));
        sum = sum + term;
        if term.hi.abs() < 1e-28 {
            break;
        }
    }
    Some((value / (Dd::one() + square) * sum).flag(value.loose))
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
    // Below -1/e the real principal branch ends.
    if value.hi < -0.3678794411714423 {
        return None;
    }
    let mut w = if value.hi < 0.0 {
        Dd::from_f64(-0.5)
    } else if value.hi < 1.0 {
        value
    } else {
        let log = ln(value)?;
        log - ln(log)?
    };
    for _ in 0..16 {
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
    use crate::eval::constants::{APERY, CATALAN, E, GAMMA, PHI, PI, PLASTIC};
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
        Symbol::Pi => PI,
        Symbol::E => E,
        Symbol::Phi => PHI,
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
    let scale = lhs.abs().hi.abs().max(rhs.abs().hi.abs());
    let limit = if scale == 0.0 {
        0.0
    } else {
        8.0 * ulp_of(scale)
    };
    diff.hi.abs() <= limit
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
}
