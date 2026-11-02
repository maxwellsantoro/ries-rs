//! Analytical equation solver and expression canonicalization
//!
//! This module provides tools to solve symbolic equations for x and
//! to reduce expressions to a canonical form for deduplication.

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
    solve_for_x_inner(lhs, rhs, None)
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
/// `x_value` is the numeric root of the original equation. Non-finite values
/// fall back to the principal branch.
pub fn solve_for_x_at(lhs: &Expression, rhs: &Expression, x_value: f64) -> Option<Expression> {
    let hint = x_value.is_finite().then_some(x_value);
    solve_for_x_inner(lhs, rhs, hint)
}

fn solve_for_x_inner(
    lhs: &Expression,
    rhs: &Expression,
    x_value: Option<f64>,
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
                let target = x_value.and_then(|x| eval_expr(&child.expr, x));
                rhs_expr = unary_inverse_expression(op, &rhs_expr, target)?;
                node = *child;
            }
            SolveNodeKind::Binary(op, left, right) => {
                let lx = left.x_count;
                let rx = right.x_count;
                if lx + rx != 1 {
                    return None;
                }

                if lx == 1 {
                    let target = x_value.and_then(|x| eval_expr(&left.expr, x));
                    rhs_expr = invert_binary_left(op, &rhs_expr, &right.expr, target)?;
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
) -> Option<Expression> {
    Some(match op {
        Symbol::Neg => append_unary_expression(rhs_value, Symbol::Neg),
        Symbol::Recip => append_unary_expression(rhs_value, Symbol::Recip),
        Symbol::Square => {
            // x² = y ⟹ x = ±√y. The sign is the one nearest `target`.
            let principal = append_unary_expression(rhs_value, Symbol::Sqrt);
            choose_signed_root(principal, target)
        }
        Symbol::Sqrt => append_unary_expression(rhs_value, Symbol::Square), // √x = y ⟹ x = y²
        Symbol::Ln => append_unary_expression(rhs_value, Symbol::Exp),
        Symbol::Exp => append_unary_expression(rhs_value, Symbol::Ln),
        Symbol::TanPi => {
            // tan(πx) = y ⟹ x = atan(y)/π + k
            let principal = atan_over_pi(rhs_value);
            lift_periodic(principal, target, &[(false, 1, 0)])
        }
        Symbol::SinPi => {
            // sin(πx) = y ⟹ x = asin(y)/π + 2k  or  x = 1 - asin(y)/π + 2k
            let principal = asin_over_pi(rhs_value);
            lift_periodic(principal, target, &[(false, 2, 0), (true, 2, 1)])
        }
        Symbol::CosPi => {
            // cos(πx) = y ⟹ x = 2k ± acos(y)/π
            let principal = acos_over_pi(rhs_value);
            lift_periodic(principal, target, &[(false, 2, 0), (true, 2, 0)])
        }
        Symbol::LambertW => {
            // W(x) = y ⟹ x = y·eʸ. The inverse of W is single-valued on the reals.
            let exp_rhs = append_unary_expression(rhs_value, Symbol::Exp);
            combine_binary_expressions(rhs_value, &exp_rhs, Symbol::Mul)
        }
        _ => return None,
    })
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
) -> Option<Expression> {
    Some(match op {
        Symbol::Add => combine_binary_expressions(rhs_value, known_right, Symbol::Sub),
        Symbol::Sub => combine_binary_expressions(rhs_value, known_right, Symbol::Add),
        Symbol::Mul => combine_binary_expressions(rhs_value, known_right, Symbol::Div),
        Symbol::Div => combine_binary_expressions(rhs_value, known_right, Symbol::Mul),
        // x^k = y ⟹ x = ± y^(1/k). Even integer powers have two real roots.
        Symbol::Pow => {
            let root = combine_binary_expressions(known_right, rhs_value, Symbol::Root);
            choose_signed_root(root, target)
        }
        Symbol::Root => combine_binary_expressions(rhs_value, known_right, Symbol::Log),
        Symbol::Log => combine_binary_expressions(rhs_value, known_right, Symbol::Root),
        _ => return None,
    })
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
    let mut out = Expression::new();
    for &sym in lhs.symbols() {
        out.push(sym);
    }
    for &sym in rhs.symbols() {
        out.push(sym);
    }
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
    crate::eval::evaluate(expr, x)
        .ok()
        .map(|result| result.value)
}

/// Keep `principal` (a non-negative root) or its negation, whichever is nearer `target`.
fn choose_signed_root(principal: Expression, target: Option<f64>) -> Expression {
    let Some(target) = target else {
        return principal;
    };
    let Some(positive) = eval_expr(&principal, 0.0) else {
        return principal;
    };
    let negative = append_unary_expression(&principal, Symbol::Neg);
    let Some(neg_value) = eval_expr(&negative, 0.0) else {
        return principal;
    };
    // Require a strict improvement so a root at 0 stays on the principal branch.
    if (neg_value - target).abs() + 1e-12 < (positive - target).abs() {
        negative
    } else {
        principal
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
) -> Expression {
    let Some(target) = target else {
        return principal;
    };
    let Some(principal_value) = eval_expr(&principal, 0.0) else {
        return principal;
    };
    if !principal_value.is_finite() {
        return principal;
    }

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
        let Some(k0) = nearest_branch_index((target - base) / period as f64) else {
            continue;
        };
        for k in (k0 - 1)..=(k0 + 1) {
            let Some(offset) = family_offset(period, extra, k) else {
                continue;
            };
            let candidate = linear_combo(&principal, negate, offset);
            let Some(value) = eval_expr(&candidate, 0.0) else {
                continue;
            };
            if !value.is_finite() {
                continue;
            }
            let err = (value - target).abs();
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

    best.map(|(expr, _)| expr).unwrap_or(principal)
}

fn nearest_branch_index(k: f64) -> Option<i64> {
    if !k.is_finite() || k.abs() > 1_000_000.0 {
        return None;
    }
    Some(k.round() as i64)
}

fn family_offset(period: i64, extra: i64, k: i64) -> Option<i64> {
    period.checked_mul(k)?.checked_add(extra)
}

/// `±principal + offset`, written as a sum or difference so the offset is an integer literal.
fn linear_combo(principal: &Expression, negate: bool, offset: i64) -> Expression {
    if !negate {
        return add_integer(principal, offset);
    }
    if offset == 0 {
        return append_unary_expression(principal, Symbol::Neg);
    }
    if offset > 0 {
        let off = integer_expression(offset as u64);
        return combine_binary_expressions(&off, principal, Symbol::Sub);
    }
    // offset - principal with offset < 0 is -( |offset| + principal ).
    let magnitude = integer_expression(offset.unsigned_abs());
    let sum = combine_binary_expressions(&magnitude, principal, Symbol::Add);
    append_unary_expression(&sum, Symbol::Neg)
}

fn add_integer(base: &Expression, offset: i64) -> Expression {
    if offset == 0 {
        return base.clone();
    }
    if offset > 0 {
        let off = integer_expression(offset as u64);
        return combine_binary_expressions(base, &off, Symbol::Add);
    }
    let magnitude = integer_expression(offset.unsigned_abs());
    combine_binary_expressions(base, &magnitude, Symbol::Sub)
}

/// Positive integer as a postfix expression. Digits above 9 use Horner's method with 10 = 2·5.
fn integer_expression(n: u64) -> Expression {
    debug_assert!(n > 0);
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
    expr
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
        let result = unary_inverse_expression(Symbol::Neg, &rhs, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "3n");
    }

    #[test]
    fn test_unary_inverse_reciprocal() {
        // 1/x = y => x = 1/y
        let rhs = expr("3");
        let result = unary_inverse_expression(Symbol::Recip, &rhs, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "3r");
    }

    #[test]
    fn test_unary_inverse_square_sqrt() {
        // x^2 = y => x = sqrt(y)
        let rhs = expr("4");
        let result = unary_inverse_expression(Symbol::Square, &rhs, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "4q");

        // sqrt(x) = y => x = y^2
        let result = unary_inverse_expression(Symbol::Sqrt, &rhs, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "4s");
    }

    #[test]
    fn test_unary_inverse_ln_exp() {
        // ln(x) = y => x = e^y
        let rhs = expr("2");
        let result = unary_inverse_expression(Symbol::Ln, &rhs, None);
        assert!(result.is_some());

        // e^x = y => x = ln(y)
        let result = unary_inverse_expression(Symbol::Exp, &rhs, None);
        assert!(result.is_some());
    }

    // ==================== binary inversion tests ====================

    #[test]
    fn test_binary_inverse_add_left() {
        // x + k = y => x = y - k
        let rhs = expr("5");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Add, &rhs, &known, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "52-");
    }

    #[test]
    fn test_binary_inverse_sub_left() {
        // x - k = y => x = y + k
        let rhs = expr("3");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Sub, &rhs, &known, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "32+");
    }

    #[test]
    fn test_binary_inverse_mul_left() {
        // x * k = y => x = y / k
        let rhs = expr("6");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Mul, &rhs, &known, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "62/");
    }

    #[test]
    fn test_binary_inverse_div_left() {
        // x / k = y => x = y * k
        let rhs = expr("3");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Div, &rhs, &known, None);
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
        let value = solved_value("xS", "1", 2.506_628_274_631);
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
        let value = solved_value("xC", "1l", 2.506_628_274_631);
        assert!(
            (value - 2.5).abs() < 1e-9,
            "cos branch should be 2.5, got {value}"
        );
    }

    #[test]
    fn test_tan_inverse_period_near_integer() {
        let zero = expr("11-");
        let solved = solve_for_x_at(&expr("xT"), &zero, 3.02).expect("tan inverse");
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
        let solved = solve_for_x_at(&expr("xS"), &zero, 12.01).expect("sin inverse");
        let value = crate::eval::evaluate(&solved, 0.0).unwrap().value;
        assert!(
            (value - 12.0).abs() < 1e-8,
            "expected 12, got {value} from {}",
            solved.to_infix()
        );
    }
}
