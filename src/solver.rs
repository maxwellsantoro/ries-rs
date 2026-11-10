//! Analytical equation solver and expression canonicalization
//!
//! This module provides tools to solve symbolic equations for x and
//! to reduce expressions to a canonical form for deduplication.

use crate::eval::{evaluate_with_context, EvalContext, DEFAULT_TRIG_ARGUMENT_SCALE};
use crate::expr::Expression;
use crate::symbol::{Seft, Symbol};

/// Represents a node in a simplified expression AST used for solving.
#[derive(Clone)]
enum SolveNodeKind {
    Atom,
    Unary(Symbol, Box<SolveNode>),
    Binary(Symbol, Box<SolveNode>, Box<SolveNode>),
}

/// A node in the solver AST, tracking symbol count of 'x'.
#[derive(Clone)]
struct SolveNode {
    expr: Expression,
    x_count: u32,
    kind: SolveNodeKind,
}

/// Solves an equation `LHS = RHS` for `x`, returning a new expression for `x`.
///
/// This only succeeds if `x` appears exactly once in the `LHS` and the
/// equation can be analytically inverted using supported operations.
/// If solving fails, it returns `None`.
///
/// Multi-valued inverses (sine, cosine, tangent, and even roots) use the
/// **principal branch**. Call [`solve_for_x_at`] when the matched root is
/// known so the displayed expression evaluates to that root.
pub fn solve_for_x_rhs_expression(lhs: &Expression, rhs: &Expression) -> Option<Expression> {
    solve_for_x_inner(lhs, rhs, None, &EvalContext::new())
}

/// Solve `lhs = rhs` for `x`, choosing the real inverse branch nearest `x_value`.
///
/// Sine, cosine, tangent, and even roots are multi-valued. The principal
/// inverse (for example `asin(y)/π` or the non-negative square root) can sit
/// a full period away from the root the search refined. Passing that root
/// selects:
///
/// - square / even power: `√y` or `-√y`
/// - sine: `asin(y)/π + 2k` or `1 - asin(y)/π + 2k`
/// - cosine: `2k ± acos(y)/π`
/// - tangent: `atan(y)/π + k`
///
/// `x_value` is the numeric root of the original equation. The result is
/// `None` when `x_value` is not finite, the inverse is not real, or no
/// closed form evaluates to that root. A non-finite hint does not fall back
/// to the principal branch. Integer shifts are exact for every finite integer
/// `f64`, including magnitudes past 2^63.
pub fn solve_for_x_at(lhs: &Expression, rhs: &Expression, x_value: f64) -> Option<Expression> {
    solve_for_x_at_with_context(lhs, rhs, x_value, &EvalContext::new())
}

/// Solve at a matched root using the search's trig scale and user symbols.
pub fn solve_for_x_at_with_context(
    lhs: &Expression,
    rhs: &Expression,
    x_value: f64,
    context: &EvalContext<'_>,
) -> Option<Expression> {
    if !x_value.is_finite() {
        return None;
    }
    let solved = solve_for_x_inner(lhs, rhs, Some(x_value), context)?;
    let value = eval_with_context(&solved, 0.0, context)?;
    if !matches_hint(value, x_value) {
        return None;
    }
    let lhs_value = eval_with_context(lhs, value, context)?;
    let rhs_value = eval_with_context(rhs, value, context)?;
    values_agree(lhs_value, rhs_value).then_some(solved)
}

fn solve_for_x_inner(
    lhs: &Expression,
    rhs: &Expression,
    x_value: Option<f64>,
    context: &EvalContext<'_>,
) -> Option<Expression> {
    if lhs.count_symbol(Symbol::X) != 1 {
        return None;
    }

    let mut node = build_solve_ast(lhs)?;
    if node.x_count != 1 {
        return None;
    }
    let mut rhs_expr = rhs.clone();

    loop {
        match node.kind {
            SolveNodeKind::Atom => {
                // If it's the atom 'x', we've solved the equation
                return is_x_atom(&node.expr).then_some(rhs_expr);
            }
            SolveNodeKind::Unary(op, child) => {
                if child.x_count != 1 {
                    return None;
                }
                let target = x_value.and_then(|x| eval_with_context(&child.expr, x, context));
                rhs_expr = unary_inverse_expression(op, &rhs_expr, target, context)?;
                node = *child;
            }
            SolveNodeKind::Binary(op, left, right) => {
                let lx = left.x_count;
                let rx = right.x_count;
                if lx + rx != 1 {
                    return None;
                }

                if lx == 1 {
                    let target = x_value.and_then(|x| eval_with_context(&left.expr, x, context));
                    rhs_expr = invert_binary_left(op, &rhs_expr, &right.expr, target, context)?;
                    node = *left;
                } else {
                    rhs_expr = invert_binary_right(op, &left.expr, &rhs_expr)?;
                    node = *right;
                }
            }
        }
    }
}

/// Generates a canonical string key for an expression to identify equivalent forms.
///
/// This handles commutativity for addition and multiplication to ensure that
/// e.g., `x + 1` and `1 + x` produce the same key.
pub fn canonical_expression_key(expression: &Expression) -> Option<String> {
    let node = build_solve_ast(expression)?;
    Some(canonical_node_key(&node))
}

fn build_solve_ast(expression: &Expression) -> Option<SolveNode> {
    let mut stack: Vec<SolveNode> = Vec::with_capacity(expression.len());

    for &sym in expression.symbols() {
        match sym.seft() {
            Seft::A => {
                let mut e = Expression::new();
                e.push(sym);
                stack.push(SolveNode {
                    expr: e,
                    x_count: u32::from(sym == Symbol::X),
                    kind: SolveNodeKind::Atom,
                });
            }
            Seft::B => {
                let arg = stack.pop()?;
                let mut e = arg.expr.clone();
                e.push(sym);
                stack.push(SolveNode {
                    expr: e,
                    x_count: arg.x_count,
                    kind: SolveNodeKind::Unary(sym, Box::new(arg)),
                });
            }
            Seft::C => {
                let rhs = stack.pop()?;
                let lhs = stack.pop()?;
                let mut e = Expression::new();
                for &s in lhs.expr.symbols() {
                    e.push(s);
                }
                for &s in rhs.expr.symbols() {
                    e.push(s);
                }
                e.push(sym);
                stack.push(SolveNode {
                    expr: e,
                    x_count: lhs.x_count.saturating_add(rhs.x_count),
                    kind: SolveNodeKind::Binary(sym, Box::new(lhs), Box::new(rhs)),
                });
            }
        }
    }

    if stack.len() == 1 {
        stack.pop()
    } else {
        None
    }
}

