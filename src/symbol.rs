//! Symbol definitions for RIES expressions
//!
//! Symbols represent constants, variables, and operators in postfix notation.

use std::fmt;

/// Stack effect type - how many values a symbol pops and pushes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Seft {
    /// Constants and variables: push 1 value (pop 0)
    A,
    /// Unary operators: pop 1, push 1
    B,
    /// Binary operators: pop 2, push 1
    C,
}

/// Number type classification for algebraic properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum NumType {
    /// Transcendental (e.g., e^π)
    Transcendental = 0,
    /// Liouvillian (closed under exp, ln, and algebraic operations)
    Liouvillian = 1,
    /// Elementary (between algebraic and Liouvillian)
    Elementary = 2,
    /// Algebraic (roots of polynomials with rational coefficients)
    Algebraic = 3,
    /// Constructible (compass and straightedge)
    Constructible = 4,
    /// Rational
    Rational = 5,
    /// Integer
    Integer = 6,
}

impl NumType {
    /// Combine two types - result is the "weaker" (more general) type
    #[inline]
    pub fn combine(self, other: Self) -> Self {
        std::cmp::min(self, other)
    }

    /// Check if this type is at least as strong as the given type
    #[inline]
    pub fn is_at_least(self, required: Self) -> bool {
        self >= required
    }
}

/// A symbol in a RIES expression
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Symbol {
    // === Constants (Seft::A) ===
    One = b'1',
    Two = b'2',
    Three = b'3',
    Four = b'4',
    Five = b'5',
    Six = b'6',
    Seven = b'7',
    Eight = b'8',
    Nine = b'9',
    Pi = b'p',
    E = b'e',
    Phi = b'f',
    X = b'x',

    // === Unary operators (Seft::B) ===
    Neg = b'n',
    Recip = b'r',
    Sqrt = b'q',
    Square = b's',
    Ln = b'l',
    Exp = b'E',
    SinPi = b'S',
    CosPi = b'C',
    TanPi = b'T',
    LambertW = b'W',

    // === Binary operators (Seft::C) ===
    Add = b'+',
    Sub = b'-',
    Mul = b'*',
    Div = b'/',
    Pow = b'^',
    Root = b'v',  // a-th root of b
    Log = b'L',   // log base a of b
    Atan2 = b'A',
}

impl Symbol {
    /// Get the stack effect type of this symbol
    #[inline]
    pub const fn seft(self) -> Seft {
        use Symbol::*;
        match self {
            One | Two | Three | Four | Five | Six | Seven | Eight | Nine |
            Pi | E | Phi | X => Seft::A,

            Neg | Recip | Sqrt | Square | Ln | Exp | SinPi | CosPi | TanPi | LambertW => Seft::B,

            Add | Sub | Mul | Div | Pow | Root | Log | Atan2 => Seft::C,
        }
    }

    /// Get the default complexity weight of this symbol
    #[inline]
    pub const fn weight(self) -> u16 {
        use Symbol::*;
        match self {
            One => 10,
            Two => 13,
            Three => 15,
            Four => 16,
            Five => 17,
            Six => 18,
            Seven => 18,
            Eight => 19,
            Nine => 19,
            Pi => 14,
            E => 16,
            Phi => 18,
            X => 15,

            Neg => 7,
            Recip => 7,
            Sqrt => 9,
            Square => 9,
            Ln => 13,
            Exp => 13,
            SinPi => 13,
            CosPi => 13,
            TanPi => 16,
            LambertW => 15,

            Add => 4,
            Sub => 5,
            Mul => 4,
            Div => 5,
            Pow => 6,
            Root => 7,
            Log => 9,
            Atan2 => 9,
        }
    }

