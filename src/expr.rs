//! Expression representation and manipulation
//!
//! Expressions are stored in postfix (reverse Polish) notation.

use crate::symbol::{NumType, Seft, Symbol};
use smallvec::SmallVec;
use std::fmt;

/// Maximum expression length (matching C version's MAX_ELEN)
pub const MAX_EXPR_LEN: usize = 21;

/// Output format for expression display
#[derive(Debug, Clone, Copy, Default)]
pub enum OutputFormat {
    /// Default RIES format
    #[default]
    Default,
    /// Pretty format with Unicode symbols (π, ℯ, φ, √)
    Pretty,
    /// Mathematica-compatible syntax
    Mathematica,
    /// SymPy Python syntax
    SymPy,
}

impl OutputFormat {
    /// Get the name for a symbol in this format
    #[allow(dead_code)]
    pub fn symbol_name(&self, sym: Symbol) -> &'static str {
        use Symbol::*;
        match self {
            OutputFormat::Default => sym.name(),
            OutputFormat::Pretty => match sym {
                Pi => "π",
                E => "ℯ",
                Phi => "φ",
                Sqrt => "√",
                Square => "²",
                Gamma => "γ",
                Plastic => "ρ",
                Catalan => "G",
                _ => sym.name(),
            },
            OutputFormat::Mathematica => match sym {
                Pi => "Pi",
                E => "E",
                Phi => "GoldenRatio",
                Sqrt => "Sqrt",
                Square => "²",
                Ln => "Log",
                Exp => "Exp",
                SinPi => "Sin[Pi*",
                CosPi => "Cos[Pi*",
                TanPi => "Tan[Pi*",
                LambertW => "ProductLog",
                Gamma => "EulerGamma",
                _ => sym.name(),
            },
            OutputFormat::SymPy => match sym {
                Pi => "pi",
                E => "E",
                Phi => "GoldenRatio",
                Sqrt => "sqrt",
                Square => "²",
                Ln => "log",
                Exp => "exp",
                SinPi => "sin(pi*",
                CosPi => "cos(pi*",
                TanPi => "tan(pi*",
                LambertW => "lambertw",
                Gamma => "EulerGamma",
                _ => sym.name(),
            },
        }
    }
}

/// A symbolic expression in postfix notation
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Expression {
    /// Symbols in postfix order
    symbols: SmallVec<[Symbol; MAX_EXPR_LEN]>,
    /// Cached complexity score
    complexity: u32,
    /// Whether this expression contains the variable x
    contains_x: bool,
}

impl Expression {
    /// Create an empty expression
    pub fn new() -> Self {
        Self {
            symbols: SmallVec::new(),
            complexity: 0,
            contains_x: false,
        }
    }

    /// Create an expression from a slice of symbols
    #[cfg(test)]
    pub fn from_symbols(symbols: &[Symbol]) -> Self {
        // Use saturating_add to prevent overflow with maliciously large weights
        let complexity: u32 = symbols
            .iter()
            .map(|s| s.weight())
            .fold(0u32, |acc, w| acc.saturating_add(w));
        let contains_x = symbols.contains(&Symbol::X);
        Self {
            symbols: SmallVec::from_slice(symbols),
            complexity,
            contains_x,
        }
    }

    /// Parse an expression from a postfix string (e.g., "32s1+s*")
    pub fn parse(s: &str) -> Option<Self> {
        let mut symbols = SmallVec::new();
        for b in s.bytes() {
            symbols.push(Symbol::from_byte(b)?);
        }
        // Use saturating_add to prevent overflow with maliciously large weights
        let complexity: u32 = symbols
            .iter()
            .map(|s: &Symbol| s.weight())
            .fold(0u32, |acc, w| acc.saturating_add(w));
        let contains_x = symbols.contains(&Symbol::X);
        Some(Self {
            symbols,
            complexity,
            contains_x,
        })
    }

    /// Get the symbols in this expression
    #[inline]
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    /// Get the expression length
    #[inline]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if expression is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Get the complexity score
    #[inline]
    pub fn complexity(&self) -> u32 {
        self.complexity
    }

    /// Check if this expression contains the variable x
    #[inline]
    pub fn contains_x(&self) -> bool {
        self.contains_x
    }