/// Build an expression for the inverse of `op(child) = rhs_value`.
///
/// `target` is the numeric value of `child` at the matched root. When it is
/// `Some`, multi-valued inverses use the real branch nearest that value.
/// When it is `None`, the principal branch is returned.
///
/// Returns `None` if the operator has no supported closed-form inverse.
fn unary_inverse_expression(
    op: Symbol,
    rhs_value: &Expression,
    target: Option<f64>,
    context: &EvalContext<'_>,
) -> Option<Expression> {
    match op {
        Symbol::Neg => Some(append_unary_expression(rhs_value, Symbol::Neg)),
        Symbol::Recip => Some(append_unary_expression(rhs_value, Symbol::Recip)),
        Symbol::Square => {
            // x² = y ⟹ x = ±√y. Only the sign nearer `target` is real for that root.
            let radicand = nonnegative_argument(rhs_value, context)?;
            let principal = append_unary_expression(&radicand, Symbol::Sqrt);
            choose_signed_root(principal, target, context)
        }
        Symbol::Sqrt => Some(append_unary_expression(rhs_value, Symbol::Square)), // √x = y ⟹ x = y²
        Symbol::Ln => Some(append_unary_expression(rhs_value, Symbol::Exp)),
        Symbol::Exp => Some(append_unary_expression(rhs_value, Symbol::Ln)),
        Symbol::TanPi => {
            // tan(πx) = y ⟹ x = atan(y)/π + k. y must be finite.
            finite_constant(rhs_value, context)?;
            let principal = atan_over_pi(rhs_value);
            lift_periodic(principal, target, &[(false, 1, 0)], context)
        }
        Symbol::SinPi => {
            // sin(πx) = y ⟹ x = asin(y)/π + 2k  or  x = 1 - asin(y)/π + 2k
            // Real only for y in [-1, 1].
            let argument = unit_interval_argument(rhs_value, context)?;
            let principal = asin_over_pi(&argument);
            lift_periodic(principal, target, &[(false, 2, 0), (true, 2, 1)], context)
        }
        Symbol::CosPi => {
            // cos(πx) = y ⟹ x = 2k ± acos(y)/π. Real only for y in [-1, 1].
            let argument = unit_interval_argument(rhs_value, context)?;
            let principal = acos_over_pi(&argument);
            lift_periodic(principal, target, &[(false, 2, 0), (true, 2, 0)], context)
        }
        Symbol::LambertW => {
            // W(x) = y ⟹ x = y·eʸ. The inverse of W is single-valued on the reals.
            let exp_rhs = append_unary_expression(rhs_value, Symbol::Exp);
            Some(combine_binary_expressions(rhs_value, &exp_rhs, Symbol::Mul))
        }
        _ => None,
    }
}

/// `atan(y)/π = atan2(y, 1)/π`, principal value in (-1/2, 1/2).
fn atan_over_pi(rhs_value: &Expression) -> Expression {
    let one = constant_expression(Symbol::One);
    let atan = combine_binary_expressions(rhs_value, &one, Symbol::Atan2);
    let pi = constant_expression(Symbol::Pi);
    combine_binary_expressions(&atan, &pi, Symbol::Div)
}

/// `asin(y)/π = atan2(y, sqrt(1-y²))/π`, principal value in [-1/2, 1/2].
fn asin_over_pi(rhs_value: &Expression) -> Expression {
    let one = constant_expression(Symbol::One);
    let rhs_sq = append_unary_expression(rhs_value, Symbol::Square);
    let inner = combine_binary_expressions(&one, &rhs_sq, Symbol::Sub);
    let denom = append_unary_expression(&inner, Symbol::Sqrt);
    let atan = combine_binary_expressions(rhs_value, &denom, Symbol::Atan2);
    let pi = constant_expression(Symbol::Pi);
    combine_binary_expressions(&atan, &pi, Symbol::Div)
}

/// `acos(y)/π = atan2(sqrt(1-y²), y)/π`, principal value in [0, 1].
fn acos_over_pi(rhs_value: &Expression) -> Expression {
    let one = constant_expression(Symbol::One);
    let rhs_sq = append_unary_expression(rhs_value, Symbol::Square);
    let inner = combine_binary_expressions(&one, &rhs_sq, Symbol::Sub);
    let numer = append_unary_expression(&inner, Symbol::Sqrt);
    let atan = combine_binary_expressions(&numer, rhs_value, Symbol::Atan2);
    let pi = constant_expression(Symbol::Pi);
    combine_binary_expressions(&atan, &pi, Symbol::Div)
}

fn invert_binary_left(
    op: Symbol,
    rhs_value: &Expression,
    known_right: &Expression,
    target: Option<f64>,
    context: &EvalContext<'_>,
) -> Option<Expression> {
    match op {
        Symbol::Add => Some(combine_binary_expressions(
            rhs_value,
            known_right,
            Symbol::Sub,
        )),
        Symbol::Sub => Some(combine_binary_expressions(
            rhs_value,
            known_right,
            Symbol::Add,
        )),
        Symbol::Mul => Some(combine_binary_expressions(
            rhs_value,
            known_right,
            Symbol::Div,
        )),
        Symbol::Div => Some(combine_binary_expressions(
            rhs_value,
            known_right,
            Symbol::Mul,
        )),
        // x^k = y ⟹ x = y^(1/k). ± only when k is an even integer.
        Symbol::Pow => invert_power(rhs_value, known_right, target, context),
        Symbol::Root => Some(combine_binary_expressions(
            rhs_value,
            known_right,
            Symbol::Log,
        )),
        Symbol::Log => Some(combine_binary_expressions(
            rhs_value,
            known_right,
            Symbol::Root,
        )),
        _ => None,
    }
}

/// Inverse of `x^exponent = radicand`.
///
/// Even integers have two real roots when the radicand is non-negative. Odd
/// integers have one real root, including for a negative radicand. Other
/// exponents stay on the non-negative real root and are undefined for a
/// negative radicand.
fn invert_power(
    radicand: &Expression,
    exponent: &Expression,
    target: Option<f64>,
    context: &EvalContext<'_>,
) -> Option<Expression> {
    let y = finite_constant(radicand, context)?;
    let exponent_value = finite_constant(exponent, context)?;
    let even = is_even_integer_value(exponent_value);
    if even {
        if y < -1e-12 {
            return None;
        }
        let source = if y < 0.0 {
            zero_expression()
        } else {
            radicand.clone()
        };
        let root = combine_binary_expressions(exponent, &source, Symbol::Root);
        return choose_signed_root(root, target, context);
    }
    if y < 0.0 && !is_odd_integer_value(exponent_value) {
        return None;
    }
    Some(combine_binary_expressions(exponent, radicand, Symbol::Root))
}

fn invert_binary_right(
    op: Symbol,
    known_left: &Expression,
    rhs_value: &Expression,
) -> Option<Expression> {
    Some(match op {
        Symbol::Add => combine_binary_expressions(rhs_value, known_left, Symbol::Sub),
        Symbol::Sub => combine_binary_expressions(known_left, rhs_value, Symbol::Sub),
        Symbol::Mul => combine_binary_expressions(rhs_value, known_left, Symbol::Div),
        Symbol::Div => combine_binary_expressions(known_left, rhs_value, Symbol::Div),
        Symbol::Pow => combine_binary_expressions(known_left, rhs_value, Symbol::Log),
        Symbol::Root => combine_binary_expressions(rhs_value, known_left, Symbol::Pow),
        Symbol::Log => combine_binary_expressions(known_left, rhs_value, Symbol::Pow),
        _ => return None,
    })
}

