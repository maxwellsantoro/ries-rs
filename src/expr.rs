//! Expression representation and manipulation
//!
//! Expressions are stored in postfix (reverse Polish) notation.

use crate::symbol::{NumType, Seft, Symbol};
use smallvec::SmallVec;
use std::fmt;
use std::hash::{Hash, Hasher};

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
///
/// Exact integer annotations live in a process-wide table keyed by
/// [`Expression::literal_id`], not in this struct. Search clones and scans
/// millions of expressions that never carry an annotation; an inline list
/// here doubled the struct and showed up in search time. `literal_id == 0`
/// means "no annotation" and never takes the lock. The id fits in the padding
/// after `contains_x`, so a search expression is the same size as before.
pub struct Expression {
    /// Symbols in postfix order
    symbols: SmallVec<[Symbol; MAX_EXPR_LEN]>,
    /// Cached complexity score
    complexity: u32,
    /// Whether this expression contains the variable x
    contains_x: bool,
    /// Key into [`literal_store`]. Zero when this expression has no solver integer.
    literal_id: u16,
}

/// Whether `child` must be parenthesized under `parent`.
///
/// Power is right-associative, so `(a^b)^c` needs parentheses and `a^(b^c)`
/// prints as `a^b^c`. Addition, subtraction, multiplication, and division are
/// left-associative, so an equal-precedence expression on the right
/// (`a-(b-c)`, `a-(b+c)`) needs parentheses.
fn infix_needs_paren(
    parent_prec: u8,
    child_prec: u8,
    is_right_assoc: bool,
    is_right_operand: bool,
) -> bool {
    if child_prec < parent_prec {
        return true;
    }
    if child_prec == parent_prec {
        if is_right_assoc {
            return !is_right_operand;
        }
        return is_right_operand;
    }
    false
}

/// Parenthesize a subexpression that is not a primary (atom or function call).
fn infix_paren_unless_primary(s: &str, prec: u8) -> String {
    if prec < 100 {
        format!("({})", s)
    } else {
        s.to_string()
    }
}

/// Log bases that are themselves calls (`log_sqrt(2)(3)`) or operators need grouping.
fn infix_group_log_base(s: &str, prec: u8) -> String {
    if prec < 100 || s.contains('(') {
        format!("({})", s)
    } else {
        s.to_string()
    }
}

/// Decimal digits of a finite integer `f64`, with no exponent and no rounding.
///
/// Every finite value with magnitude at least 2^52 is an integer. Powers of two
/// past 2^53 are exact too, but `Display` would print them in scientific
/// notation and a `u64` cannot hold 2^64. The digits are the integer value of
/// the significand, shifted by the unbiased exponent.
pub(crate) fn format_exact_f64_integer(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let negative = value.is_sign_negative();
    let bits = value.abs().to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    if biased == 0 {
        return "0".to_string();
    }
    let mut significand = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    // Little-endian decimal digits of the 53-bit significand.
    let mut digits = Vec::new();
    while significand > 0 {
        digits.push((significand % 10) as u8);
        significand /= 10;
    }
    let shift = biased - 1023 - 52;
    if shift > 0 {
        for _ in 0..shift {
            let mut carry = 0u8;
            for digit in &mut digits {
                let scaled = *digit * 2 + carry;
                *digit = scaled % 10;
                carry = scaled / 10;
            }
            if carry > 0 {
                digits.push(carry);
            }
        }
    } else if shift < 0 {
        for _ in 0..(-shift) {
            let mut remainder = 0u8;
            for digit in digits.iter_mut().rev() {
                let scaled = remainder * 10 + *digit;
                *digit = scaled / 2;
                remainder = scaled % 2;
            }
            while digits.last() == Some(&0) && digits.len() > 1 {
                digits.pop();
            }
        }
    }
    while digits.last() == Some(&0) && digits.len() > 1 {
        digits.pop();
    }
    let mut text = String::with_capacity(digits.len() + usize::from(negative));
    if negative {
        text.push('-');
    }
    for digit in digits.iter().rev() {
        text.push(char::from(b'0' + *digit));
    }
    text
}