    /// Get the result type when this operation is applied
    pub fn result_type(self, arg_types: &[NumType]) -> NumType {
        use Symbol::*;
        use NumType::*;

        match self {
            // Integer constants
            One | Two | Three | Four | Five | Six | Seven | Eight | Nine => Integer,

            // Transcendental constants
            Pi | E => Transcendental,

            // Algebraic constant
            Phi => Algebraic,

            // Variable inherits from context
            X => Transcendental,

            // Operations that preserve integer-ness
            Neg | Add | Sub | Mul => {
                if arg_types.iter().all(|t| *t == Integer) {
                    Integer
                } else if arg_types.iter().all(|t| t.is_at_least(Rational)) {
                    arg_types.iter().copied().fold(Integer, NumType::combine)
                } else {
                    arg_types.iter().copied().fold(Integer, NumType::combine)
                }
            }

            // Division: integer -> rational
            Div | Recip => {
                let base = arg_types.iter().copied().fold(Integer, NumType::combine);
                if base == Integer { Rational } else { base }
            }

            // Square root: rational -> constructible (or algebraic)
            Sqrt => {
                let base = arg_types.iter().copied().fold(Integer, NumType::combine);
                if base.is_at_least(Constructible) {
                    Constructible
                } else if base.is_at_least(Algebraic) {
                    Algebraic
                } else {
                    base
                }
            }

            // Square preserves type
            Square => arg_types.iter().copied().fold(Integer, NumType::combine),

            // Nth root: generally algebraic
            Root => Algebraic,

            // Power: depends on exponent
            Pow => {
                // If exponent is integer, preserves algebraic-ness
                // Otherwise, generally transcendental
                if arg_types.len() >= 2 && arg_types[0] == Integer {
                    arg_types[1]
                } else {
                    Transcendental
                }
            }

            // Transcendental functions
            Ln | Exp | SinPi | CosPi | TanPi | Log | LambertW | Atan2 => Transcendental,
        }
    }

    /// Get the infix name for display
    pub const fn name(self) -> &'static str {
        use Symbol::*;
        match self {
            One => "1", Two => "2", Three => "3", Four => "4", Five => "5",
            Six => "6", Seven => "7", Eight => "8", Nine => "9",
            Pi => "pi", E => "e", Phi => "phi", X => "x",
            Neg => "-", Recip => "1/", Sqrt => "sqrt", Square => "^2",
            Ln => "ln", Exp => "e^", SinPi => "sinpi", CosPi => "cospi",
            TanPi => "tanpi", LambertW => "W",
            Add => "+", Sub => "-", Mul => "*", Div => "/",
            Pow => "^", Root => "\"/", Log => "log_", Atan2 => "atan2",
        }
    }

    /// Parse a symbol from its byte representation
    pub fn from_byte(b: u8) -> Option<Self> {
        use Symbol::*;
        Some(match b {
            b'1' => One, b'2' => Two, b'3' => Three, b'4' => Four, b'5' => Five,
            b'6' => Six, b'7' => Seven, b'8' => Eight, b'9' => Nine,
            b'p' => Pi, b'e' => E, b'f' => Phi, b'x' => X,
            b'n' => Neg, b'r' => Recip, b'q' => Sqrt, b's' => Square,
            b'l' => Ln, b'E' => Exp, b'S' => SinPi, b'C' => CosPi,
            b'T' => TanPi, b'W' => LambertW,
            b'+' => Add, b'-' => Sub, b'*' => Mul, b'/' => Div,
            b'^' => Pow, b'v' => Root, b'L' => Log, b'A' => Atan2,
            _ => return None,
        })
    }

    /// Get all constant symbols (Seft::A)
    pub fn constants() -> &'static [Symbol] {
        use Symbol::*;
        &[One, Two, Three, Four, Five, Six, Seven, Eight, Nine, Pi, E, Phi]
    }

    /// Get all unary operators (Seft::B)
    pub fn unary_ops() -> &'static [Symbol] {
        use Symbol::*;
        &[Neg, Recip, Sqrt, Square, Ln, Exp, SinPi, CosPi, TanPi]
    }

    /// Get all binary operators (Seft::C)
    pub fn binary_ops() -> &'static [Symbol] {
        use Symbol::*;
        &[Add, Sub, Mul, Div, Pow, Root, Log, Atan2]
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl From<Symbol> for u8 {
    fn from(s: Symbol) -> u8 {
        s as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_roundtrip() {
        for &sym in Symbol::constants().iter()
            .chain(Symbol::unary_ops())
            .chain(Symbol::binary_ops())
        {
            let byte = sym as u8;
            let parsed = Symbol::from_byte(byte).unwrap();
            assert_eq!(sym, parsed);
        }
    }

    #[test]
    fn test_num_type_ordering() {
        assert!(NumType::Integer > NumType::Rational);
        assert!(NumType::Rational > NumType::Algebraic);
        assert!(NumType::Algebraic > NumType::Transcendental);
    }

    #[test]
    fn test_seft() {
        assert_eq!(Symbol::Pi.seft(), Seft::A);
        assert_eq!(Symbol::Sqrt.seft(), Seft::B);
        assert_eq!(Symbol::Add.seft(), Seft::C);
    }
}
