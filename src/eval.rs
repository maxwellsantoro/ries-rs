//! Expression evaluation with automatic differentiation
//!
//! Evaluates postfix expressions and computes derivatives using forward-mode AD.

use crate::expr::Expression;
use crate::symbol::{NumType, Seft, Symbol};

/// Result of evaluating an expression
#[derive(Debug, Clone, Copy)]
pub struct EvalResult {
    /// The computed value
    pub value: f64,
    /// Derivative with respect to x
    pub derivative: f64,
    /// Numeric type of the result
    pub num_type: NumType,
}

/// Evaluation error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalError {
    /// Stack underflow during evaluation
    StackUnderflow,
    /// Division by zero
    DivisionByZero,
    /// Logarithm of non-positive number
    LogDomain,
    /// Square root of negative number
    SqrtDomain,
    /// Overflow or NaN result
    Overflow,
    /// Invalid expression
    Invalid,
}

/// Mathematical constants
pub mod constants {
    pub const PI: f64 = std::f64::consts::PI;
    pub const E: f64 = std::f64::consts::E;
    pub const PHI: f64 = 1.6180339887498948482; // Golden ratio
}

/// Stack entry for evaluation with derivative tracking
#[derive(Debug, Clone, Copy)]
struct StackEntry {
    val: f64,
    deriv: f64,
    num_type: NumType,
}

impl StackEntry {
    fn new(val: f64, deriv: f64, num_type: NumType) -> Self {
        Self { val, deriv, num_type }
    }

    fn constant(val: f64, num_type: NumType) -> Self {
        Self { val, deriv: 0.0, num_type }
    }
}

/// Evaluate an expression at a given value of x
pub fn evaluate(expr: &Expression, x: f64) -> Result<EvalResult, EvalError> {
    let mut stack: Vec<StackEntry> = Vec::with_capacity(16);

    for &sym in expr.symbols() {
        match sym.seft() {
            Seft::A => {
                let entry = eval_constant(sym, x);
                stack.push(entry);
            }
            Seft::B => {
                let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                let result = eval_unary(sym, a)?;
                stack.push(result);
            }
            Seft::C => {
                let b = stack.pop().ok_or(EvalError::StackUnderflow)?;
                let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                let result = eval_binary(sym, a, b)?;
                stack.push(result);
            }
        }
    }

    if stack.len() != 1 {
        return Err(EvalError::Invalid);
    }

    let result = stack.pop().unwrap();

    // Check for invalid results
    if result.val.is_nan() || result.val.is_infinite() {
        return Err(EvalError::Overflow);
    }

    Ok(EvalResult {
        value: result.val,
        derivative: result.deriv,
        num_type: result.num_type,
    })
}

/// Evaluate a constant or variable symbol
fn eval_constant(sym: Symbol, x: f64) -> StackEntry {
    use Symbol::*;
    match sym {
        One => StackEntry::constant(1.0, NumType::Integer),
        Two => StackEntry::constant(2.0, NumType::Integer),
        Three => StackEntry::constant(3.0, NumType::Integer),
        Four => StackEntry::constant(4.0, NumType::Integer),
        Five => StackEntry::constant(5.0, NumType::Integer),
        Six => StackEntry::constant(6.0, NumType::Integer),
        Seven => StackEntry::constant(7.0, NumType::Integer),
        Eight => StackEntry::constant(8.0, NumType::Integer),
        Nine => StackEntry::constant(9.0, NumType::Integer),
        Pi => StackEntry::constant(constants::PI, NumType::Transcendental),
        E => StackEntry::constant(constants::E, NumType::Transcendental),
        Phi => StackEntry::constant(constants::PHI, NumType::Algebraic),
        X => StackEntry::new(x, 1.0, NumType::Transcendental), // dx/dx = 1
        _ => unreachable!("Not a constant: {:?}", sym),
    }
}