fn append_unary_expression(base: &Expression, op: Symbol) -> Expression {
    let mut out = base.clone();
    out.push(op);
    out
}

fn combine_binary_expressions(lhs: &Expression, rhs: &Expression, op: Symbol) -> Expression {
    let mut out = lhs.clone();
    let shift = lhs.len() as u32;
    for &sym in rhs.symbols() {
        out.push(sym);
    }
    out.append_decimal_literals(rhs, shift);
    out.push(op);
    out
}

fn constant_expression(sym: Symbol) -> Expression {
    let mut out = Expression::new();
    out.push(sym);
    out
}

fn is_x_atom(expression: &Expression) -> bool {
    expression.len() == 1
        && expression
            .symbols()
            .first()
            .is_some_and(|sym| *sym == Symbol::X)
}

fn eval_expr(expr: &Expression, x: f64) -> Option<f64> {
    eval_with_context(expr, x, &EvalContext::new())
}

fn eval_with_context(expr: &Expression, x: f64, context: &EvalContext<'_>) -> Option<f64> {
    evaluate_with_context(expr, x, context)
        .ok()
        .map(|result| result.value)
}

/// Keep `principal` (a non-negative root) or its negation, whichever is nearer `target`.
///
/// The margin is relative to the root, so a negative root smaller than `1e-12`
/// still wins. A tie, including a root at 0, stays non-negative.
fn choose_signed_root(
    principal: Expression,
    target: Option<f64>,
    context: &EvalContext<'_>,
) -> Option<Expression> {
    let positive = eval_with_context(&principal, 0.0, context)?;
    if !positive.is_finite() {
        return None;
    }
    let Some(target) = target else {
        return Some(principal);
    };
    let negative = append_unary_expression(&principal, Symbol::Neg);
    let Some(neg_value) = eval_with_context(&negative, 0.0, context) else {
        return Some(principal);
    };
    if !neg_value.is_finite() {
        return Some(principal);
    }
    let pos_err = (positive - target).abs();
    let neg_err = (neg_value - target).abs();
    let scale = positive.abs().max(neg_value.abs()).max(target.abs());
    let margin = 1e-9 * scale;
    if neg_err + margin < pos_err {
        Some(negative)
    } else {
        Some(principal)
    }
}

/// `(negate, period, extra)` describes the family `±principal + extra + period·k`.
///
/// Sine uses `(false, 2, 0)` and `(true, 2, 1)` (`asin/π + 2k` and `1 - asin/π + 2k`).
/// Cosine uses `(false, 2, 0)` and `(true, 2, 0)` (`2k ± acos/π`).
/// Tangent uses `(false, 1, 0)` (`atan/π + k`).
fn lift_periodic(
    principal: Expression,
    target: Option<f64>,
    families: &[(bool, i64, i64)],
    context: &EvalContext<'_>,
) -> Option<Expression> {
    let principal_value = eval_with_context(&principal, 0.0, context)?;
    if !principal_value.is_finite() {
        return None;
    }
    let Some(target) = target else {
        return Some(principal);
    };

    let scale = context.trig_argument_scale;
    let normalized_target = if scale == DEFAULT_TRIG_ARGUMENT_SCALE {
        target
    } else {
        (target * scale) / DEFAULT_TRIG_ARGUMENT_SCALE
    };

    let mut best: Option<(Expression, f64)> = None;
    for &(negate, period, extra) in families {
        if period == 0 {
            continue;
        }
        let signed = if negate {
            -principal_value
        } else {
            principal_value
        };
        let base = signed + extra as f64;
        let Some(k0) = nearest_branch_index((normalized_target - base) / period as f64) else {
            continue;
        };
        for delta in [-1.0_f64, 0.0, 1.0] {
            let k = k0 + delta;
            if !k.is_finite() {
                continue;
            }
            let Some(offset) = family_offset(period, extra, k) else {
                continue;
            };
            let Some(candidate) = linear_combo(&principal, principal_value, negate, offset) else {
                continue;
            };
            let Some(value) = eval_with_context(&candidate, 0.0, context) else {
                continue;
            };
            if !matches_hint(value, normalized_target) {
                continue;
            }
            let err = (value - normalized_target).abs();
            let replace = match &best {
                None => true,
                Some((prev, prev_err)) => {
                    err + 1e-9 < *prev_err
                        || ((err - prev_err).abs() <= 1e-9 && candidate.len() < prev.len())
                }
            };
            if replace {
                best = Some((candidate, err));
            }
        }
    }

    // No representable branch sits on the hint. Returning the principal value
    // here would display a different root (for example 0 instead of 2000002).
    let (normalized, _) = best?;
    if scale == DEFAULT_TRIG_ARGUMENT_SCALE {
        return Some(normalized);
    }
    let pi = constant_expression(Symbol::Pi);
    let angle = combine_binary_expressions(&normalized, &pi, Symbol::Mul);
    if scale == 1.0 {
        return Some(angle);
    }
    let magnitude = finite_positive_expression(scale.abs())?;
    let divisor = if scale.is_sign_negative() {
        append_unary_expression(&magnitude, Symbol::Neg)
    } else {
        magnitude
    };
    Some(combine_binary_expressions(&angle, &divisor, Symbol::Div))
}

fn nearest_branch_index(k: f64) -> Option<f64> {
    if !k.is_finite() {
        return None;
    }
    let rounded = k.round();
    rounded.is_finite().then_some(rounded)
}

/// `period·k + extra` when that sum is a finite integer-valued float.
fn family_offset(period: i64, extra: i64, k: f64) -> Option<f64> {
    let offset = (period as f64).mul_add(k, extra as f64);
    (offset.is_finite() && offset == offset.trunc()).then_some(offset)
}

/// `±principal + offset`. A zero principal is omitted so the shift prints as
/// the integer itself (`2000002`, not `atan(...)+2000002`).
fn linear_combo(
    principal: &Expression,
    principal_value: f64,
    negate: bool,
    offset: f64,
) -> Option<Expression> {
    if principal_value == 0.0 {
        return signed_integer(offset);
    }
    if !negate {
        return add_integer(principal, offset);
    }
    if offset == 0.0 {
        return Some(append_unary_expression(principal, Symbol::Neg));
    }
    if offset > 0.0 {
        let off = integer_expression(offset)?;
        return Some(combine_binary_expressions(&off, principal, Symbol::Sub));
    }
    // offset - principal with offset < 0 is -( |offset| + principal ).
    let magnitude = integer_expression(-offset)?;
    let sum = combine_binary_expressions(&magnitude, principal, Symbol::Add);
    Some(append_unary_expression(&sum, Symbol::Neg))
}