    /// Check if this is a valid complete expression (stack depth = 1)
    ///
    /// This method is part of the public API for external consumers who may want to
    /// validate expressions before processing them.
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        let mut depth: i32 = 0;
        for sym in &self.symbols {
            match sym.seft() {
                Seft::A => depth += 1,
                Seft::B => { /* pop 1, push 1 - no change */ }
                Seft::C => depth -= 1, // pop 2, push 1
            }
            if depth < 1 {
                return false; // Stack underflow
            }
        }
        depth == 1
    }

    /// Append a symbol to this expression
    pub fn push(&mut self, sym: Symbol) {
        self.complexity += sym.weight();
        if sym == Symbol::X {
            self.contains_x = true;
        }
        self.symbols.push(sym);
    }

    /// Remove the last symbol
    pub fn pop(&mut self) -> Option<Symbol> {
        let sym = self.symbols.pop()?;
        self.complexity -= sym.weight();
        // Recompute contains_x after popping
        if sym == Symbol::X {
            self.contains_x = self.symbols.contains(&Symbol::X);
        }
        Some(sym)
    }

    /// Get the postfix string representation
    pub fn to_postfix(&self) -> String {
        self.symbols.iter().map(|s| *s as u8 as char).collect()
    }

    /// Convert to infix notation for display
    ///
    /// Uses proper operator precedence and associativity rules:
    /// - Precedence levels (higher = tighter binding):
    ///   - 100: Atoms (constants, x, function calls)
    ///   - 9: Power (right-associative)
    ///   - 7: Unary operators (negation, reciprocal)
    ///   - 6: Multiplication, division
    ///   - 5: Addition, subtraction
    /// - Right-associative operators (power) bind right-to-left
    /// - Left-associative operators bind left-to-right
    pub fn to_infix(&self) -> String {
        /// Precedence levels for operators
        const PREC_ATOM: u8 = 100; // Constants, x, function calls
        const PREC_POWER: u8 = 9; // ^ (right-associative)
        const PREC_UNARY: u8 = 8; // Unary minus, reciprocal
        const PREC_MUL: u8 = 6; // *, /
        const PREC_ADD: u8 = 4; // +, -

        /// Check if we need parentheses around an operand
        /// parent_prec: precedence of the parent operator
        /// child_prec: precedence of the child expression
        /// is_right_assoc: true if parent is right-associative
        /// is_right_operand: true if this is the right operand
        fn needs_paren(
            parent_prec: u8,
            child_prec: u8,
            is_right_assoc: bool,
            is_right_operand: bool,
        ) -> bool {
            if child_prec < parent_prec {
                return true;
            }
            // For right-associative operators, the right operand needs parens
            // if it has the same precedence (e.g., a^(b^c) needs parens)
            if is_right_assoc && is_right_operand && child_prec == parent_prec {
                return true;
            }
            false
        }

        /// Wrap in parentheses if needed
        fn maybe_paren_prec(s: &str, prec: u8, parent_prec: u8, is_right_assoc: bool, is_right: bool) -> String {
            if needs_paren(parent_prec, prec, is_right_assoc, is_right) {
                format!("({})", s)
            } else {
                s.to_string()
            }
        }

        let mut stack: Vec<(String, u8)> = Vec::new(); // (string, precedence)

        for &sym in &self.symbols {
            match sym.seft() {
                Seft::A => {
                    stack.push((sym.name().to_string(), PREC_ATOM));
                }
                Seft::B => {
                    let (arg, arg_prec) = stack.pop().unwrap_or(("?".into(), 0));
                    let result = match sym {
                        Symbol::Neg => {
                            // Negation needs parens around low-precedence expressions
                            let arg_s = maybe_paren_prec(&arg, arg_prec, PREC_UNARY, false, false);
                            format!("-{}", arg_s)
                        }
                        Symbol::Recip => {
                            let arg_s = maybe_paren_prec(&arg, arg_prec, PREC_MUL, false, false);
                            format!("1/{}", arg_s)
                        }
                        Symbol::Sqrt => format!("sqrt({})", arg),
                        Symbol::Square => {
                            let arg_s = maybe_paren_prec(&arg, arg_prec, PREC_POWER, false, false);
                            format!("{}^2", arg_s)
                        }
                        Symbol::Ln => format!("ln({})", arg),
                        Symbol::Exp => {
                            let arg_s = maybe_paren_prec(&arg, arg_prec, PREC_POWER, true, true);
                            format!("e^{}", arg_s)
                        }
                        Symbol::SinPi => format!("sinpi({})", arg),
                        Symbol::CosPi => format!("cospi({})", arg),
                        Symbol::TanPi => format!("tanpi({})", arg),
                        Symbol::LambertW => format!("W({})", arg),
                        _ => unreachable!(),
                    };
                    stack.push((result, PREC_ATOM)); // Function calls are atomic
                }
                Seft::C => {
                    let (b, b_prec) = stack.pop().unwrap_or(("?".into(), 0));
                    let (a, a_prec) = stack.pop().unwrap_or(("?".into(), 0));
                    let (result, prec) = match sym {
                        Symbol::Add => {
                            // Left operand never needs parens for left-associative +
                            // Right operand needs parens if lower precedence
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            // Right operand needs parens for - and +
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = maybe_paren_prec(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_MUL, false, true);
                            // Omit * in some cases: 2x instead of 2*x
                            if a_s.chars().last().is_some_and(|c| c.is_ascii_digit())
                                && b_s.chars().next().is_some_and(|c| c.is_alphabetic())
                            {
                                (format!("{} {}", a_s, b_s), PREC_MUL)
                            } else {
                                (format!("{}*{}", a_s, b_s), PREC_MUL)
                            }
                        }
                        Symbol::Div => {
                            let a_s = maybe_paren_prec(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_MUL + 1, false, true);
                            (format!("{}/{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Pow => {
                            // Power is right-associative: a^b^c = a^(b^c)
                            // Left operand needs parens if lower precedence
                            let a_s = maybe_paren_prec(&a, a_prec, PREC_POWER, true, false);
                            // Right operand needs parens if same or lower precedence (due to right-assoc)
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_POWER, true, true);
                            (format!("{}^{}", a_s, b_s), PREC_POWER)
                        }
                        Symbol::Root => (format!("{}\"/{}", a, b), PREC_POWER),
                        Symbol::Log => (format!("log_{}({})", a, b), PREC_ATOM),
                        Symbol::Atan2 => (format!("atan2({}, {})", a, b), PREC_ATOM),
                        _ => unreachable!(),
                    };
                    stack.push((result, prec));
                }
            }
        }

        stack.pop().map(|(s, _)| s).unwrap_or_else(|| "?".into())
    }

    /// Convert to infix notation with specified format
    pub fn to_infix_with_format(&self, format: OutputFormat) -> String {
        // For now, pretty format just wraps the default
        // Full implementation would customize each operation's output
        match format {
            OutputFormat::Default => self.to_infix(),
            OutputFormat::Pretty => {
                let mut result = self.to_infix();
                // Simple Unicode substitutions
                result = result.replace("pi", "π");
                result = result.replace("sqrt(", "√(");
                result = result.replace("^2", "²");
                result
            }
            OutputFormat::Mathematica => {
                let mut result = self.to_infix();
                result = result.replace("pi", "Pi");
                result = result.replace("ln(", "Log[");
                result = result.replace("sqrt(", "Sqrt[");
                result = result.replace("exp(", "Exp[");
                result = result.replace("sinpi(", "Sin[Pi*");
                result = result.replace("cospi(", "Cos[Pi*");
                result
            }
            OutputFormat::SymPy => {
                let mut result = self.to_infix();
                result = result.replace("ln(", "log(");
                result = result.replace("sinpi(", "sin(pi*");
                result = result.replace("cospi(", "cos(pi*");
                result = result.replace("W(", "lambertw(");
                result
            }
        }
    }
}