/// Evaluate a unary operator with derivative
fn eval_unary(sym: Symbol, a: StackEntry) -> Result<StackEntry, EvalError> {
    use Symbol::*;

    let (val, deriv, num_type) = match sym {
        // Negation: -a, d(-a)/dx = -da/dx
        Neg => (-a.val, -a.deriv, a.num_type),

        // Reciprocal: 1/a, d(1/a)/dx = -da/dx / a²
        Recip => {
            if a.val.abs() < 1e-300 {
                return Err(EvalError::DivisionByZero);
            }
            let val = 1.0 / a.val;
            let deriv = -a.deriv / (a.val * a.val);
            let num_type = if a.num_type == NumType::Integer {
                NumType::Rational
            } else {
                a.num_type
            };
            (val, deriv, num_type)
        }

        // Square root: sqrt(a), d(sqrt(a))/dx = da/dx / (2*sqrt(a))
        Sqrt => {
            if a.val < 0.0 {
                return Err(EvalError::SqrtDomain);
            }
            let val = a.val.sqrt();
            let deriv = if val.abs() > 1e-300 {
                a.deriv / (2.0 * val)
            } else {
                0.0
            };
            let num_type = if a.num_type >= NumType::Constructible {
                NumType::Constructible
            } else {
                a.num_type
            };
            (val, deriv, num_type)
        }

        // Square: a², d(a²)/dx = 2*a*da/dx
        Square => {
            let val = a.val * a.val;
            let deriv = 2.0 * a.val * a.deriv;
            (val, deriv, a.num_type)
        }

        // Natural log: ln(a), d(ln(a))/dx = da/dx / a
        Ln => {
            if a.val <= 0.0 {
                return Err(EvalError::LogDomain);
            }
            let val = a.val.ln();
            let deriv = a.deriv / a.val;
            (val, deriv, NumType::Transcendental)
        }

        // Exponential: e^a, d(e^a)/dx = e^a * da/dx
        Exp => {
            let val = a.val.exp();
            if val.is_infinite() {
                return Err(EvalError::Overflow);
            }
            let deriv = val * a.deriv;
            (val, deriv, NumType::Transcendental)
        }

        // sin(π*a), d(sin(πa))/dx = π*cos(πa)*da/dx
        SinPi => {
            let val = (constants::PI * a.val).sin();
            let deriv = constants::PI * (constants::PI * a.val).cos() * a.deriv;
            (val, deriv, NumType::Transcendental)
        }

        // cos(π*a), d(cos(πa))/dx = -π*sin(πa)*da/dx
        CosPi => {
            let val = (constants::PI * a.val).cos();
            let deriv = -constants::PI * (constants::PI * a.val).sin() * a.deriv;
            (val, deriv, NumType::Transcendental)
        }

        // tan(π*a), d(tan(πa))/dx = π*sec²(πa)*da/dx
        TanPi => {
            let cos_val = (constants::PI * a.val).cos();
            if cos_val.abs() < 1e-10 {
                return Err(EvalError::Overflow);
            }
            let val = (constants::PI * a.val).tan();
            let deriv = constants::PI * a.deriv / (cos_val * cos_val);
            (val, deriv, NumType::Transcendental)
        }

        // Lambert W function (principal branch)
        LambertW => {
            let val = lambert_w(a.val)?;
            // d(W(a))/dx = W(a) / (a * (1 + W(a))) * da/dx
            let denom = a.val * (1.0 + val);
            let deriv = if denom.abs() > 1e-300 {
                val / denom * a.deriv
            } else {
                0.0
            };
            (val, deriv, NumType::Transcendental)
        }

        _ => unreachable!("Not a unary operator: {:?}", sym),
    };

    Ok(StackEntry::new(val, deriv, num_type))
}