fn add_integer(base: &Expression, offset: f64) -> Option<Expression> {
    if offset == 0.0 {
        return Some(base.clone());
    }
    if offset > 0.0 {
        let off = integer_expression(offset)?;
        return Some(combine_binary_expressions(base, &off, Symbol::Add));
    }
    let magnitude = integer_expression(-offset)?;
    Some(combine_binary_expressions(base, &magnitude, Symbol::Sub))
}

fn signed_integer(offset: f64) -> Option<Expression> {
    if offset == 0.0 {
        return Some(zero_expression());
    }
    let magnitude = integer_expression(offset.abs())?;
    if offset > 0.0 {
        Some(magnitude)
    } else {
        Some(append_unary_expression(&magnitude, Symbol::Neg))
    }
}

/// Encode a custom trig scale as a decimal literal with an exact binary
/// postfix scaffold, so both display formats preserve its value.
fn finite_positive_expression(n: f64) -> Option<Expression> {
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    if n == n.trunc() {
        return integer_expression(n);
    }
    let bits = n.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let mut mantissa = bits & ((1u64 << 52) - 1);
    if biased != 0 {
        mantissa |= 1u64 << 52;
    }
    let trailing = mantissa.trailing_zeros();
    mantissa >>= trailing;
    let shift = (1075 - biased.max(1)) as u32 - trailing;
    let exponent = append_unary_expression(&horner_u64(u64::from(shift))?, Symbol::Neg);
    let power =
        combine_binary_expressions(&constant_expression(Symbol::Two), &exponent, Symbol::Pow);
    let mut expression = combine_binary_expressions(&horner_u64(mantissa)?, &power, Symbol::Mul);
    expression.mark_decimal_literal(n);
    Some(expression)
}

/// Positive finite integer. The printer shows its decimal digits.
///
/// The postfix scaffold is `mantissa * 2^shift` (or just the power of two).
/// Evaluation does not trust that scaffold: the span is marked with `n` and
/// [`crate::eval`] pushes that exact float. A base-10 Horner product is not
/// an exact f64 for every integer (2^58 through 2^61), and an `i64` branch
/// index cannot name 2^63 or anything larger.
fn integer_expression(n: f64) -> Option<Expression> {
    // Prefer the decimal Horner scaffold when it is an exact f64. That keeps
    // single-digit offsets (2, 3, …) one symbol long, so a tie between
    // `principal + 2` and `3 - principal` still prefers the form found first.
    // 2^58..2^61 are exact integers whose Horner products are not, and anything
    // at or above 2^63 is not an i64; those use `mantissa * 2^shift` plus the
    // recorded literal.
    if n > 0.0 && n < 9223372036854775808.0 && n == n.trunc() {
        let as_u64 = n as u64;
        if as_u64 as f64 == n {
            if let Some(mut expr) = horner_u64(as_u64) {
                expr.mark_decimal_literal(n);
                let got = eval_expr(&expr, 0.0)?;
                return (got == n).then_some(expr);
            }
        }
    }
    let (mantissa, shift) = split_positive_integer(n)?;
    let mut expr = if mantissa == 1 && shift > 0 {
        power_of_two(shift)?
    } else if shift == 0 {
        horner_u64(mantissa)?
    } else {
        let magnitude = horner_u64(mantissa)?;
        let power = power_of_two(shift)?;
        combine_binary_expressions(&magnitude, &power, Symbol::Mul)
    };
    expr.mark_decimal_literal(n);
    let got = eval_expr(&expr, 0.0)?;
    (got == n).then_some(expr)
}

/// `n = mantissa * 2^shift` with `mantissa` odd (or `shift == 0`) and `mantissa <= 2^53`.
fn split_positive_integer(n: f64) -> Option<(u64, u32)> {
    if !n.is_finite() || n <= 0.0 || n != n.trunc() {
        return None;
    }
    let bits = n.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    if biased == 0 {
        return None;
    }
    let mut mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let mut shift = biased - 1023 - 52;
    if shift < 0 {
        let down = (-shift) as u32;
        if mantissa & ((1u64 << down) - 1) != 0 {
            return None;
        }
        mantissa >>= down;
        shift = 0;
    }
    let trailing = mantissa.trailing_zeros();
    mantissa >>= trailing;
    shift += trailing as i32;
    if shift < 0 || mantissa == 0 {
        return None;
    }
    Some((mantissa, shift as u32))
}

fn power_of_two(shift: u32) -> Option<Expression> {
    if shift == 0 {
        return Some(constant_expression(Symbol::One));
    }
    let exponent = horner_u64(u64::from(shift))?;
    Some(combine_binary_expressions(
        &constant_expression(Symbol::Two),
        &exponent,
        Symbol::Pow,
    ))
}

/// Positive integer whose value is an exact f64, as a base-10 Horner scaffold.
///
/// Every integer up to 2^53 is an exact f64, and each Horner step stays inside
/// that range, so this scaffold evaluates to `n` on its own. Larger integers
/// go through [`integer_expression`], which records the exact value.
fn horner_u64(n: u64) -> Option<Expression> {
    if n == 0 || (n as f64) as u64 != n {
        return None;
    }
    let mut digits = Vec::new();
    let mut value = n;
    while value > 0 {
        digits.push((value % 10) as u32);
        value /= 10;
    }
    digits.reverse();
    let mut expr = constant_expression(digit_symbol(digits[0]));
    for &digit in &digits[1..] {
        let ten = combine_binary_expressions(
            &constant_expression(Symbol::Two),
            &constant_expression(Symbol::Five),
            Symbol::Mul,
        );
        expr = combine_binary_expressions(&expr, &ten, Symbol::Mul);
        if digit != 0 {
            expr = combine_binary_expressions(
                &expr,
                &constant_expression(digit_symbol(digit)),
                Symbol::Add,
            );
        }
    }
    let got = eval_expr(&expr, 0.0)?;
    (got == n as f64).then_some(expr)
}

fn digit_symbol(digit: u32) -> Symbol {
    match digit {
        1 => Symbol::One,
        2 => Symbol::Two,
        3 => Symbol::Three,
        4 => Symbol::Four,
        5 => Symbol::Five,
        6 => Symbol::Six,
        7 => Symbol::Seven,
        8 => Symbol::Eight,
        9 => Symbol::Nine,
        _ => unreachable!("decimal digit"),
    }
}

fn zero_expression() -> Expression {
    combine_binary_expressions(
        &constant_expression(Symbol::One),
        &constant_expression(Symbol::One),
        Symbol::Sub,
    )
}

fn finite_constant(expr: &Expression, context: &EvalContext<'_>) -> Option<f64> {
    let value = eval_with_context(expr, 0.0, context)?;
    value.is_finite().then_some(value)
}

