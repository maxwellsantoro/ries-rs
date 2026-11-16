//! An extra rounding-sensitivity gate for the numerical `exact` label.
//!
//! Reuse the ordinary evaluator, carrying a forward roundoff budget instead of
//! implementing a second transcendental library. In particular, a rounded trig
//! argument or `exp(x)` rounded to one must not acquire unlimited precision when
//! used by the next operator. This is a conservative numerical guard, not a
//! certified interval, symbolic equality test, or independent verification.

use super::{eval_binary, eval_constant_with_user, eval_unary, EvalContext, StackEntry};
use crate::{
    expr::Expression,
    symbol::{NumType, Seft, Symbol},
    udf::UdfOp,
};
use smallvec::SmallVec;

#[derive(Clone, Copy)]
struct Tracked {
    point: StackEntry,
    noise: f64,
}

type Stack = SmallVec<[Tracked; 21]>;

fn add(a: f64, b: f64) -> f64 {
    if a == 0.0 && b == 0.0 {
        0.0
    } else {
        (a + b).next_up()
    }
}

fn mul(a: f64, b: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() {
        f64::INFINITY
    } else if a == 0.0 || b == 0.0 {
        0.0
    } else {
        (a * b).next_up()
    }
}

fn div(a: f64, b: f64) -> f64 {
    if a == 0.0 {
        0.0
    } else if b <= 0.0 {
        f64::INFINITY
    } else {
        (a / b).next_up()
    }
}

fn lower(value: f64, noise: f64) -> f64 {
    if noise == 0.0 {
        value
    } else {
        (value - noise).next_down()
    }
}

fn ulp(value: f64) -> f64 {
    crate::search::ulp_of(value.abs())
}