/// Evaluate a binary operator with derivative
fn eval_binary(sym: Symbol, a: StackEntry, b: StackEntry) -> Result<StackEntry, EvalError> {
    use Symbol::*;

    let (val, deriv, num_type) = match sym {
        // Addition: a + b
        Add => {
            let val = a.val + b.val;
            let deriv = a.deriv + b.deriv;
            let num_type = a.num_type.combine(b.num_type);
            (val, deriv, num_type)
        }

        // Subtraction: a - b
        Sub => {
            let val = a.val - b.val;
            let deriv = a.deriv - b.deriv;
            let num_type = a.num_type.combine(b.num_type);
            (val, deriv, num_type)
        }

        // Multiplication: a * b, d(ab)/dx = a*db/dx + b*da/dx
        Mul => {
            let val = a.val * b.val;
            let deriv = a.val * b.deriv + b.val * a.deriv;
            let num_type = a.num_type.combine(b.num_type);
            (val, deriv, num_type)
        }

        // Division: a / b, d(a/b)/dx = (b*da/dx - a*db/dx) / b²
        Div => {
            if b.val.abs() < 1e-300 {
                return Err(EvalError::DivisionByZero);
            }
            let val = a.val / b.val;
            let deriv = (b.val * a.deriv - a.val * b.deriv) / (b.val * b.val);
            let mut num_type = a.num_type.combine(b.num_type);
            if num_type == NumType::Integer {
                num_type = NumType::Rational;
            }
            (val, deriv, num_type)
        }

        // Power: a^b, d(a^b)/dx = a^b * (b*da/dx/a + ln(a)*db/dx)
        Pow => {
            if a.val <= 0.0 && b.val.fract() != 0.0 {
                return Err(EvalError::SqrtDomain);
            }
            let val = a.val.powf(b.val);
            if val.is_infinite() || val.is_nan() {
                return Err(EvalError::Overflow);
            }
            let deriv = if a.val > 0.0 {
                val * (b.val * a.deriv / a.val + a.val.ln() * b.deriv)
            } else if a.val == 0.0 && b.val > 0.0 {
                0.0
            } else {
                // Integer power of negative number
                val * b.val * a.deriv / a.val
            };
            let num_type = if b.num_type == NumType::Integer {
                a.num_type
            } else {
                NumType::Transcendental
            };
            (val, deriv, num_type)
        }

        // a-th root of b: b^(1/a)
        Root => {
            if a.val.abs() < 1e-300 {
                return Err(EvalError::DivisionByZero);
            }
            let exp = 1.0 / a.val;
            if b.val < 0.0 && (a.val.round() as i64) % 2 == 0 {
                return Err(EvalError::SqrtDomain);
            }
            let val = if b.val < 0.0 {
                // Odd root of negative number
                -((-b.val).powf(exp))
            } else {
                b.val.powf(exp)
            };
            if val.is_infinite() || val.is_nan() {
                return Err(EvalError::Overflow);
            }
            // d(b^(1/a))/dx = b^(1/a) * (db/dx/(a*b) - ln(b)*da/dx/a²)
            let deriv = if b.val.abs() > 1e-300 {
                val * (b.deriv / (a.val * b.val) - b.val.abs().ln() * a.deriv / (a.val * a.val))
            } else {
                0.0
            };
            (val, deriv, NumType::Algebraic)
        }

        // Logarithm base a of b: ln(b) / ln(a)
        Log => {
            if a.val <= 0.0 || a.val == 1.0 || b.val <= 0.0 {
                return Err(EvalError::LogDomain);
            }
            let ln_a = a.val.ln();
            let ln_b = b.val.ln();
            let val = ln_b / ln_a;
            // d(log_a(b))/dx = (db/dx/(b*ln(a)) - ln(b)*da/dx/(a*ln(a)²))
            let deriv = b.deriv / (b.val * ln_a) - ln_b * a.deriv / (a.val * ln_a * ln_a);
            (val, deriv, NumType::Transcendental)
        }

        // atan2(a, b) = angle of point (b, a) from origin
        Atan2 => {
            let val = a.val.atan2(b.val);
            // d(atan2(a,b))/dx = (b*da/dx - a*db/dx) / (a² + b²)
            let denom = a.val * a.val + b.val * b.val;
            let deriv = if denom.abs() > 1e-300 {
                (b.val * a.deriv - a.val * b.deriv) / denom
            } else {
                0.0
            };
            (val, deriv, NumType::Transcendental)
        }

        _ => unreachable!("Not a binary operator: {:?}", sym),
    };

    Ok(StackEntry::new(val, deriv, num_type))
}