/// Argument of `asin` / `acos`. Values a few ulps outside `[-1, 1]` clamp to
/// the endpoint so `sqrt(1-y^2)` stays real; anything further is not real.
fn unit_interval_argument(expr: &Expression, context: &EvalContext<'_>) -> Option<Expression> {
    let y = finite_constant(expr, context)?;
    const TOL: f64 = 1e-8;
    if !(-1.0 - TOL..=1.0 + TOL).contains(&y) {
        return None;
    }
    if y > 1.0 {
        return Some(constant_expression(Symbol::One));
    }
    if y < -1.0 {
        return Some(append_unary_expression(
            &constant_expression(Symbol::One),
            Symbol::Neg,
        ));
    }
    Some(expr.clone())
}

/// Radicand of an even root. A tiny negative from rounding becomes 0.
fn nonnegative_argument(expr: &Expression, context: &EvalContext<'_>) -> Option<Expression> {
    let y = finite_constant(expr, context)?;
    if y >= 0.0 {
        return Some(expr.clone());
    }
    if y >= -1e-12 {
        return Some(zero_expression());
    }
    None
}

fn is_even_integer_value(value: f64) -> bool {
    integer_parity(value).is_some_and(|even| even)
}

fn is_odd_integer_value(value: f64) -> bool {
    integer_parity(value).is_some_and(|even| !even)
}

/// `Some(true)` even, `Some(false)` odd, `None` when `value` is not an integer.
///
/// Exact integers of any finite magnitude use the significand. A value within
/// `1e-8` of an integer still counts, which is only possible below 2^52.
fn integer_parity(value: f64) -> Option<bool> {
    if let Some(even) = crate::eval::f64_exact_integer_is_even(value) {
        return Some(even);
    }
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if (value - rounded).abs() > 1e-8 {
        return None;
    }
    crate::eval::f64_exact_integer_is_even(rounded)
}

/// Closed form versus the refined root.
///
/// Relative slack covers a flat Newton step and a negative root near 0.
/// The absolute cap stays below 1, so a neighboring trig period cannot pass
/// once the root is large.
fn matches_hint(value: f64, hint: f64) -> bool {
    if !value.is_finite() || !hint.is_finite() {
        return false;
    }
    let diff = (value - hint).abs();
    let scale = value.abs().max(hint.abs());
    let tol = (1e-4 * scale).min(1e-3);
    diff <= tol
}

fn values_agree(left: f64, right: f64) -> bool {
    if !left.is_finite() || !right.is_finite() {
        return false;
    }
    let diff = (left - right).abs();
    let scale = left.abs().max(right.abs());
    diff <= 1e-6 * scale.max(1e-300)
}

fn canonical_node_key(node: &SolveNode) -> String {
    match &node.kind {
        SolveNodeKind::Atom => node.expr.to_postfix(),
        SolveNodeKind::Unary(op, child) => {
            format!("{}({})", symbol_key(*op), canonical_node_key(child))
        }
        SolveNodeKind::Binary(op, left, right) => {
            let mut lk = canonical_node_key(left);
            let mut rk = canonical_node_key(right);
            // Handle commutativity
            if matches!(op, Symbol::Add | Symbol::Mul) && lk > rk {
                std::mem::swap(&mut lk, &mut rk);
            }
            format!("({}{}{})", lk, symbol_key(*op), rk)
        }
    }
}