fn infix_maybe_paren(
    s: &str,
    prec: u8,
    parent_prec: u8,
    is_right_assoc: bool,
    is_right: bool,
) -> String {
    // `1--x`, `2*-x`, and `1/-x` are not primaries. Parenthesize a leading minus
    // on the right of an operator.
    let leading_minus = is_right && s.starts_with('-');
    if leading_minus || infix_needs_paren(parent_prec, prec, is_right_assoc, is_right) {
        format!("({})", s)
    } else {
        s.to_string()
    }
}

impl Expression {
    /// Create an empty expression
    pub fn new() -> Self {
        Self {
            symbols: SmallVec::new(),
            complexity: 0,
            contains_x: false,
            literal_id: 0,
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
            literal_id: 0,
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
            literal_id: 0,
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

    /// Remember that the whole expression is the finite integer `value`.
    ///
    /// Later combinations shift the recorded span with the symbols. Evaluation
    /// and printing use this value, not the postfix scaffold under the span.
    pub(crate) fn mark_decimal_literal(&mut self, value: f64) {
        if !value.is_finite() {
            return;
        }
        let len = self.symbols.len();
        if len == 0 || len > u32::MAX as usize {
            return;
        }
        self.literal_id =
            literal_store::push_span(self.literal_id, (0, len as u32, value.to_bits()));
    }

    pub(crate) fn append_decimal_literals(&mut self, other: &Expression, shift: u32) {
        if other.literal_id == 0 {
            return;
        }
        self.literal_id = literal_store::append_spans(self.literal_id, other.literal_id, shift);
    }

    fn trim_decimal_literals(&mut self) {
        if self.literal_id == 0 {
            return;
        }
        let n = self.symbols.len() as u32;
        self.literal_id = literal_store::retain_spans(self.literal_id, n);
    }

    /// Solver-inserted integers are rare. Search expressions leave this empty,
    /// and the evaluator skips the literal scan in that case.
    #[inline(always)]
    pub(crate) fn has_decimal_literals(&self) -> bool {
        self.literal_id != 0
    }

    /// When a decimal literal starts at `index`, its symbol length and value.
    pub(crate) fn decimal_literal_at(&self, index: usize) -> Option<(usize, f64)> {
        if self.literal_id == 0 {
            return None;
        }
        literal_store::span_at(self.literal_id, index)
    }

    /// If a decimal literal starts at `index`, return its text and advance past it.
    fn take_decimal_literal(&self, index: &mut usize) -> Option<String> {
        let (len, value) = self.decimal_literal_at(*index)?;
        *index += len;
        Some(format_exact_f64_integer(value))
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
        // Search never annotates integers. Skip the table unless one is present.
        if self.literal_id != 0 {
            self.trim_decimal_literals();
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
        if self.literal_id != 0 {
            self.trim_decimal_literals();
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
    ///   - 9: Power (right-associative), including `^2` and `e^`
    ///   - 8: Unary minus
    ///   - 6: Multiplication, division, and reciprocal (`1/`)
    ///   - 4: Addition, subtraction
    /// - Right-associative operators (power) bind right-to-left
    /// - Left-associative operators bind left-to-right
    /// - Printed grouping round-trips: parsing the infix with these rules
    ///   yields the same value as the postfix expression. Unary results keep
    ///   the precedence of the operator they render as, so chains such as
    ///   `-(1/(1/(-phi)))` are not flattened.
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

        let mut stack: Vec<(String, u8)> = Vec::new();

        let mut index = 0;
        while index < self.symbols.len() {
            if let Some(literal) = self.take_decimal_literal(&mut index) {
                stack.push((literal, PREC_ATOM));
                continue;
            }
            let sym = self.symbols[index];
            index += 1;
            match sym.seft() {
                Seft::A => {
                    stack.push((sym.display_name(), PREC_ATOM));
                }
                Seft::B => {
                    let (arg, arg_prec) =
                        stack.pop().ok_or(crate::eval::EvalError::StackUnderflow)?;
                    // Unary results carry the precedence of the operator they
                    // print as. Tagging them as atoms drops parentheses, so
                    // `-(1/(1/(-phi)))` used to render as `-1/1/-phi`.
                    // Negation's operand is parenthesized unless it binds
                    // strictly tighter than unary minus (power does: `-x^2`).
                    let (result, prec) = match sym {
                        Symbol::Neg => {
                            let arg_s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, false, false);
                            (format!("-{}", arg_s), PREC_UNARY)
                        }
                        Symbol::Recip => {
                            // Denominator is the right side of a division.
                            let arg_s =
                                infix_maybe_paren(&arg, arg_prec, PREC_MUL + 1, false, true);
                            (format!("1/{}", arg_s), PREC_MUL)
                        }
                        Symbol::Sqrt => (format!("sqrt({})", arg), PREC_ATOM),
                        Symbol::Square => {
                            let arg_s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, true, false);
                            (format!("{}^2", arg_s), PREC_POWER)
                        }
                        Symbol::Ln => (format!("ln({})", arg), PREC_ATOM),
                        Symbol::Exp => {
                            let arg_s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, true, true);
                            (format!("e^{}", arg_s), PREC_POWER)
                        }
                        Symbol::SinPi => (format!("sinpi({})", arg), PREC_ATOM),
                        Symbol::CosPi => (format!("cospi({})", arg), PREC_ATOM),
                        Symbol::TanPi => (format!("tanpi({})", arg), PREC_ATOM),
                        Symbol::LambertW => (format!("W({})", arg), PREC_ATOM),
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
                        | Symbol::UserFunction15 => {
                            (format!("{}({})", sym.display_name(), arg), PREC_ATOM)
                        }
                        _ => ("?".to_string(), PREC_ATOM),
                    };
                    stack.push((result, prec));
                }
                Seft::C => {
                    let (b, b_prec) = stack.pop().ok_or(crate::eval::EvalError::StackUnderflow)?;
                    let (a, a_prec) = stack.pop().ok_or(crate::eval::EvalError::StackUnderflow)?;
                    let (result, prec) = match sym {
                        Symbol::Add => {
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL, false, true);
                            if a_s.chars().last().is_some_and(|c| c.is_ascii_digit())
                                && b_s.chars().next().is_some_and(|c| c.is_alphabetic())
                            {
                                (format!("{} {}", a_s, b_s), PREC_MUL)
                            } else {
                                (format!("{}*{}", a_s, b_s), PREC_MUL)
                            }
                        }
                        Symbol::Div => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL + 1, false, true);
                            (format!("{}/{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Pow => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_POWER, true, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_POWER, true, true);
                            (format!("{}^{}", a_s, b_s), PREC_POWER)
                        }
                        Symbol::Root => {
                            let index = infix_paren_unless_primary(&a, a_prec);
                            let radicand = infix_paren_unless_primary(&b, b_prec);
                            (format!("{}\"/{}", index, radicand), PREC_POWER)
                        }
                        Symbol::Log => {
                            let base = infix_group_log_base(&a, a_prec);
                            (format!("log_{}({})", base, b), PREC_ATOM)
                        }
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

    /// Format as infix, falling back to postfix when the expression is malformed.
    ///
    /// Prefer this over [`to_infix`](Self::to_infix) at FFI and other untrusted-output
    /// boundaries where a panic would be unacceptable.
    pub fn to_infix_or_postfix(&self) -> String {
        self.try_to_infix().unwrap_or_else(|_| self.to_postfix())
    }

    /// Convert to infix notation using a symbol table for display names
    ///
    /// This is the table-driven version that uses per-run configuration
    /// instead of global overrides for symbol display names.
    pub fn to_infix_with_table(&self, table: &crate::symbol_table::SymbolTable) -> String {
        const PREC_ATOM: u8 = 100; // Constants, x, function calls
        const PREC_POWER: u8 = 9; // ^ (right-associative)
        const PREC_UNARY: u8 = 8; // Unary minus, reciprocal
        const PREC_MUL: u8 = 6; // *, /
        const PREC_ADD: u8 = 4; // +, -

        let mut stack: Vec<(String, u8)> = Vec::new();

        let mut index = 0;
        while index < self.symbols.len() {
            if let Some(literal) = self.take_decimal_literal(&mut index) {
                stack.push((literal, PREC_ATOM));
                continue;
            }
            let sym = self.symbols[index];
            index += 1;
            match sym.seft() {
                Seft::A => {
                    stack.push((table.name(sym).to_string(), PREC_ATOM));
                }
                Seft::B => {
                    let (arg, arg_prec) = stack
                        .pop()
                        .expect("stack underflow in to_infix: expression is not valid postfix");
                    let (result, prec) = match sym {
                        Symbol::Neg => {
                            let arg_s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, false, false);
                            (format!("-{}", arg_s), PREC_UNARY)
                        }
                        Symbol::Recip => {
                            let arg_s =
                                infix_maybe_paren(&arg, arg_prec, PREC_MUL + 1, false, true);
                            (format!("1/{}", arg_s), PREC_MUL)
                        }
                        Symbol::Sqrt => (format!("sqrt({})", arg), PREC_ATOM),
                        Symbol::Square => {
                            let arg_s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, true, false);
                            (format!("{}^2", arg_s), PREC_POWER)
                        }
                        Symbol::Ln => (format!("ln({})", arg), PREC_ATOM),
                        Symbol::Exp => {
                            let arg_s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, true, true);
                            (format!("e^{}", arg_s), PREC_POWER)
                        }
                        Symbol::SinPi => (format!("sinpi({})", arg), PREC_ATOM),
                        Symbol::CosPi => (format!("cospi({})", arg), PREC_ATOM),
                        Symbol::TanPi => (format!("tanpi({})", arg), PREC_ATOM),
                        Symbol::LambertW => (format!("W({})", arg), PREC_ATOM),
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
                        | Symbol::UserFunction15 => {
                            (format!("{}({})", table.name(sym), arg), PREC_ATOM)
                        }
                        _ => ("?".to_string(), PREC_ATOM),
                    };
                    stack.push((result, prec));
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
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL, false, true);
                            if a_s.chars().last().is_some_and(|c| c.is_ascii_digit())
                                && b_s.chars().next().is_some_and(|c| c.is_alphabetic())
                            {
                                (format!("{} {}", a_s, b_s), PREC_MUL)
                            } else {
                                (format!("{}*{}", a_s, b_s), PREC_MUL)
                            }
                        }
                        Symbol::Div => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL + 1, false, true);
                            (format!("{}/{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Pow => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_POWER, true, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_POWER, true, true);
                            (format!("{}^{}", a_s, b_s), PREC_POWER)
                        }
                        Symbol::Root => {
                            let index = infix_paren_unless_primary(&a, a_prec);
                            let radicand = infix_paren_unless_primary(&b, b_prec);
                            (format!("{}\"/{}", index, radicand), PREC_POWER)
                        }
                        Symbol::Log => {
                            let base = infix_group_log_base(&a, a_prec);
                            (format!("log_{}({})", base, b), PREC_ATOM)
                        }
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

        let mut stack: Vec<(String, u8)> = Vec::new();

        let mut index = 0;
        while index < self.symbols.len() {
            if let Some(literal) = self.take_decimal_literal(&mut index) {
                stack.push((literal, PREC_ATOM));
                continue;
            }
            let sym = self.symbols[index];
            index += 1;
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
                    let grouped = infix_paren_unless_primary(&arg, arg_prec);
                    let (result, prec) = match sym {
                        Symbol::Neg => {
                            let s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, false, false);
                            (format!("-{}", s), PREC_UNARY)
                        }
                        Symbol::Recip => {
                            let s = infix_maybe_paren(&arg, arg_prec, PREC_MUL + 1, false, true);
                            (format!("1/{}", s), PREC_MUL)
                        }
                        Symbol::Sqrt => (format!("Sqrt[{}]", arg), PREC_ATOM),
                        Symbol::Square => {
                            let s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, true, false);
                            (format!("{}^2", s), PREC_POWER)
                        }
                        Symbol::Ln => (format!("Log[{}]", arg), PREC_ATOM),
                        Symbol::Exp => (format!("Exp[{}]", arg), PREC_ATOM),
                        Symbol::SinPi => (format!("Sin[Pi*{}]", grouped), PREC_ATOM),
                        Symbol::CosPi => (format!("Cos[Pi*{}]", grouped), PREC_ATOM),
                        Symbol::TanPi => (format!("Tan[Pi*{}]", grouped), PREC_ATOM),
                        Symbol::LambertW => (format!("ProductLog[{}]", arg), PREC_ATOM),
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
                        | Symbol::UserFunction15 => {
                            (format!("{}[{}]", sym.display_name(), arg), PREC_ATOM)
                        }
                        _ => ("?".to_string(), PREC_ATOM),
                    };
                    stack.push((result, prec));
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
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL, false, true);
                            (format!("{}*{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Div => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL + 1, false, true);
                            (format!("{}/{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Pow => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_POWER, true, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_POWER, true, true);
                            (format!("{}^{}", a_s, b_s), PREC_POWER)
                        }
                        Symbol::Root => {
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_POWER, true, false);
                            let a_s = infix_paren_unless_primary(&a, a_prec);
                            (format!("{}^(1/{})", b_s, a_s), PREC_POWER)
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

        let mut stack: Vec<(String, u8)> = Vec::new();

        let mut index = 0;
        while index < self.symbols.len() {
            if let Some(literal) = self.take_decimal_literal(&mut index) {
                stack.push((literal, PREC_ATOM));
                continue;
            }
            let sym = self.symbols[index];
            index += 1;
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
                    let grouped = infix_paren_unless_primary(&arg, arg_prec);
                    let (result, prec) = match sym {
                        Symbol::Neg => {
                            let s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, false, false);
                            (format!("-{}", s), PREC_UNARY)
                        }
                        Symbol::Recip => {
                            let s = infix_maybe_paren(&arg, arg_prec, PREC_MUL + 1, false, true);
                            (format!("1/{}", s), PREC_MUL)
                        }
                        Symbol::Sqrt => (format!("sqrt({})", arg), PREC_ATOM),
                        Symbol::Square => {
                            let s = infix_maybe_paren(&arg, arg_prec, PREC_POWER, true, false);
                            (format!("{}**2", s), PREC_POWER)
                        }
                        Symbol::Ln => (format!("log({})", arg), PREC_ATOM),
                        Symbol::Exp => (format!("exp({})", arg), PREC_ATOM),
                        Symbol::SinPi => (format!("sin(pi*{})", grouped), PREC_ATOM),
                        Symbol::CosPi => (format!("cos(pi*{})", grouped), PREC_ATOM),
                        Symbol::TanPi => (format!("tan(pi*{})", grouped), PREC_ATOM),
                        Symbol::LambertW => (format!("lambertw({})", arg), PREC_ATOM),
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
                        | Symbol::UserFunction15 => {
                            (format!("{}({})", sym.display_name(), arg), PREC_ATOM)
                        }
                        _ => ("?".to_string(), PREC_ATOM),
                    };
                    stack.push((result, prec));
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
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}+{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Sub => {
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_ADD, false, true);
                            (format!("{}-{}", a, b_s), PREC_ADD)
                        }
                        Symbol::Mul => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL, false, true);
                            (format!("{}*{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Div => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_MUL, false, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_MUL + 1, false, true);
                            (format!("{}/{}", a_s, b_s), PREC_MUL)
                        }
                        Symbol::Pow => {
                            let a_s = infix_maybe_paren(&a, a_prec, PREC_POWER, true, false);
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_POWER, true, true);
                            (format!("{}**{}", a_s, b_s), PREC_POWER)
                        }
                        Symbol::Root => {
                            let b_s = infix_maybe_paren(&b, b_prec, PREC_POWER, true, false);
                            let a_s = infix_paren_unless_primary(&a, a_prec);
                            (format!("{}**(1/{})", b_s, a_s), PREC_POWER)
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

impl Clone for Expression {
    fn clone(&self) -> Self {
        Self {
            symbols: self.symbols.clone(),
            complexity: self.complexity,
            contains_x: self.contains_x,
            literal_id: literal_store::clone_id(self.literal_id),
        }
    }
}

impl Drop for Expression {
    fn drop(&mut self) {
        literal_store::release(self.literal_id);
    }
}

impl PartialEq for Expression {
    fn eq(&self, other: &Self) -> bool {
        self.symbols == other.symbols
            && self.complexity == other.complexity
            && self.contains_x == other.contains_x
            && literal_store::same_spans(self.literal_id, other.literal_id)
    }
}

impl Eq for Expression {}

impl Hash for Expression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Match the field hash of an expression with no integer annotation so
        // search dedup keys stay identical to a build that has no side table.
        self.symbols.hash(state);
        self.complexity.hash(state);
        self.contains_x.hash(state);
        if self.literal_id != 0 {
            literal_store::hash_spans(self.literal_id, state);
        }
    }
}

impl Default for Expression {
    fn default() -> Self {
        Self::new()
    }
}

/// Exact integer spans for solver-built expressions.
///
/// Search expressions use id 0 and never lock. Ids are reference-counted so a
/// cloned closed form keeps its value after the original is dropped, including
/// across threads (reporting can run off the search pool).
mod literal_store {
    use std::hash::{Hash, Hasher};
    use std::sync::Mutex;

    struct Slot {
        refs: u32,
        spans: Vec<(u32, u32, u64)>,
    }

    struct Store {
        /// Index 0 is unused so a zero id means "no annotation".
        entries: Vec<Option<Slot>>,
        free: Vec<u16>,
    }

    impl Store {
        fn new() -> Self {
            Self {
                entries: vec![None],
                free: Vec::new(),
            }
        }

        fn alloc(&mut self, spans: Vec<(u32, u32, u64)>) -> u16 {
            let slot = Slot { refs: 1, spans };
            if let Some(id) = self.free.pop() {
                self.entries[id as usize] = Some(slot);
                return id;
            }
            let id = self.entries.len();
            assert!(
                id <= u16::MAX as usize,
                "exact integer literal table exhausted"
            );
            self.entries.push(Some(slot));
            id as u16
        }

        fn release_locked(&mut self, id: u16) {
            if id == 0 {
                return;
            }
            let slot = self.entries[id as usize]
                .as_mut()
                .expect("literal id released twice");
            slot.refs -= 1;
            if slot.refs == 0 {
                self.entries[id as usize] = None;
                self.free.push(id);
            }
        }

        /// Unique slot the caller may mutate. Shared slots are copied first.
        fn detach(&mut self, id: u16) -> u16 {
            if id == 0 {
                return self.alloc(Vec::new());
            }
            let refs = self.entries[id as usize]
                .as_ref()
                .expect("literal id missing")
                .refs;
            if refs == 1 {
                return id;
            }
            let spans = self.entries[id as usize]
                .as_ref()
                .expect("literal id missing")
                .spans
                .clone();
            self.release_locked(id);
            self.alloc(spans)
        }
    }

    fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
        static STORE: Mutex<Option<Store>> = Mutex::new(None);
        let mut guard = STORE.lock().unwrap_or_else(|err| err.into_inner());
        if guard.is_none() {
            *guard = Some(Store::new());
        }
        f(guard.as_mut().expect("literal store"))
    }

    #[inline(always)]
    pub(super) fn clone_id(id: u16) -> u16 {
        if id == 0 {
            return 0;
        }
        with_store(|store| {
            store.entries[id as usize]
                .as_mut()
                .expect("literal id missing")
                .refs += 1;
            id
        })
    }

    #[inline(always)]
    pub(super) fn release(id: u16) {
        if id == 0 {
            return;
        }
        with_store(|store| store.release_locked(id));
    }

    pub(super) fn push_span(id: u16, span: (u32, u32, u64)) -> u16 {
        with_store(|store| {
            let id = store.detach(id);
            store.entries[id as usize]
                .as_mut()
                .expect("literal id missing")
                .spans
                .push(span);
            id
        })
    }

    pub(super) fn append_spans(self_id: u16, other_id: u16, shift: u32) -> u16 {
        if other_id == 0 {
            return self_id;
        }
        with_store(|store| {
            let extra: Vec<(u32, u32, u64)> = store.entries[other_id as usize]
                .as_ref()
                .expect("literal id missing")
                .spans
                .iter()
                .map(|&(start, len, bits)| (start.saturating_add(shift), len, bits))
                .collect();
            let mut id = self_id;
            for span in extra {
                id = store.detach(id);
                store.entries[id as usize]
                    .as_mut()
                    .expect("literal id missing")
                    .spans
                    .push(span);
            }
            id
        })
    }

    pub(super) fn retain_spans(id: u16, n: u32) -> u16 {
        if id == 0 {
            return 0;
        }
        with_store(|store| {
            let id = store.detach(id);
            let spans = &mut store.entries[id as usize]
                .as_mut()
                .expect("literal id missing")
                .spans;
            spans.retain(|(start, len, _)| start.saturating_add(*len) <= n);
            if spans.is_empty() {
                store.release_locked(id);
                0
            } else {
                id
            }
        })
    }

    pub(super) fn span_at(id: u16, index: usize) -> Option<(usize, f64)> {
        if id == 0 {
            return None;
        }
        with_store(|store| {
            store.entries[id as usize]
                .as_ref()
                .expect("literal id missing")
                .spans
                .iter()
                .find_map(|&(start, len, bits)| {
                    (start as usize == index && len > 0)
                        .then_some((len as usize, f64::from_bits(bits)))
                })
        })
    }

    pub(super) fn same_spans(left: u16, right: u16) -> bool {
        if left == right {
            return true;
        }
        if left == 0 || right == 0 {
            return false;
        }
        with_store(|store| {
            let a = &store.entries[left as usize]
                .as_ref()
                .expect("literal id missing")
                .spans;
            let b = &store.entries[right as usize]
                .as_ref()
                .expect("literal id missing")
                .spans;
            a == b
        })
    }

    pub(super) fn hash_spans<H: Hasher>(id: u16, state: &mut H) {
        if id == 0 {
            return;
        }
        with_store(|store| {
            store.entries[id as usize]
                .as_ref()
                .expect("literal id missing")
                .spans
                .hash(state);
        });
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
    fn search_expression_stays_the_size_of_its_symbol_buffer() {
        // The integer-annotation id sits in the padding after `contains_x`.
        // Growing past this makes every search clone and the RHS value scan
        // touch extra bytes.
        let symbols = std::mem::size_of::<SmallVec<[Symbol; MAX_EXPR_LEN]>>();
        let expr = std::mem::size_of::<Expression>();
        assert!(
            expr <= symbols + 8,
            "Expression is {expr} bytes, symbol buffer is {symbols}"
        );
        assert_eq!(expr, 40, "search Expression grew to {expr} bytes");
    }

    #[test]
    fn decimal_literal_survives_clone_and_drop() {
        let mut expr = Expression::parse("2").unwrap();
        let big = 2.0_f64.powi(63);
        expr.mark_decimal_literal(big);
        let cloned = expr.clone();
        drop(expr);
        assert_eq!(cloned.decimal_literal_at(0), Some((1, big)));
        let again = cloned.clone();
        assert_eq!(again, cloned);
        assert_eq!(again.to_infix(), "9223372036854775808");
    }

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
    fn test_format_exact_f64_integer_digits() {
        assert_eq!(format_exact_f64_integer(0.0), "0");
        assert_eq!(format_exact_f64_integer(2000002.0), "2000002");
        assert_eq!(format_exact_f64_integer(-12.0), "-12");
        // Independent of the digit-shift formatter: primitive integers.
        for exp in [1_u32, 10, 53, 58, 59, 60, 61, 62, 63, 64, 100, 127] {
            let text = format_exact_f64_integer(2.0_f64.powi(exp as i32));
            let reference = (1_u128 << exp).to_string();
            assert_eq!(text, reference, "2^{exp}");
        }
        let huge = format_exact_f64_integer(2.0_f64.powi(1023));
        assert_eq!(huge.parse::<f64>().unwrap(), 2.0_f64.powi(1023));
        assert_eq!(huge.len(), 308);
        let mut mod_billion = 1u64;
        for _ in 0..1023 {
            mod_billion = (mod_billion * 2) % 1_000_000_000;
        }
        let tail: u64 = huge[huge.len() - 9..].parse().unwrap();
        assert_eq!(tail, mod_billion);
        let max_digits = format_exact_f64_integer(f64::MAX);
        assert_eq!(max_digits.parse::<f64>().unwrap(), f64::MAX);
        assert!(max_digits.bytes().all(|b| b.is_ascii_digit()));
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
    fn test_to_infix_or_postfix_falls_back_on_malformed_expression() {
        let expr = Expression::from_symbols(&[Symbol::Add]);
        assert_eq!(expr.to_infix_or_postfix(), expr.to_postfix());
    }

    #[test]
    fn test_try_to_infix_succeeds_on_valid_expression() {
        let expr = Expression::parse("32+").unwrap();
        let result = expr.try_to_infix();
        assert_eq!(result.unwrap(), "3+2");
    }

    #[test]
    fn test_reciprocal_chain_keeps_grouping() {
        // -(1/(1/(-phi))), previously flattened to `-1/1/-phi`.
        assert_eq!(
            Expression::parse("fnrrn").unwrap().to_infix(),
            "-(1/(1/(-phi)))"
        );
    }

    #[test]
    fn test_subtraction_is_left_associative_in_print() {
        assert_eq!(Expression::parse("12-3-").unwrap().to_infix(), "1-2-3");
        assert_eq!(Expression::parse("123--").unwrap().to_infix(), "1-(2-3)");
        assert_eq!(Expression::parse("123+-").unwrap().to_infix(), "1-(2+3)");
    }

    #[test]
    fn test_power_is_right_associative_in_print() {
        assert_eq!(Expression::parse("x23^^").unwrap().to_infix(), "x^2^3");
        assert_eq!(Expression::parse("x2^3^").unwrap().to_infix(), "(x^2)^3");
        assert_eq!(Expression::parse("xss").unwrap().to_infix(), "(x^2)^2");
        assert_eq!(Expression::parse("xsn").unwrap().to_infix(), "-x^2");
        assert_eq!(Expression::parse("xns").unwrap().to_infix(), "(-x)^2");
    }

    #[test]
    fn test_division_of_division_keeps_grouping() {
        assert_eq!(Expression::parse("xrr").unwrap().to_infix(), "1/(1/x)");
        assert_eq!(Expression::parse("xrs").unwrap().to_infix(), "(1/x)^2");
        assert_eq!(Expression::parse("xsr").unwrap().to_infix(), "1/x^2");
    }
}