// Exact binary-rational product comparison. FMA alone cannot detect a residual
// smaller than the least subnormal, and rounded integral words lose parity.
fn binary(value: f64) -> (u128, i32) {
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let significand = (bits & ((1_u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1_u64 << 52 };
    let shift = significand.trailing_zeros();
    (
        (significand >> shift) as u128,
        if exponent == 0 {
            -1074
        } else {
            exponent - 1075
        } + shift as i32,
    )
}

fn product_exact(a: f64, b: f64, value: f64) -> bool {
    if a == 0.0 || b == 0.0 {
        return value == 0.0;
    }
    if value == 0.0
        || !value.is_finite()
        || a.is_sign_negative() ^ b.is_sign_negative() != value.is_sign_negative()
    {
        return false;
    }
    let (a, ae) = binary(a);
    let (b, be) = binary(b);
    let (v, ve) = binary(value);
    a * b == v && ae + be == ve
}

fn sum_roundoff(a: f64, b: f64) -> f64 {
    let sum = a + b;
    let recovered = sum - a;
    ((a - (sum - recovered)) + (b - recovered)).abs()
}

fn arithmetic(sym: Symbol, a: Tracked, b: Tracked, value: f64) -> f64 {
    let av = a.point.val;
    let bv = b.point.val;
    match sym {
        Symbol::Add | Symbol::Sub => add(
            add(a.noise, b.noise),
            sum_roundoff(av, if sym == Symbol::Add { bv } else { -bv }),
        ),
        Symbol::Mul => {
            let roundoff = if product_exact(av, bv, value) {
                0.0
            } else {
                av.mul_add(bv, -value)
                    .abs()
                    .max(f64::from_bits(1))
                    .next_up()
            };
            add(
                add(
                    add(mul(av.abs(), b.noise), mul(bv.abs(), a.noise)),
                    mul(a.noise, b.noise),
                ),
                roundoff,
            )
        }
        Symbol::Div => {
            let denominator = lower(bv.abs(), b.noise);
            let roundoff = if product_exact(bv, value, av) {
                0.0
            } else {
                ulp(value)
            };
            if denominator <= 0.0 {
                f64::INFINITY
            } else {
                add(
                    div(add(a.noise, mul(value.abs(), b.noise)), denominator),
                    roundoff,
                )
            }
        }
        _ => unreachable!(),
    }
}

fn exact_power(base: f64, exponent: f64, value: f64) -> bool {
    if exponent.fract() != 0.0 || exponent.abs() > 1024.0 {
        return false;
    }
    let mut n = exponent.abs() as u32;
    let mut factor = base;
    let mut result = 1.0;
    while n != 0 {
        if n & 1 != 0 {
            let product = result * factor;
            if !product_exact(result, factor, product) {
                return false;
            }
            result = product;
        }
        n >>= 1;
        if n != 0 {
            let squared = factor * factor;
            if !product_exact(factor, factor, squared) {
                return false;
            }
            factor = squared;
        }
    }
    if exponent < 0.0 {
        product_exact(result, value, 1.0)
    } else {
        result == value
    }
}

fn power_noise(base: Tracked, exponent: Tracked, value: f64) -> f64 {
    let b = base.point.val;
    let e = exponent.point.val;
    if (b == 1.0 && base.noise == 0.0) || (e == 0.0 && exponent.noise == 0.0) {
        return 0.0;
    }
    if b == 0.0 && base.noise == 0.0 && e > exponent.noise {
        return 0.0;
    }
    if value == 0.0 && (base.noise != 0.0 || exponent.noise != 0.0) {
        return f64::INFINITY;
    }
    if b < 0.0 && (e.fract() != 0.0 || exponent.noise != 0.0) {
        return f64::INFINITY;
    }
    let floor = lower(b.abs(), base.noise);
    if floor <= 0.0 {
        return f64::INFINITY;
    }
    let log_noise = div(base.noise, floor);
    let amplified = add(
        mul(e.abs(), log_noise),
        mul(add(b.abs().ln().abs(), log_noise), exponent.noise),
    );
    let roundoff = if exact_power(b, e, value) {
        0.0
    } else {
        ulp(value)
    };
    if amplified == 0.0 {
        roundoff
    } else {
        add(
            mul(add(value.abs(), roundoff), amplified.exp_m1()),
            roundoff,
        )
    }
}

fn tracked(point: StackEntry, noise: f64) -> Option<Tracked> {
    (point.val.is_finite() && !noise.is_nan()).then_some(Tracked { point, noise })
}

fn constant(value: f64) -> Tracked {
    Tracked {
        point: StackEntry::constant(value, NumType::Integer),
        noise: 0.0,
    }
}

fn unary(sym: Symbol, a: Tracked, context: &EvalContext<'_>) -> Option<Tracked> {
    let point = eval_unary(sym, a.point, context.trig_argument_scale).ok()?;
    let value = point.val;
    let arg = a.point.val;
    let noise = match sym {
        Symbol::Neg => a.noise,
        Symbol::Recip => arithmetic(Symbol::Div, constant(1.0), a, value),
        Symbol::Square => arithmetic(Symbol::Mul, a, a, value),
        Symbol::Sqrt => {
            let floor = (arg - a.noise).max(0.0);
            let roundoff = if product_exact(value, value, arg) {
                0.0
            } else {
                ulp(value)
            };
            if a.noise > arg {
                f64::INFINITY
            } else {
                add(
                    div(a.noise, (floor.sqrt() + value.abs()).next_down()),
                    roundoff,
                )
            }
        }
        Symbol::Ln => {
            let floor = lower(arg, a.noise);
            if floor <= 0.0 {
                f64::INFINITY
            } else {
                add(
                    div(a.noise, floor),
                    if arg == 1.0 { 0.0 } else { ulp(value) },
                )
            }
        }
        Symbol::Exp => {
            let roundoff = if arg == 0.0 { 0.0 } else { ulp(value) };
            if a.noise == 0.0 {
                roundoff
            } else if value == 0.0 {
                f64::INFINITY
            } else {
                add(mul(add(value.abs(), roundoff), a.noise.exp_m1()), roundoff)
            }
        }
        Symbol::SinPi | Symbol::CosPi | Symbol::TanPi => {
            let default_scale = context.trig_argument_scale == super::DEFAULT_TRIG_ARGUMENT_SCALE;
            let angle_noise = if default_scale {
                mul(context.trig_argument_scale.abs(), a.noise)
            } else {
                let scaled = arg * context.trig_argument_scale;
                arithmetic(
                    Symbol::Mul,
                    a,
                    constant(context.trig_argument_scale),
                    scaled,
                )
            };
            let special =
                (default_scale && (arg.fract() == 0.0 || (arg * 2.0).fract() == 0.0)) || arg == 0.0;
            let roundoff = if special { 0.0 } else { 2.0 * ulp(value) };
            if sym == Symbol::TanPi && angle_noise != 0.0 {
                f64::INFINITY
            } else {
                add(angle_noise.min(2.0), roundoff)
            }
        }
        Symbol::LambertW => {
            // Keep uncertainty from the argument even when its rounded value is
            // zero. Avoid a new root finder or an unjustified derivative bound
            // across the principal branch point.
            if a.noise != 0.0 {
                f64::INFINITY
            } else if arg == 0.0 {
                0.0
            } else if arg == std::f64::consts::E && value == 1.0 {
                // W(e)=1, and W' <= 1 for positive arguments. Include the
                // stored constant's distance from mathematical e.
                ulp(arg)
            } else if arg.abs() <= 1e-8 {
                // Near zero, |W(x)-x| <= 2*x^2 on the principal branch.
                // Include the solver's displacement from x as well. Its
                // absolute residual stopping rule alone does not imply ulps.
                add((value - arg).abs(), mul(2.0, mul(arg.abs(), arg.abs())))
            } else {
                // The ordinary solver is an approximation, not an ulp-bounded
                // elementary function. Keep the candidate without exactness.
                f64::INFINITY
            }
        }
        _ => return None,
    };
    tracked(point, noise)
}

fn binary_op(sym: Symbol, a: Tracked, b: Tracked) -> Option<Tracked> {
    let point = eval_binary(sym, a.point, b.point).ok()?;
    let noise = match sym {
        Symbol::Add | Symbol::Sub | Symbol::Mul | Symbol::Div => arithmetic(sym, a, b, point.val),
        Symbol::Pow => power_noise(a, b, point.val),
        Symbol::Root => {
            if a.point.val == 0.0 || a.noise >= a.point.val.abs() {
                return tracked(point, f64::INFINITY);
            }
            let reciprocal = 1.0 / a.point.val;
            let exponent = tracked(
                StackEntry::constant(reciprocal, NumType::Rational),
                arithmetic(Symbol::Div, constant(1.0), a, reciprocal),
            )?;
            if b.point.val < 0.0 && (a.noise != 0.0 || a.point.val.fract() != 0.0) {
                f64::INFINITY
            } else {
                power_noise(
                    Tracked {
                        point: StackEntry::constant(b.point.val.abs(), b.point.num_type),
                        ..b
                    },
                    exponent,
                    point.val.abs(),
                )
            }
        }
        Symbol::Log => {
            let numerator = unary(Symbol::Ln, b, &EvalContext::new())?;
            let denominator = unary(Symbol::Ln, a, &EvalContext::new())?;
            arithmetic(Symbol::Div, numerator, denominator, point.val)
        }
        Symbol::Atan2 => {
            if a.noise == 0.0 && b.noise == 0.0 {
                2.0 * ulp(point.val)
            } else if a.noise != 0.0
                && a.point.val.abs() <= a.noise
                && lower(b.point.val, b.noise) < 0.0
            {
                // A tiny uncertain y can cross the negative x axis, changing
                // atan2 by a full turn. A local slope budget misses that cut.
                f64::INFINITY
            } else {
                let radius = (a.point.val.hypot(b.point.val) - add(a.noise, b.noise)).next_down();
                if radius <= 0.0 || !radius.is_finite() {
                    f64::INFINITY
                } else {
                    add(div(add(a.noise, b.noise), radius), 2.0 * ulp(point.val))
                }
            }
        }
        _ => return None,
    };
    tracked(point, noise)
}

fn apply(
    sym: Symbol,
    stack: &mut Stack,
    x: f64,
    context: &EvalContext<'_>,
    in_function: bool,
) -> Option<()> {
    if let Some(index) = sym.user_function_index() {
        if in_function {
            return None;
        }
        let input = stack.pop()?;
        let function = context.user_functions.get(index as usize)?;
        let mut body = Stack::new();
        body.push(input);
        for op in &function.body {
            match op {
                UdfOp::Symbol(Symbol::X) => return None,
                UdfOp::Symbol(symbol) => apply(*symbol, &mut body, 0.0, context, true)?,
                UdfOp::Dup => body.push(*body.last()?),
                UdfOp::Swap => {
                    let len = body.len();
                    if len < 2 {
                        return None;
                    }
                    body.swap(len - 1, len - 2);
                }
            }
        }
        if body.len() != 1 {
            return None;
        }
        stack.push(body.pop()?);
    } else {
        match sym.seft() {
            Seft::A => stack.push(Tracked {
                point: eval_constant_with_user(sym, x, context.user_constants).ok()?,
                noise: 0.0,
            }),
            Seft::B => {
                let a = stack.pop()?;
                stack.push(unary(sym, a, context)?);
            }
            Seft::C => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(binary_op(sym, a, b)?);
            }
        }
    }
    Some(())
}

fn evaluate(expr: &Expression, x: f64, context: &EvalContext<'_>) -> Option<Tracked> {
    let mut stack = Stack::new();
    let mut index = 0;
    while index < expr.len() {
        if let Some((len, value)) = expr.decimal_literal_at(index) {
            stack.push(constant(value));
            index += len;
        } else {
            apply(expr.symbols()[index], &mut stack, x, context, false)?;
            index += 1;
        }
    }
    if stack.len() == 1 {
        stack.pop()
    } else {
        None
    }
}

pub(crate) fn exactness_resolved(
    lhs: &Expression,
    rhs: &Expression,
    x: f64,
    context: &EvalContext<'_>,
) -> bool {
    let (Some(a), Some(b)) = (evaluate(lhs, x, context), evaluate(rhs, x, context)) else {
        return false;
    };
    if a.point.val == 0.0 || b.point.val == 0.0 {
        return a.point.val == b.point.val && a.noise == 0.0 && b.noise == 0.0;
    }
    // Keep the same side tolerance as match_exactness; account for both the
    // observed residual and the propagated roundoff inside that allowance.
    add((a.point.val - b.point.val).abs(), add(a.noise, b.noise))
        <= 8.0 * f64::EPSILON * a.point.val.abs().max(b.point.val.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolves(lhs: &str, rhs: &str, target: f64) -> bool {
        exactness_resolved(
            &Expression::parse(lhs).unwrap(),
            &Expression::parse(rhs).unwrap(),
            target,
            &EvalContext::new(),
        )
    }

    #[test]
    fn lost_bits_are_not_reinterpreted_as_integer_parity_or_divisibility() {
        // (2^53+1)^2 is odd, and half of it is a half-integer. The rounded
        // f64 square loses those residues; do not promote its snaps to exact.
        assert!(!resolves("1x+sC", "1", 2.0_f64.powi(53)));
        assert!(!resolves("1x+s2/S", "1l", 2.0_f64.powi(53)));
    }

    #[test]
    fn zero_requires_zero_roundoff_including_underflow_and_cancellation() {
        for (lhs, target) in [
            ("xCS", 1e-16),
            ("xE1-", 1e-31),
            ("1x+l", 1e-310),
            ("x2/", f64::from_bits(1)),
            ("x2/W", f64::from_bits(1)),
            ("xE", -1000.0),
        ] {
            assert!(!resolves(lhs, "1l", target), "{lhs} at {target}");
        }
        assert!(resolves("xx-", "1l", f64::from_bits(1)));
        assert!(resolves("xE1-", "1l", 0.0));
    }

    #[test]
    fn angular_uncertainty_cannot_cross_the_atan2_branch_cut() {
        assert!(!resolves("1x+1-1nA", "p", -1e-20));
        assert!(!resolves("1x+1-1nA", "p", 1e-20));
        assert!(resolves("1x+1-1nA", "p", 0.0));
    }

    #[test]
    fn rounded_zero_does_not_erase_uncertain_exponential_or_power_inputs() {
        let uncertain = Tracked {
            point: constant(-746.0).point,
            noise: 100.0,
        };
        assert_eq!(
            unary(Symbol::Exp, uncertain, &EvalContext::new())
                .unwrap()
                .noise,
            f64::INFINITY
        );
        let uncertain_power = Tracked {
            point: constant(2.0).point,
            noise: 1.0,
        };
        assert_eq!(
            power_noise(uncertain_power, constant(-1076.0), 0.0),
            f64::INFINITY
        );
    }

    #[test]
    fn ordinary_constants_finite_extremes_and_direct_trig_controls_stay_resolved() {
        for (lhs, rhs, target) in [
            ("x", "p", std::f64::consts::PI),
            ("x", "2q", 2.0_f64.sqrt()),
            ("2x*", "5", 2.5),
            ("xC", "1l", 0.5),
            ("xW", "x", -1e-100),
            ("xW", "x", 0.0),
            ("xW", "1", std::f64::consts::E),
            ("xEl", "x", 709.5),
            ("xlE", "x", 2.0),
            ("3xv", "2n", -8.0),
        ] {
            assert!(resolves(lhs, rhs, target), "{lhs}={rhs} at {target}");
        }
        for target in [2.0_f64.powi(53), 1e300, f64::MAX] {
            assert!(resolves("xS", "1l", target), "sinpi integer {target}");
            assert!(resolves("xC", "1", target), "cospi even integer {target}");
        }
    }

    #[test]
    fn configured_functions_keep_their_argument_uncertainty() {
        let function = crate::UserFunction::parse("1:f:reciprocal cosine:rC").unwrap();
        let context = EvalContext::from_slices(&[], std::slice::from_ref(&function));
        assert!(!exactness_resolved(
            &Expression::parse("xH").unwrap(),
            &Expression::parse("8rC").unwrap(),
            1e-15,
            &context
        ));
        let radians = EvalContext::new().with_trig_argument_scale(1.0);
        assert!(!exactness_resolved(
            &Expression::parse("xrSE").unwrap(),
            &Expression::parse("1").unwrap(),
            1e-12,
            &radians
        ));
    }
}
