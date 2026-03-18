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

    /// Parse a well-formed postfix expression (e.g., "32s1+s*").
    ///
    /// This validates stack discipline while parsing, so malformed or incomplete
    /// postfix strings return `None` instead of constructing an expression that
    /// will later panic during formatting.
    pub fn parse(s: &str) -> Option<Self> {
        let mut symbols = SmallVec::new();
        for b in s.bytes() {
            symbols.push(Symbol::from_byte(b)?);
        }
        if !Self::is_valid_postfix(&symbols) {
            return None;
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

    /// Count occurrences of a symbol in this expression.
    #[inline]
    pub fn count_symbol(&self, sym: Symbol) -> u32 {
        self.symbols.iter().filter(|&&s| s == sym).count() as u32
    }

    /// Check if this is a valid complete expression (stack depth = 1)
    ///
    /// This method is part of the public API for external consumers who may want to
    /// validate expressions before processing them.
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        Self::is_valid_postfix(&self.symbols)
    }

    fn is_valid_postfix(symbols: &[Symbol]) -> bool {
        let mut depth: i32 = 0;
        for sym in symbols {
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
        // Use saturating_add to prevent overflow with many operations
        self.complexity = self.complexity.saturating_add(sym.weight());
        if sym == Symbol::X {
            self.contains_x = true;
        }
        self.symbols.push(sym);
    }

    /// Remove the last symbol
    pub fn pop(&mut self) -> Option<Symbol> {
        let sym = self.symbols.pop()?;
        // Use saturating_sub to prevent underflow (shouldn't happen with valid state)
        self.complexity = self.complexity.saturating_sub(sym.weight());
        // Recompute contains_x after popping
        if sym == Symbol::X {
            self.contains_x = self.symbols.contains(&Symbol::X);
        }
        Some(sym)
    }

    /// Append a symbol using a symbol table for weight lookup
    ///
    /// This is the table-driven version that uses per-run configuration
    /// instead of global overrides.
    pub fn push_with_table(&mut self, sym: Symbol, table: &crate::symbol_table::SymbolTable) {
        // Use saturating_add to prevent overflow with many operations
        self.complexity = self.complexity.saturating_add(table.weight(sym));
        if sym == Symbol::X {
            self.contains_x = true;
        }
        self.symbols.push(sym);
    }

    /// Remove the last symbol using a symbol table for weight lookup
    ///
    /// This is the table-driven version that uses per-run configuration
    /// instead of global overrides.
    pub fn pop_with_table(&mut self, table: &crate::symbol_table::SymbolTable) -> Option<Symbol> {
        let sym = self.symbols.pop()?;
        // Use saturating_sub to prevent underflow (shouldn't happen with valid state)
        self.complexity = self.complexity.saturating_sub(table.weight(sym));
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
    ///
    /// Convert to infix notation, returning `Err(EvalError::StackUnderflow)` if
    /// the expression is malformed (e.g. a binary operator with no operands).
    ///
    /// Prefer this over [`to_infix`](Self::to_infix) when the expression may come
    /// from untrusted or user-provided input.
    pub fn try_to_infix(&self) -> Result<String, crate::eval::EvalError> {
        const PREC_ATOM: u8 = 100;
        const PREC_POWER: u8 = 9;
        const PREC_UNARY: u8 = 8;
        const PREC_MUL: u8 = 6;
        const PREC_ADD: u8 = 4;

        fn needs_paren(
            parent_prec: u8,
            child_prec: u8,
            is_right_assoc: bool,
            is_right_operand: bool,
        ) -> bool {
            if child_prec < parent_prec {
                return true;
            }
            if is_right_assoc && is_right_operand && child_prec == parent_prec {
                return true;
            }
            false
        }

        fn maybe_paren_prec(
            s: &str,
            prec: u8,
            parent_prec: u8,
            is_right_assoc: bool,
            is_right: bool,
        ) -> String {
            if needs_paren(parent_prec, prec, is_right_assoc, is_right) {
                format!("({})", s)
            } else {
                s.to_string()
            }
        }

        let mut stack: Vec<(String, u8)> = Vec::new();

        for &sym in &self.symbols {
            match sym.seft() {
                Seft::A => {
                    stack.push((sym.display_name(), PREC_ATOM));
                }
                Seft::B => {
                    let (arg, arg_prec) =
                        stack.pop().ok_or(crate::eval::EvalError::StackUnderflow)?;
                    let result = match sym {
                        Symbol::Neg => {
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
                        Symbol::UserFunction0
                        | Symbol::UserFunction1
                        | Symbol::UserFunction2
                        | Symbol::UserFunction3
                        | Symbol::UserFunction4
                        | Symbol::UserFunction5
                        | Symbol::UserFunction6
                        | Symbol::UserFunction7
                        | Symbol::UserFunction8
                        | Symbol::UserFunction9
                        | Symbol::UserFunction10
                        | Symbol::UserFunction11
                        | Symbol::UserFunction12
                        | Symbol::UserFunction13
                        | Symbol::UserFunction14
                        | Symbol::UserFunction15 => format!("{}({})", sym.display_name(), arg),
                        _ => "?".to_string(),
                    };
                    stack.push((result, PREC_ATOM));
                }
                Seft::C => {
                    let (b, b_prec) = stack.pop().ok_or(crate::eval::EvalError::StackUnderflow)?;
                    let (a, a_prec) = stack.pop().ok_or(crate::eval::EvalError::StackUnderflow)?;
                    let (result, prec) = match sym {
                        Symbol::Add => {
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = maybe_paren_prec(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_MUL, false, true);
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
                            let a_s = maybe_paren_prec(&a, a_prec, PREC_POWER, true, false);
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

        Ok(stack.pop().map(|(s, _)| s).unwrap_or_else(|| "?".into()))
    }

    pub fn to_infix(&self) -> String {
        self.try_to_infix()
            .expect("stack underflow in to_infix: expression is not valid postfix")
    }

    /// Convert to infix notation using a symbol table for display names
    ///
    /// This is the table-driven version that uses per-run configuration
    /// instead of global overrides for symbol display names.
    pub fn to_infix_with_table(&self, table: &crate::symbol_table::SymbolTable) -> String {
        /// Precedence levels for operators
        const PREC_ATOM: u8 = 100; // Constants, x, function calls
        const PREC_POWER: u8 = 9; // ^ (right-associative)
        const PREC_UNARY: u8 = 8; // Unary minus, reciprocal
        const PREC_MUL: u8 = 6; // *, /
        const PREC_ADD: u8 = 4; // +, -

        /// Check if we need parentheses around an operand
        fn needs_paren(
            parent_prec: u8,
            child_prec: u8,
            is_right_assoc: bool,
            is_right_operand: bool,
        ) -> bool {
            if child_prec < parent_prec {
                return true;
            }
            if is_right_assoc && is_right_operand && child_prec == parent_prec {
                return true;
            }
            false
        }

        /// Wrap in parentheses if needed
        fn maybe_paren_prec(
            s: &str,
            prec: u8,
            parent_prec: u8,
            is_right_assoc: bool,
            is_right: bool,
        ) -> String {
            if needs_paren(parent_prec, prec, is_right_assoc, is_right) {
                format!("({})", s)
            } else {
                s.to_string()
            }
        }

        let mut stack: Vec<(String, u8)> = Vec::new();

        for &sym in &self.symbols {
            match sym.seft() {
                Seft::A => {
                    stack.push((table.name(sym).to_string(), PREC_ATOM));
                }
                Seft::B => {
                    let (arg, arg_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let result = match sym {
                        Symbol::Neg => {
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
                        Symbol::UserFunction0
                        | Symbol::UserFunction1
                        | Symbol::UserFunction2
                        | Symbol::UserFunction3
                        | Symbol::UserFunction4
                        | Symbol::UserFunction5
                        | Symbol::UserFunction6
                        | Symbol::UserFunction7
                        | Symbol::UserFunction8
                        | Symbol::UserFunction9
                        | Symbol::UserFunction10
                        | Symbol::UserFunction11
                        | Symbol::UserFunction12
                        | Symbol::UserFunction13
                        | Symbol::UserFunction14
                        | Symbol::UserFunction15 => format!("{}({})", table.name(sym), arg),
                        _ => "?".to_string(),
                    };
                    stack.push((result, PREC_ATOM));
                }
                Seft::C => {
                    let (b, b_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let (a, a_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let (result, prec) = match sym {
                        Symbol::Add => {
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = maybe_paren_prec(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren_prec(&b, b_prec, PREC_MUL, false, true);
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
                            let a_s = maybe_paren_prec(&a, a_prec, PREC_POWER, true, false);
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
            OutputFormat::Mathematica => self.to_infix_mathematica(),
            OutputFormat::SymPy => self.to_infix_sympy(),
        }
    }

    /// Count the number of operators (non-atoms) in the expression
    pub fn operator_count(&self) -> usize {
        self.symbols
            .iter()
            .filter(|sym| sym.seft() != Seft::A)
            .count()
    }

    /// Compute the maximum depth of the expression tree
    pub fn tree_depth(&self) -> usize {
        let mut stack: Vec<usize> = Vec::with_capacity(self.len());
        for &sym in &self.symbols {
            match sym.seft() {
                Seft::A => stack.push(1),
                Seft::B => {
                    let Some(arg_depth) = stack.pop() else {
                        return 0;
                    };
                    stack.push(arg_depth.saturating_add(1));
                }
                Seft::C => {
                    let Some(rhs_depth) = stack.pop() else {
                        return 0;
                    };
                    let Some(lhs_depth) = stack.pop() else {
                        return 0;
                    };
                    stack.push(lhs_depth.max(rhs_depth).saturating_add(1));
                }
            }
        }
        if stack.len() == 1 {
            stack[0]
        } else {
            0
        }
    }

    pub fn to_infix_mathematica(&self) -> String {
        const PREC_ATOM: u8 = 100;
        const PREC_POWER: u8 = 9;
        const PREC_UNARY: u8 = 8;
        const PREC_MUL: u8 = 6;
        const PREC_ADD: u8 = 4;

        fn needs_paren(
            parent_prec: u8,
            child_prec: u8,
            is_right_assoc: bool,
            is_right_operand: bool,
        ) -> bool {
            if child_prec < parent_prec {
                return true;
            }
            if is_right_assoc && is_right_operand && child_prec == parent_prec {
                return true;
            }
            false
        }

        fn maybe_paren(
            s: &str,
            prec: u8,
            parent_prec: u8,
            is_right_assoc: bool,
            is_right: bool,
        ) -> String {
            if needs_paren(parent_prec, prec, is_right_assoc, is_right) {
                format!("({})", s)
            } else {
                s.to_string()
            }
        }

        let mut stack: Vec<(String, u8)> = Vec::new();

        for &sym in &self.symbols {
            match sym.seft() {
                Seft::A => {
                    let s = match sym {
                        Symbol::Pi => "Pi",
                        Symbol::E => "E",
                        Symbol::Phi => "GoldenRatio",
                        Symbol::Gamma => "EulerGamma",
                        Symbol::Apery => "Zeta[3]",
                        Symbol::Catalan => "Catalan",
                        _ => "",
                    };
                    let name = if s.is_empty() {
                        sym.display_name()
                    } else {
                        s.to_string()
                    };
                    stack.push((name, PREC_ATOM));
                }
                Seft::B => {
                    let (arg, arg_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let result = match sym {
                        Symbol::Neg => {
                            let s = maybe_paren(&arg, arg_prec, PREC_UNARY, false, false);
                            format!("-{}", s)
                        }
                        Symbol::Recip => {
                            let s = maybe_paren(&arg, arg_prec, PREC_MUL, false, false);
                            format!("1/{}", s)
                        }
                        Symbol::Sqrt => format!("Sqrt[{}]", arg),
                        Symbol::Square => {
                            let s = maybe_paren(&arg, arg_prec, PREC_POWER, false, false);
                            format!("{}^2", s)
                        }
                        Symbol::Ln => format!("Log[{}]", arg),
                        Symbol::Exp => format!("Exp[{}]", arg),
                        Symbol::SinPi => format!("Sin[Pi*{}]", arg),
                        Symbol::CosPi => format!("Cos[Pi*{}]", arg),
                        Symbol::TanPi => format!("Tan[Pi*{}]", arg),
                        Symbol::LambertW => format!("ProductLog[{}]", arg),
                        Symbol::UserFunction0
                        | Symbol::UserFunction1
                        | Symbol::UserFunction2
                        | Symbol::UserFunction3
                        | Symbol::UserFunction4
                        | Symbol::UserFunction5
                        | Symbol::UserFunction6
                        | Symbol::UserFunction7
                        | Symbol::UserFunction8
                        | Symbol::UserFunction9
                        | Symbol::UserFunction10
                        | Symbol::UserFunction11
                        | Symbol::UserFunction12
                        | Symbol::UserFunction13
                        | Symbol::UserFunction14
                        | Symbol::UserFunction15 => format!("{}[{}]", sym.display_name(), arg),
                        _ => "?".to_string(),
                    };
                    stack.push((result, PREC_ATOM));
                }
                Seft::C => {
                    let (b, b_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let (a, a_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let (result, prec) = match sym {
                        Symbol::Add => {
                            let b_s = maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren(&b, b_prec, PREC_MUL, false, true);
                            (format!("{}*{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Div => {
                            let a_s = maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren(&b, b_prec, PREC_MUL + 1, false, true);
                            (format!("{}/{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Pow => {
                            let a_s = maybe_paren(&a, a_prec, PREC_POWER, true, false);
                            let b_s = maybe_paren(&b, b_prec, PREC_POWER, true, true);
                            (format!("{}^{}", a_s, b_s), PREC_POWER)
                        }
                        Symbol::Root => {
                            let b_s = maybe_paren(&b, b_prec, PREC_POWER, true, false);
                            (format!("{}^(1/{})", b_s, a), PREC_POWER)
                        }
                        Symbol::Log => (format!("Log[{}, {}]", a, b), PREC_ATOM),
                        Symbol::Atan2 => (format!("ArcTan[{}, {}]", b, a), PREC_ATOM),
                        _ => unreachable!(),
                    };
                    stack.push((result, prec));
                }
            }
        }

        stack
            .pop()
            .map(|(s, _)| s)
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn to_infix_sympy(&self) -> String {
        const PREC_ATOM: u8 = 100;
        const PREC_POWER: u8 = 9;
        const PREC_UNARY: u8 = 8;
        const PREC_MUL: u8 = 6;
        const PREC_ADD: u8 = 4;

        fn needs_paren(
            parent_prec: u8,
            child_prec: u8,
            is_right_assoc: bool,
            is_right_operand: bool,
        ) -> bool {
            if child_prec < parent_prec {
                return true;
            }
            if is_right_assoc && is_right_operand && child_prec == parent_prec {
                return true;
            }
            false
        }

        fn maybe_paren(
            s: &str,
            prec: u8,
            parent_prec: u8,
            is_right_assoc: bool,
            is_right: bool,
        ) -> String {
            if needs_paren(parent_prec, prec, is_right_assoc, is_right) {
                format!("({})", s)
            } else {
                s.to_string()
            }
        }

        let mut stack: Vec<(String, u8)> = Vec::new();

        for &sym in &self.symbols {
            match sym.seft() {
                Seft::A => {
                    let s = match sym {
                        Symbol::Pi => "pi",
                        Symbol::E => "E",
                        Symbol::Phi => "GoldenRatio",
                        Symbol::Gamma => "EulerGamma",
                        Symbol::Apery => "zeta(3)",
                        Symbol::Catalan => "Catalan",
                        _ => "",
                    };
                    let name = if s.is_empty() {
                        sym.display_name()
                    } else {
                        s.to_string()
                    };
                    stack.push((name, PREC_ATOM));
                }
                Seft::B => {
                    let (arg, arg_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let result = match sym {
                        Symbol::Neg => {
                            let s = maybe_paren(&arg, arg_prec, PREC_UNARY, false, false);
                            format!("-{}", s)
                        }
                        Symbol::Recip => {
                            let s = maybe_paren(&arg, arg_prec, PREC_MUL, false, false);
                            format!("1/{}", s)
                        }
                        Symbol::Sqrt => format!("sqrt({})", arg),
                        Symbol::Square => {
                            let s = maybe_paren(&arg, arg_prec, PREC_POWER, false, false);
                            format!("{}**2", s)
                        }
                        Symbol::Ln => format!("log({})", arg),
                        Symbol::Exp => format!("exp({})", arg),
                        Symbol::SinPi => format!("sin(pi*{})", arg),
                        Symbol::CosPi => format!("cos(pi*{})", arg),
                        Symbol::TanPi => format!("tan(pi*{})", arg),
                        Symbol::LambertW => format!("lambertw({})", arg),
                        Symbol::UserFunction0
                        | Symbol::UserFunction1
                        | Symbol::UserFunction2
                        | Symbol::UserFunction3
                        | Symbol::UserFunction4
                        | Symbol::UserFunction5
                        | Symbol::UserFunction6
                        | Symbol::UserFunction7
                        | Symbol::UserFunction8
                        | Symbol::UserFunction9
                        | Symbol::UserFunction10
                        | Symbol::UserFunction11
                        | Symbol::UserFunction12
                        | Symbol::UserFunction13
                        | Symbol::UserFunction14
                        | Symbol::UserFunction15 => format!("{}({})", sym.display_name(), arg),
                        _ => "?".to_string(),
                    };
                    stack.push((result, PREC_ATOM));
                }
                Seft::C => {
                    let (b, b_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let (a, a_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let (result, prec) = match sym {
                        Symbol::Add => {
                            let b_s = maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren(&b, b_prec, PREC_MUL, false, true);
                            (format!("{}*{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Div => {
                            let a_s = maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = maybe_paren(&b, b_prec, PREC_MUL + 1, false, true);
                            (format!("{}/{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Pow => {
                            let a_s = maybe_paren(&a, a_prec, PREC_POWER, true, false);
                            let b_s = maybe_paren(&b, b_prec, PREC_POWER, true, true);
                            (format!("{}**{}", a_s, b_s), PREC_POWER)
                        }
                        Symbol::Root => {
                            let b_s = maybe_paren(&b, b_prec, PREC_POWER, true, false);
                            (format!("{}**(1/{})", b_s, a), PREC_POWER)
                        }
                        Symbol::Log => (format!("log({}, {})", b, a), PREC_ATOM),
                        Symbol::Atan2 => (format!("atan2({}, {})", a, b), PREC_ATOM),
                        _ => unreachable!(),
                    };
                    stack.push((result, prec));
                }
            }
        }

        stack
            .pop()
            .map(|(s, _)| s)
            .unwrap_or_else(|| "?".to_string())
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
#[derive(Clone, Debug)]
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
        assert!(Expression::parse("3+").is_none());

        // Invalid: 3 2 (two values left on stack)
        assert!(Expression::parse("32").is_none());
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
                                                     // x = 15, s (square) = 9
        assert_eq!(expr.complexity(), 15 + 9);
    }

    #[test]
    fn test_tree_depth_atom() {
        // Single atom has depth 1
        assert_eq!(Expression::parse("x").unwrap().tree_depth(), 1);
        assert_eq!(Expression::parse("1").unwrap().tree_depth(), 1);
        assert_eq!(Expression::parse("p").unwrap().tree_depth(), 1); // pi
    }

    #[test]
    fn test_tree_depth_unary() {
        // Unary op adds 1 to depth
        assert_eq!(Expression::parse("xq").unwrap().tree_depth(), 2); // sqrt(x)
        assert_eq!(Expression::parse("xs").unwrap().tree_depth(), 2); // x^2
        assert_eq!(Expression::parse("xn").unwrap().tree_depth(), 2); // -x
    }

    #[test]
    fn test_tree_depth_binary() {
        // Binary op takes max of children + 1
        assert_eq!(Expression::parse("12+").unwrap().tree_depth(), 2); // 1+2
        assert_eq!(Expression::parse("x2*").unwrap().tree_depth(), 2); // x*2
        assert_eq!(Expression::parse("x1+2*").unwrap().tree_depth(), 3); // (x+1)*2
    }

    #[test]
    fn test_tree_depth_nested() {
        // Deeply nested expressions
        assert_eq!(Expression::parse("xqq").unwrap().tree_depth(), 3); // sqrt(sqrt(x))
        assert_eq!(Expression::parse("12+34+*").unwrap().tree_depth(), 3); // (1+2)*(3+4)
    }

    #[test]
    fn test_tree_depth_empty() {
        // Empty expression has depth 0
        assert_eq!(Expression::new().tree_depth(), 0);
    }

    #[test]
    fn test_tree_depth_malformed() {
        // Malformed expressions return 0
        assert_eq!(
            Expression::from_symbols(&[Symbol::X, Symbol::One]).tree_depth(),
            0
        );
    }

    #[test]
    fn test_operator_count_atom() {
        // Single atom has no operators
        assert_eq!(Expression::parse("x").unwrap().operator_count(), 0);
        assert_eq!(Expression::parse("1").unwrap().operator_count(), 0);
        assert_eq!(Expression::parse("p").unwrap().operator_count(), 0);
    }

    #[test]
    fn test_operator_count_unary() {
        // Unary op counts as 1 operator
        assert_eq!(Expression::parse("xq").unwrap().operator_count(), 1);
        assert_eq!(Expression::parse("xs").unwrap().operator_count(), 1);
        assert_eq!(Expression::parse("xn").unwrap().operator_count(), 1);
    }

    #[test]
    fn test_operator_count_binary() {
        // Binary op counts as 1 operator
        assert_eq!(Expression::parse("12+").unwrap().operator_count(), 1);
        assert_eq!(Expression::parse("x2*").unwrap().operator_count(), 1);
    }

    #[test]
    fn test_operator_count_complex() {
        // Multiple operators
        assert_eq!(Expression::parse("x1+2*").unwrap().operator_count(), 2); // (x+1)*2
        assert_eq!(Expression::parse("xq1+").unwrap().operator_count(), 2); // sqrt(x)+1
        assert_eq!(Expression::parse("12+34+*").unwrap().operator_count(), 3); // (1+2)*(3+4)
    }

    #[test]
    fn test_operator_count_empty() {
        assert_eq!(Expression::new().operator_count(), 0);
    }

    #[test]
    fn test_push_pop_complexity_saturating() {
        let mut expr = Expression::new();

        // Push should use saturating_add
        for _ in 0..1000 {
            expr.push(Symbol::X);
        }
        // Complexity should saturate, not overflow
        assert!(expr.complexity() < u32::MAX);

        // Pop should use saturating_sub
        for _ in 0..1000 {
            expr.pop();
        }
        // Should be back to 0 without underflow
        assert_eq!(expr.complexity(), 0);
    }

    /// Issue 6: to_infix must not silently produce '?' for invalid expressions.
    /// Instead, stack underflow in the mid-loop pops is a programming error and
    /// should panic with a clear message via expect().
    #[test]
    #[should_panic(expected = "stack underflow in to_infix")]
    fn test_to_infix_panics_on_malformed_expression() {
        // An expression with only a binary operator has no operands — stack underflows
        // on the first pop inside the loop. from_symbols bypasses parse validation.
        let expr = Expression::from_symbols(&[Symbol::Add]);
        let _ = expr.to_infix();
    }

    #[test]
    fn test_try_to_infix_returns_err_on_malformed_expression() {
        // from_symbols bypasses parse validation, producing a malformed postfix string.
        // try_to_infix should return Err(StackUnderflow) rather than panicking.
        let expr = Expression::from_symbols(&[Symbol::Add]);
        let result = expr.try_to_infix();
        assert!(
            result.is_err(),
            "try_to_infix on malformed expression should return Err, got Ok({:?})",
            result.ok()
        );
    }

    #[test]
    fn test_try_to_infix_succeeds_on_valid_expression() {
        let expr = Expression::parse("32+").unwrap();
        let result = expr.try_to_infix();
        assert_eq!(result.unwrap(), "3+2");
    }
}