impl Default for Expression {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_infix())
    }
}

impl fmt::Debug for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Expr[{}] = {}", self.to_postfix(), self.to_infix())
    }
}

/// An evaluated expression with its numeric value
#[derive(Clone)]
pub struct EvaluatedExpr {
    /// The symbolic expression
    pub expr: Expression,
    /// Computed value at x = target
    pub value: f64,
    /// Derivative with respect to x
    pub derivative: f64,
    /// Numeric type classification
    ///
    /// This field is part of the public API for library consumers who need
    /// to track the numeric type of evaluated expressions.
    #[allow(dead_code)]
    pub num_type: NumType,
}

impl EvaluatedExpr {
    pub fn new(expr: Expression, value: f64, derivative: f64, num_type: NumType) -> Self {
        Self {
            expr,
            value,
            derivative,
            num_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expression() {
        let expr = Expression::parse("32+").unwrap();
        assert_eq!(expr.len(), 3);
        assert_eq!(expr.to_postfix(), "32+");
        assert!(!expr.contains_x());
    }

    #[test]
    fn test_expression_validity() {
        // Valid: 3 2 + (pushes 3, pushes 2, adds them -> 1 value)
        assert!(Expression::parse("32+").unwrap().is_valid());

        // Valid: x 2 ^ (x squared)
        assert!(Expression::parse("xs").unwrap().is_valid());

        // Invalid: 3 + (not enough operands)
        assert!(!Expression::parse("3+").unwrap().is_valid());

        // Invalid: 3 2 (two values left on stack)
        assert!(!Expression::parse("32").unwrap().is_valid());
    }

    #[test]
    fn test_infix_conversion() {
        assert_eq!(Expression::parse("32+").unwrap().to_infix(), "3+2");
        assert_eq!(Expression::parse("32*").unwrap().to_infix(), "3*2");
        assert_eq!(Expression::parse("xs").unwrap().to_infix(), "x^2");
        assert_eq!(Expression::parse("xq").unwrap().to_infix(), "sqrt(x)");
        assert_eq!(Expression::parse("32+5*").unwrap().to_infix(), "(3+2)*5");
    }

    #[test]
    fn test_complexity() {
        let expr = Expression::parse("xs").unwrap(); // x^2
                                                     // x = 6, s (square) = 5
        assert_eq!(expr.complexity(), 6 + 5);
    }
}
