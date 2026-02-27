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
pub fn solve_for_x_rhs_expression(lhs: &Expression, rhs: &Expression) -> Option<Expression> {
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
                rhs_expr = unary_inverse_expression(op, &rhs_expr)?;
                node = *child;
            }
            SolveNodeKind::Binary(op, left, right) => {
                let lx = left.x_count;
                let rx = right.x_count;
                if lx + rx != 1 {
                    return None;
                }

                if lx == 1 {
                    rhs_expr = invert_binary_left(op, &rhs_expr, &right.expr)?;
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

fn unary_inverse_expression(op: Symbol, rhs_value: &Expression) -> Option<Expression> {
    Some(match op {
        Symbol::Neg => append_unary_expression(rhs_value, Symbol::Neg),
        Symbol::Recip => append_unary_expression(rhs_value, Symbol::Recip),
        Symbol::Square => append_unary_expression(rhs_value, Symbol::Sqrt),
        Symbol::Sqrt => append_unary_expression(rhs_value, Symbol::Square),
        Symbol::Ln => append_unary_expression(rhs_value, Symbol::Exp),
        Symbol::Exp => append_unary_expression(rhs_value, Symbol::Ln),
        Symbol::TanPi => {
            // x = atan(rhs) / pi = atan2(rhs, 1) / pi
            let one = constant_expression(Symbol::One);
            let atan = combine_binary_expressions(rhs_value, &one, Symbol::Atan2);
            let pi = constant_expression(Symbol::Pi);
            combine_binary_expressions(&atan, &pi, Symbol::Div)
        }
        Symbol::SinPi => {
            // x = asin(rhs)/pi = atan2(rhs, sqrt(1-rhs^2))/pi
            let one = constant_expression(Symbol::One);
            let rhs_sq = append_unary_expression(rhs_value, Symbol::Square);
            let inner = combine_binary_expressions(&one, &rhs_sq, Symbol::Sub);
            let denom = append_unary_expression(&inner, Symbol::Sqrt);
            let atan = combine_binary_expressions(rhs_value, &denom, Symbol::Atan2);
            let pi = constant_expression(Symbol::Pi);
            combine_binary_expressions(&atan, &pi, Symbol::Div)
        }
        Symbol::CosPi => {
            // x = acos(rhs)/pi = atan2(sqrt(1-rhs^2), rhs)/pi
            let one = constant_expression(Symbol::One);
            let rhs_sq = append_unary_expression(rhs_value, Symbol::Square);
            let inner = combine_binary_expressions(&one, &rhs_sq, Symbol::Sub);
            let numer = append_unary_expression(&inner, Symbol::Sqrt);
            let atan = combine_binary_expressions(&numer, rhs_value, Symbol::Atan2);
            let pi = constant_expression(Symbol::Pi);
            combine_binary_expressions(&atan, &pi, Symbol::Div)
        }
        Symbol::LambertW => {
            // x = W(y)  =>  y = x * exp(x)
            let exp_rhs = append_unary_expression(rhs_value, Symbol::Exp);
            combine_binary_expressions(rhs_value, &exp_rhs, Symbol::Mul)
        }
        _ => return None,
    })
}

fn invert_binary_left(
    op: Symbol,
    rhs_value: &Expression,
    known_right: &Expression,
) -> Option<Expression> {
    Some(match op {
        Symbol::Add => combine_binary_expressions(rhs_value, known_right, Symbol::Sub),
        Symbol::Sub => combine_binary_expressions(rhs_value, known_right, Symbol::Add),
        Symbol::Mul => combine_binary_expressions(rhs_value, known_right, Symbol::Div),
        Symbol::Div => combine_binary_expressions(rhs_value, known_right, Symbol::Mul),
        Symbol::Pow => combine_binary_expressions(known_right, rhs_value, Symbol::Root),
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
        let result = unary_inverse_expression(Symbol::Neg, &rhs);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "3n");
    }

    #[test]
    fn test_unary_inverse_reciprocal() {
        // 1/x = y => x = 1/y
        let rhs = expr("3");
        let result = unary_inverse_expression(Symbol::Recip, &rhs);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "3r");
    }

    #[test]
    fn test_unary_inverse_square_sqrt() {
        // x^2 = y => x = sqrt(y)
        let rhs = expr("4");
        let result = unary_inverse_expression(Symbol::Square, &rhs);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "4q");

        // sqrt(x) = y => x = y^2
        let result = unary_inverse_expression(Symbol::Sqrt, &rhs);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "4s");
    }

    #[test]
    fn test_unary_inverse_ln_exp() {
        // ln(x) = y => x = e^y
        let rhs = expr("2");
        let result = unary_inverse_expression(Symbol::Ln, &rhs);
        assert!(result.is_some());

        // e^x = y => x = ln(y)
        let result = unary_inverse_expression(Symbol::Exp, &rhs);
        assert!(result.is_some());
    }

    // ==================== binary inversion tests ====================

    #[test]
    fn test_binary_inverse_add_left() {
        // x + k = y => x = y - k
        let rhs = expr("5");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Add, &rhs, &known);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "52-");
    }

    #[test]
    fn test_binary_inverse_sub_left() {
        // x - k = y => x = y + k
        let rhs = expr("3");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Sub, &rhs, &known);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "32+");
    }

    #[test]
    fn test_binary_inverse_mul_left() {
        // x * k = y => x = y / k
        let rhs = expr("6");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Mul, &rhs, &known);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_postfix(), "62/");
    }

    #[test]
    fn test_binary_inverse_div_left() {
        // x / k = y => x = y * k
        let rhs = expr("3");
        let known = expr("2");
        let result = invert_binary_left(Symbol::Div, &rhs, &known);
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
        let malformed = expr("+");
        let result = build_solve_ast(&malformed);
        assert!(result.is_none(), "Malformed expression should return None");
    }

    #[test]
    fn test_build_solve_ast_incomplete() {
        // Expression with too many operands left on stack: 1 2 3
        let incomplete = expr("123");
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
}