/// Compute the Lambert W function (principal branch) using Halley's method
fn lambert_w(x: f64) -> Result<f64, EvalError> {
    if x < -1.0 / constants::E {
        return Err(EvalError::LogDomain);
    }

    // Initial guess - different approximations for different regimes
    let mut w = if x < -0.3 {
        // Near the branch point, use a series approximation
        let p = (1.0 + constants::E * x).sqrt();
        -1.0 + p - p * p / 3.0
    } else if x < 0.5 {
        // Near 0, W(x) ≈ x - x² + 1.5x³
        x * (1.0 - x * (1.0 - 1.5 * x))
    } else if x < 3.0 {
        // For moderate x, use a simple initial estimate
        0.5 + 0.5 * x.ln().max(0.0)
    } else {
        // For large x, W(x) ≈ ln(x) - ln(ln(x))
        let l1 = x.ln();
        let l2 = l1.ln();
        l1 - l2 + l2 / l1
    };

    // Halley's method
    for _ in 0..50 {
        let ew = w.exp();
        let wew = w * ew;
        let diff = wew - x;

        if diff.abs() < 1e-15 * (1.0 + x.abs()) {
            break;
        }

        let w1 = w + 1.0;
        let denom = ew * w1 - (w + 2.0) * diff / (2.0 * w1);
        if denom.abs() < 1e-300 {
            break;
        }
        w -= diff / denom;
    }

    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-10
    }

    #[test]
    fn test_basic_eval() {
        let expr = Expression::parse("32+").unwrap();
        let result = evaluate(&expr, 0.0).unwrap();
        assert!(approx_eq(result.value, 5.0));
        assert!(approx_eq(result.derivative, 0.0));
    }

    #[test]
    fn test_variable() {
        let expr = Expression::parse("x").unwrap();
        let result = evaluate(&expr, 3.5).unwrap();
        assert!(approx_eq(result.value, 3.5));
        assert!(approx_eq(result.derivative, 1.0));
    }

    #[test]
    fn test_x_squared() {
        let expr = Expression::parse("xs").unwrap(); // x^2
        let result = evaluate(&expr, 3.0).unwrap();
        assert!(approx_eq(result.value, 9.0));
        assert!(approx_eq(result.derivative, 6.0)); // 2x
    }

    #[test]
    fn test_sqrt_pi() {
        let expr = Expression::parse("pq").unwrap(); // sqrt(pi)
        let result = evaluate(&expr, 0.0).unwrap();
        assert!(approx_eq(result.value, constants::PI.sqrt()));
    }

    #[test]
    fn test_e_to_x() {
        let expr = Expression::parse("xE").unwrap(); // e^x
        let result = evaluate(&expr, 1.0).unwrap();
        assert!(approx_eq(result.value, constants::E));
        assert!(approx_eq(result.derivative, constants::E)); // d(e^x)/dx = e^x
    }

    #[test]
    fn test_complex_expr() {
        // x^2 + 2*x + 1 = (x+1)^2
        let expr = Expression::parse("xs2x*+1+").unwrap();
        let result = evaluate(&expr, 3.0).unwrap();
        assert!(approx_eq(result.value, 16.0)); // (3+1)^2
        assert!(approx_eq(result.derivative, 8.0)); // 2x + 2 = 8
    }

    #[test]
    fn test_lambert_w() {
        // W(1) ≈ 0.5671432904
        let w = lambert_w(1.0).unwrap();
        assert!((w - 0.5671432904).abs() < 1e-9);

        // W(e) = 1
        let w = lambert_w(constants::E).unwrap();
        assert!((w - 1.0).abs() < 1e-10);
    }
}