fn symbol_key(sym: Symbol) -> String {
    let byte = sym as u8;
    if byte.is_ascii_graphic() {
        (byte as char).to_string()
    } else {
        format!("#{}", byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create expression from postfix string
    fn expr(s: &str) -> Expression {
        Expression::parse(s).expect("valid expression")
    }

    // ==================== solve_for_x_rhs_expression tests ====================

    #[test]
    fn test_solve_simple_addition() {
        // x + 1 = 2  =>  x = 2 - 1
        let lhs = expr("x1+");
        let rhs = expr("2");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        // Should be: 2 1 - (postfix)
        assert_eq!(solved.to_postfix(), "21-");
    }

    #[test]
    fn test_solve_simple_subtraction() {
        // x - 1 = 2  =>  x = 2 + 1
        let lhs = expr("x1-");
        let rhs = expr("2");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "21+");
    }

    #[test]
    fn test_solve_simple_multiplication() {
        // 2 * x = 6  =>  x = 6 / 2
        let lhs = expr("2x*");
        let rhs = expr("6");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "62/");
    }

    #[test]
    fn test_solve_simple_division() {
        // x / 2 = 3  =>  x = 3 * 2
        let lhs = expr("x2/");
        let rhs = expr("3");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "32*");
    }

    #[test]
    fn test_solve_square() {
        // x^2 = 4  =>  x = sqrt(4)
        let lhs = expr("xs");
        let rhs = expr("4");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "4q");
    }

    #[test]
    fn test_solve_sqrt() {
        // sqrt(x) = 4  =>  x = 4^2
        let lhs = expr("xq");
        let rhs = expr("4");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "4s");
    }

    #[test]
    fn test_solve_negation() {
        // -x = 3  =>  x = -3
        let lhs = expr("xn");
        let rhs = expr("3");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "3n");
    }

    #[test]
    fn test_solve_reciprocal() {
        // 1/x = 2  =>  x = 1/2
        let lhs = expr("xr");
        let rhs = expr("2");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "2r");
    }

    #[test]
    fn test_solve_ln() {
        // ln(x) = 2  =>  x = e^2
        let lhs = expr("xl"); // x then ln
        let rhs = expr("2");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        // Should contain '2' and 'E' (exp operator)
        assert!(solved.to_postfix().contains('2'));
        assert!(solved.to_postfix().contains('E'));
    }

    #[test]
    fn test_solve_exp() {
        // e^x = 2  =>  x = ln(2)
        let lhs = expr("xE"); // x then Exp (e^x)
        let rhs = expr("2");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert!(solved.to_postfix().starts_with('2'));
        assert!(solved.to_postfix().contains('l')); // ln operator
    }

    #[test]
    fn test_solve_nested_expression() {
        // (x + 1) * 2 = 6  =>  x = (6 / 2) - 1
        let lhs = expr("x1+2*");
        let rhs = expr("6");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        // Should produce: 6 2 / 1 -
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "62/1-");
    }

    #[test]
    fn test_solve_x_on_right_side() {
        // 1 + x = 3  =>  x = 3 - 1
        let lhs = expr("1x+");
        let rhs = expr("3");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
        let solved = result.unwrap();
        assert_eq!(solved.to_postfix(), "31-");
    }

    #[test]
    fn test_solve_fails_multiple_x() {
        // x * x = 4 (x appears twice) - should fail
        let lhs = expr("xx*");
        let rhs = expr("4");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(
            result.is_none(),
            "Expected None for expression with multiple x"
        );
    }

    #[test]
    fn test_solve_fails_no_x() {
        // 2 + 3 = 5 (no x) - should fail
        let lhs = expr("23+");
        let rhs = expr("5");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_none(), "Expected None for expression with no x");
    }

    #[test]
    fn test_solve_trig_functions() {
        // sinpi(x) = 0.5  =>  x = asin(0.5) / pi
        let lhs = expr("xs");
        let rhs = expr("5"); // Using '5' as constant for simplicity
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        // Note: 's' is Square, not SinPi in the symbol encoding
        // This tests that square inversion works
        assert!(result.is_some());
    }

    #[test]
    fn test_solve_power() {
        // x^2 = 4  =>  x = sqrt(4)
        let lhs = expr("x2^");
        let rhs = expr("4");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
    }

    #[test]
    fn test_solve_right_operand_x() {
        // 2^x = 8  =>  x = log_2(8)
        let lhs = expr("2x^");
        let rhs = expr("8");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_some());
    }

    // ==================== canonical_expression_key tests ====================

    #[test]
    fn test_canonical_key_atom() {
        let expr1 = expr("x");
        let key = canonical_expression_key(&expr1);
        assert!(key.is_some());
        assert_eq!(key.unwrap(), "x");
    }

    #[test]
    fn test_canonical_key_commutativity_addition() {
        // x + 1 and 1 + x should produce the same canonical key
        let expr1 = expr("x1+");
        let expr2 = expr("1x+");
        let key1 = canonical_expression_key(&expr1);
        let key2 = canonical_expression_key(&expr2);
        assert_eq!(key1, key2, "x+1 and 1+x should have same canonical key");
    }

    #[test]
    fn test_canonical_key_commutativity_multiplication() {
        // x * 2 and 2 * x should produce the same canonical key
        let expr1 = expr("x2*");
        let expr2 = expr("2x*");
        let key1 = canonical_expression_key(&expr1);
        let key2 = canonical_expression_key(&expr2);
        assert_eq!(key1, key2, "x*2 and 2*x should have same canonical key");
    }

    #[test]
    fn test_canonical_key_non_commutative() {
        // x - 1 and 1 - x should NOT produce the same canonical key
        let expr1 = expr("x1-");
        let expr2 = expr("1x-");
        let key1 = canonical_expression_key(&expr1);
        let key2 = canonical_expression_key(&expr2);
        assert_ne!(
            key1, key2,
            "x-1 and 1-x should have different canonical keys"
        );
    }

    #[test]
    fn test_canonical_key_nested() {
        // (x + 1) * 2 and 2 * (1 + x) should have same key
        let expr1 = expr("x1+2*");
        let expr2 = expr("1x+2*");
        let key1 = canonical_expression_key(&expr1);
        let key2 = canonical_expression_key(&expr2);
        assert_eq!(key1, key2, "nested commutative expressions should match");
    }

    // ==================== unary_inverse_expression tests ====================

    #[test]
    fn test_unary_inverse_negation() {
        // -x = y => x = -y (double negation)
        let rhs = expr("3");
        let result = unary_inverse_expression(Symbol::Neg, &rhs, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "3n");
    }

    #[test]
    fn test_unary_inverse_reciprocal() {
        // 1/x = y => x = 1/y
        let rhs = expr("3");
        let result = unary_inverse_expression(Symbol::Recip, &rhs, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "3r");
    }

    #[test]
    fn test_unary_inverse_square_sqrt() {
        // x^2 = y => x = sqrt(y)
        let rhs = expr("4");
        let result = unary_inverse_expression(Symbol::Square, &rhs, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "4q");

        // sqrt(x) = y => x = y^2
        let result = unary_inverse_expression(Symbol::Sqrt, &rhs, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "4s");
    }

    #[test]
    fn test_unary_inverse_ln_exp() {
        // ln(x) = y => x = e^y
        let rhs = expr("2");
        let result = unary_inverse_expression(Symbol::Ln, &rhs, None, &EvalContext::new());
        assert!(result.is_some());

        // e^x = y => x = ln(y)
        let result = unary_inverse_expression(Symbol::Exp, &rhs, None, &EvalContext::new());
        assert!(result.is_some());
    }

    // ==================== binary inversion tests ====================

    #[test]
    fn test_binary_inverse_add_left() {
        // x + k = y => x = y - k
        let rhs = expr("5");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Add, &rhs, &known, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "52-");
    }

    #[test]
    fn test_binary_inverse_sub_left() {
        // x - k = y => x = y + k
        let rhs = expr("3");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Sub, &rhs, &known, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "32+");
    }

    #[test]
    fn test_binary_inverse_mul_left() {
        // x * k = y => x = y / k
        let rhs = expr("6");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Mul, &rhs, &known, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "62/");
    }

    #[test]
    fn test_binary_inverse_div_left() {
        // x / k = y => x = y * k
        let rhs = expr("3");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Div, &rhs, &known, None, &EvalContext::new());
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "32*");
    }

    #[test]
    fn test_binary_inverse_sub_right() {
        // k - x = y => x = k - y
        let known = expr("5");
        let rhs = expr("2");
        let result = invert_binary_right(Symbol::Sub, &known, &rhs);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "52-");
    }

    // ==================== edge case tests ====================

    #[test]
    fn test_solve_empty_expression() {
        let lhs = Expression::new();
        let rhs = expr("1");
        let result = solve_for_x_rhs_expression(&lhs, &rhs);
        assert!(result.is_none());
    }

    #[test]
    fn test_canonical_empty_expression() {
        let empty = Expression::new();
        let result = canonical_expression_key(&empty);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_solve_ast_malformed() {
        // Expression that would cause stack underflow: + without operands
        let malformed = Expression::from_symbols(&[Symbol::Add]);
        let result = build_solve_ast(&malformed);
        assert!(result.is_none(), "Malformed expression should return None");
    }

    #[test]
    fn test_build_solve_ast_incomplete() {
        // Expression with too many operands left on stack: 1 2 3
        let incomplete = Expression::from_symbols(&[Symbol::One, Symbol::Two, Symbol::Three]);
        let result = build_solve_ast(&incomplete);
        assert!(result.is_none(), "Incomplete expression should return None");
    }

    #[test]
    fn test_is_x_atom() {
        let x_expr = expr("x");
        assert!(is_x_atom(&x_expr));

        let not_x = expr("1");
        assert!(!is_x_atom(&not_x));

        let complex = expr("x1+");
        assert!(!is_x_atom(&complex));
    }

    fn solved_value(postfix_lhs: &str, postfix_rhs: &str, x_value: f64) -> f64 {
        let solved = solve_for_x_at(&expr(postfix_lhs), &expr(postfix_rhs), x_value)
            .unwrap_or_else(|| panic!("expected a closed form for {postfix_lhs} = {postfix_rhs}"));
        crate::eval::evaluate(&solved, 0.0)
            .unwrap_or_else(|err| panic!("solved expression failed to evaluate: {err}"))
            .value
    }

    #[test]
    fn test_sin_inverse_uses_branch_near_target() {
        // sin(πx) = 1. Principal asin(1)/π is 0.5; the root near √(2π) ≈ 2.5066 is 2.5.
        let value = solved_value("xS", "1", 2.5);
        assert!(
            (value - 2.5).abs() < 1e-9,
            "sin branch should be 2.5, got {value}"
        );
        let principal = solve_for_x_rhs_expression(&expr("xS"), &expr("1")).unwrap();
        let principal_value = crate::eval::evaluate(&principal, 0.0).unwrap().value;
        assert!(
            (principal_value - 0.5).abs() < 1e-9,
            "principal sine inverse stays 0.5"
        );
    }

    #[test]
    fn test_cos_inverse_uses_branch_near_target() {
        // cos(πx) = ln(1) = 0. Principal acos(0)/π is 0.5; 2 + acos(0)/π = 2.5.
        let value = solved_value("xC", "1l", 2.5);
        assert!(
            (value - 2.5).abs() < 1e-9,
            "cos branch should be 2.5, got {value}"
        );
    }

    #[test]
    fn test_tan_inverse_period_near_integer() {
        let zero = expr("11-");
        let solved = solve_for_x_at(&expr("xT"), &zero, 3.0).expect("tan inverse");
        let value = crate::eval::evaluate(&solved, 0.0).unwrap().value;
        assert!(
            (value - 3.0).abs() < 1e-9,
            "tan branch should be 3, got {value} from {}",
            solved.to_infix()
        );
    }

    #[test]
    fn test_square_inverse_uses_negative_root() {
        let value = solved_value("xs", "4", -2.0);
        assert!(
            (value + 2.0).abs() < 1e-9,
            "negative square root should be -2, got {value}"
        );
        let positive = solved_value("xs", "4", 2.0);
        assert!((positive - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_even_power_inverse_uses_negative_root() {
        // x^2 via pow, not the square operator.
        let value = solved_value("x2^", "4", -2.0);
        assert!(
            (value + 2.0).abs() < 1e-9,
            "negative even root should be -2, got {value}"
        );
    }

    #[test]
    fn test_integer_offset_above_nine() {
        // sin(πx) = 0 at x = 12. Principal is 0, so the expression must add 12.
        let zero = expr("11-");
        let solved = solve_for_x_at(&expr("xS"), &zero, 12.0).expect("sin inverse");
        let value = crate::eval::evaluate(&solved, 0.0).unwrap().value;
        assert!(
            (value - 12.0).abs() < 1e-8,
            "expected 12, got {value} from {}",
            solved.to_infix()
        );
        assert_eq!(solved.to_infix(), "12");
    }

    #[test]
    fn test_large_period_prints_decimal_integer() {
        // cos(πx) = 1 at an exact integer. The principal inverse is 0.
        let solved = solve_for_x_at(&expr("xC"), &expr("1"), 2_000_002.0).expect("cos branch");
        assert_eq!(solved.to_infix(), "2000002");
        assert!(!solved.to_infix().contains("2*5"));
        let solved = solve_for_x_at(&expr("xS"), &expr("11-"), -2_000_002.0).expect("sin branch");
        assert_eq!(solved.to_infix(), "-2000002");
        let solved = solve_for_x_at(&expr("xS"), &expr("11-"), 5_000_000.0).expect("sin branch");
        assert_eq!(solved.to_infix(), "5000000");
        let solved = solve_for_x_at(&expr("xT"), &expr("11-"), 1_000_001.0).expect("tan branch");
        assert_eq!(solved.to_infix(), "1000001");
        // 2^53 + 1 is not an f64. The float rounds, and a branch that is not
        // that float must not be invented. The next exact integer is 2^53 + 2.
        let past_mantissa = (1_i64 << 53) + 2;
        let solved = solve_for_x_at(&expr("xC"), &expr("1"), past_mantissa as f64);
        let value = crate::eval::evaluate(&solved.expect("exact even integer"), 0.0)
            .unwrap()
            .value;
        assert_eq!(value, past_mantissa as f64);
    }

    #[test]
    fn test_choose_signed_root_picks_the_nearer_sign() {
        // Direct call: the equation gate must not be what rejects a flipped odd root
        // or a tiny negative root. Reintroducing an absolute 1e-12 margin, or
        // applying this helper to an odd power, fails here.
        let principal = expr("4q");
        let chosen =
            choose_signed_root(principal, Some(-2.0), &EvalContext::new()).expect("even root");
        let value = eval_expr(&chosen, 0.0).unwrap();
        assert!(
            value < 0.0,
            "expected the negative square root, got {value}"
        );

        let tiny = expr("2125**25**n^q");
        let hint = -2.0_f64.powi(-50);
        let chosen = choose_signed_root(tiny, Some(hint), &EvalContext::new()).expect("tiny root");
        let value = eval_expr(&chosen, 0.0).unwrap();
        assert!(
            value < 0.0 && (value - hint).abs() <= hint.abs() * 1e-6,
            "relative sign choice failed: {value} vs {hint}"
        );

        let odd = invert_power(&expr("8"), &expr("3"), Some(-2.0), &EvalContext::new())
            .expect("odd root");
        let odd_value = eval_expr(&odd, 0.0).unwrap();
        assert!(
            odd_value > 0.0 && (odd_value - 2.0).abs() < 1e-9,
            "odd power grew a sign choice: {odd_value} from {}",
            odd.to_infix()
        );
        assert!(
            !odd.to_infix().starts_with('-'),
            "odd root expression was negated: {}",
            odd.to_infix()
        );
    }

    #[test]
    fn test_unit_interval_argument_rejects_asin_outside_domain() {
        // `solve_for_x_at` also returns None for these, but that is the later
        // equation gate. This asserts the domain check itself.
        assert!(
            unit_interval_argument(&expr("2"), &EvalContext::new()).is_none(),
            "asin/acos argument 2 is not real"
        );
        assert!(unit_interval_argument(&expr("1"), &EvalContext::new()).is_some());
        assert!(unit_interval_argument(&expr("1n"), &EvalContext::new()).is_some());
        assert!(unary_inverse_expression(
            Symbol::SinPi,
            &expr("2"),
            Some(0.5),
            &EvalContext::new()
        )
        .is_none());
        assert!(
            unary_inverse_expression(Symbol::CosPi, &expr("2"), None, &EvalContext::new())
                .is_none()
        );
    }

    #[test]
    fn test_exact_integer_roots_include_large_powers_of_two() {
        // 2^58..2^61 used to be dropped because the base-10 Horner product is
        // not an exact f64. 2^63 and above used to be dropped because the
        // branch index was an i64. +2^62 additionally made tanpi overflow.
        let exponents = [
            1, 10, 52, 53, 58, 59, 60, 61, 62, 63, 64, 100, 127, 200, 1023,
        ];
        for exp in exponents {
            let magnitude = 2.0_f64.powi(exp);
            for hint in [magnitude, -magnitude] {
                for (lhs, rhs) in [("xC", "1"), ("xS", "11-"), ("xT", "11-")] {
                    let solved =
                        solve_for_x_at(&expr(lhs), &expr(rhs), hint).unwrap_or_else(|| {
                            panic!("expected a closed form for {lhs} = {rhs} at 2^{exp} ({hint})")
                        });
                    let value = crate::eval::evaluate(&solved, 0.0)
                        .unwrap_or_else(|err| panic!("{lhs} at {hint} failed: {err}"))
                        .value;
                    assert_eq!(
                        value,
                        hint,
                        "{lhs} = {rhs} printed {} for {hint}",
                        solved.to_infix()
                    );
                    let digits = solved.to_infix();
                    let body = digits.strip_prefix('-').unwrap_or(digits.as_str());
                    assert!(
                        !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()),
                        "expected a decimal integer, got {digits}"
                    );
                }
            }
        }
        // Exact even integer that is not a pure power of two, past 2^63.
        let mixed = 2.0_f64.powi(70) + 2.0_f64.powi(20);
        let solved = solve_for_x_at(&expr("xC"), &expr("1"), mixed).expect("mixed integer");
        assert_eq!(
            crate::eval::evaluate(&solved, 0.0).unwrap().value,
            mixed,
            "{}",
            solved.to_infix()
        );
        let solved = solve_for_x_at(&expr("xT"), &expr("11-"), mixed).expect("tan at mixed");
        assert_eq!(crate::eval::evaluate(&solved, 0.0).unwrap().value, mixed);
        // 2^52 + 1 is an exact odd integer: sine and tangent are 0, cosine is -1.
        let odd = 2.0_f64.powi(52) + 1.0;
        assert!(solve_for_x_at(&expr("xC"), &expr("1"), odd).is_none());
        let solved = solve_for_x_at(&expr("xS"), &expr("11-"), odd).expect("sin at odd integer");
        assert_eq!(crate::eval::evaluate(&solved, 0.0).unwrap().value, odd);
        let solved = solve_for_x_at(&expr("xT"), &expr("11-"), odd).expect("tan at odd integer");
        assert_eq!(crate::eval::evaluate(&solved, 0.0).unwrap().value, odd);
        // Largest finite f64 is an even integer.
        let solved = solve_for_x_at(&expr("xC"), &expr("1"), f64::MAX).expect("cos at f64::MAX");
        assert_eq!(crate::eval::evaluate(&solved, 0.0).unwrap().value, f64::MAX);
        let solved = solve_for_x_at(&expr("xT"), &expr("11-"), -f64::MAX).expect("tan at -MAX");
        assert_eq!(
            crate::eval::evaluate(&solved, 0.0).unwrap().value,
            -f64::MAX
        );
    }

    #[test]
    fn test_tiny_even_root_uses_relative_sign() {
        // x^2 = 2^-100, root = -2^-50. The absolute 1e-12 cutoff used to keep +.
        let hint = -2.0_f64.powi(-50);
        let solved = solve_for_x_at(&expr("xs"), &expr("2125**25**n^"), hint).expect("tiny root");
        let value = crate::eval::evaluate(&solved, 0.0).unwrap().value;
        assert!(
            value < 0.0 && (value - hint).abs() <= 1e-8 * hint.abs(),
            "expected {hint}, got {value} from {}",
            solved.to_infix()
        );
    }

    #[test]
    fn test_odd_power_has_one_real_root() {
        // x^3 = 8. -2 is not a root, so the solve must not print it.
        assert!(solve_for_x_at(&expr("x3^"), &expr("8"), -2.0).is_none());
        let value = solved_value("x3^", "8", 2.0);
        assert!((value - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_out_of_domain_and_nonfinite_hint_have_no_closed_form() {
        assert!(solve_for_x_at(&expr("xS"), &expr("2"), 0.5).is_none());
        assert!(solve_for_x_at(&expr("xC"), &expr("2"), 0.0).is_none());
        assert!(solve_for_x_at(&expr("xS"), &expr("1"), f64::NAN).is_none());
        assert!(solve_for_x_at(&expr("xS"), &expr("1"), f64::INFINITY).is_none());
        // Principal API still refuses a non-real inverse.
        assert!(solve_for_x_rhs_expression(&expr("xS"), &expr("2")).is_none());
    }

    #[test]
    fn test_format_of_solved_root_follows_output_dialect() {
        let solved = solve_for_x_at(&expr("x2^"), &expr("4"), -2.0).expect("even root");
        assert!(
            solved.to_infix_sympy().contains("**"),
            "sympy: {}",
            solved.to_infix_sympy()
        );
        assert!(
            solved.to_infix_mathematica().contains("^("),
            "mathematica: {}",
            solved.to_infix_mathematica()
        );
        assert!(
            !solved.to_infix().contains("**"),
            "default: {}",
            solved.to_infix()
        );
    }

    #[test]
    fn solved_form_satisfies_equation_and_hint() {
        let pow_100 = "2125**25**n^"; // 2^-100
        let cases = [
            ("xC", "1", 2_000_002.0),
            ("xC", "1", -2_000_002.0),
            ("xS", "11-", 2_000_002.0),
            ("xS", "11-", -2_000_002.0),
            ("xS", "11-", 5_000_000.0),
            ("xT", "11-", 1_000_001.0),
            ("xS", "1", 2.5),
            ("xC", "1l", 2.5),
            ("xs", "4", -2.0),
            ("xs", "4", 2.0),
            ("x2^", "4", -2.0),
            ("x3^", "8", 2.0),
            ("xs", pow_100, -2.0_f64.powi(-50)),
            ("xs", pow_100, 2.0_f64.powi(-50)),
            ("x", "5", 5.0),
        ];
        for (lhs, rhs, hint) in cases {
            let lhs_expr = expr(lhs);
            let rhs_expr = expr(rhs);
            let solved = solve_for_x_at(&lhs_expr, &rhs_expr, hint)
                .unwrap_or_else(|| panic!("expected a closed form for {lhs} = {rhs} at {hint}"));
            let value = crate::eval::evaluate(&solved, 0.0)
                .unwrap_or_else(|err| panic!("{lhs} = {rhs} at {hint} did not evaluate: {err}"))
                .value;
            let scale = value.abs().max(hint.abs());
            let hint_tol = (1e-4 * scale).min(1e-3);
            assert!(
                (value - hint).abs() <= hint_tol,
                "{lhs} = {rhs}: {} = {value}, hint {hint}",
                solved.to_infix()
            );
            let left = crate::eval::evaluate(&lhs_expr, value)
                .unwrap_or_else(|err| panic!("lhs {lhs} at {value}: {err}"))
                .value;
            let right = crate::eval::evaluate(&rhs_expr, value)
                .unwrap_or_else(|err| panic!("rhs {rhs} at {value}: {err}"))
                .value;
            let eq_scale = left.abs().max(right.abs());
            assert!(
                (left - right).abs() <= 1e-6 * eq_scale.max(1e-300),
                "{lhs} = {rhs} at printed {} (x={value}): {left} vs {right}",
                solved.to_infix()
            );
        }
    }
}
