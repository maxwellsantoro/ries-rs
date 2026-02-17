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
    /// Euler-Mascheroni constant γ ≈ 0.5772156649
    Gamma = b'g',
    /// Plastic constant ρ ≈ 1.3247179572
    Plastic = b'P',
    /// Apéry's constant ζ(3) ≈ 1.2020569032
    Apery = b'z',
    /// Catalan's constant G ≈ 0.9159655942
    Catalan = b'G',
    X = b'x',

    // === User-defined constant slots (reserved byte range 128-143) ===
    // These are accessed via UserConstant0, UserConstant1, etc.
    // The actual values are stored in the profile/symbol table
    UserConstant0 = 128,
    UserConstant1 = 129,
    UserConstant2 = 130,
    UserConstant3 = 131,
    UserConstant4 = 132,
    UserConstant5 = 133,
    UserConstant6 = 134,
    UserConstant7 = 135,
    UserConstant8 = 136,
    UserConstant9 = 137,
    UserConstant10 = 138,
    UserConstant11 = 139,
    UserConstant12 = 140,
    UserConstant13 = 141,
    UserConstant14 = 142,
    UserConstant15 = 143,

    // === User-defined function slots (reserved byte range 144-159) ===
    // These act as unary operators that expand to their defined body
    UserFunction0 = 144,
    UserFunction1 = 145,
    UserFunction2 = 146,
    UserFunction3 = 147,
    UserFunction4 = 148,
    UserFunction5 = 149,
    UserFunction6 = 150,
    UserFunction7 = 151,
    UserFunction8 = 152,
    UserFunction9 = 153,
    UserFunction10 = 154,
    UserFunction11 = 155,
    UserFunction12 = 156,
    UserFunction13 = 157,
    UserFunction14 = 158,
    UserFunction15 = 159,

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
    Root = b'v', // a-th root of b
    Log = b'L',  // log base a of b
    Atan2 = b'A',
}

impl Symbol {
    /// Get the stack effect type of this symbol
    #[inline]
    pub const fn seft(self) -> Seft {
        use Symbol::*;
        match self {
            One | Two | Three | Four | Five | Six | Seven | Eight | Nine | Pi | E | Phi | Gamma
            | Plastic | Apery | Catalan | X | UserConstant0 | UserConstant1 | UserConstant2
            | UserConstant3 | UserConstant4 | UserConstant5 | UserConstant6 | UserConstant7
            | UserConstant8 | UserConstant9 | UserConstant10 | UserConstant11 | UserConstant12
            | UserConstant13 | UserConstant14 | UserConstant15 => Seft::A,

            Neg | Recip | Sqrt | Square | Ln | Exp | SinPi | CosPi | TanPi | LambertW
            | UserFunction0 | UserFunction1 | UserFunction2 | UserFunction3 | UserFunction4
            | UserFunction5 | UserFunction6 | UserFunction7 | UserFunction8 | UserFunction9
            | UserFunction10 | UserFunction11 | UserFunction12 | UserFunction13
            | UserFunction14 | UserFunction15 => Seft::B,

            Add | Sub | Mul | Div | Pow | Root | Log | Atan2 => Seft::C,
        }
    }

    /// Get the default complexity weight of this symbol
    ///
    /// Complexity weights determine how "simple" an expression is, affecting
    /// which equations RIES presents first. Lower complexity = simpler expression.
    ///
    /// # Calibration Methodology
    ///
    /// Weights are calibrated to match original RIES behavior while ensuring
    /// intuitive simplicity ordering:
    ///
    /// ## Constants
    /// - **Small integers (1-9)**: Range from 3-6, with smaller digits cheaper
    ///   - Rationale: Single digits are fundamental building blocks
    ///   - `1` and `2` are cheapest (3) as they appear in most simple equations
    ///   - Larger digits cost more as they're less "fundamental"
    ///
    /// - **Transcendental constants (π, e)**: Weight 8
    ///   - Higher than integers as they require special notation
    ///   - Same weight as they're equally "fundamental" in mathematics
    ///
    /// - **Algebraic constants (φ, ρ)**: Weight 10
    ///   - Higher than π/e as they're less commonly used
    ///   - Plastic constant (ρ) is algebraic (root of x³ = x + 1)
    ///
    /// - **Special constants (γ, ζ(3), G)**: Weight 10-12
    ///   - Euler-Mascheroni γ and Catalan's G: 10
    ///   - Apéry's constant ζ(3): 12 (higher due to obscurity)
    ///
    /// ## Unary Operators
    /// - **Negation (-)**: Weight 4 - simplest unary operation
    /// - **Reciprocal (1/x)**: Weight 5 - slightly more complex
    /// - **Square (x²)**: Weight 5 - very common, moderate cost
    /// - **Square root (√)**: Weight 6 - inverse of square
    /// - **Logarithm (ln)**: Weight 8 - transcendental operation
    /// - **Exponential (e^x)**: Weight 8 - inverse of ln, transcendental
    /// - **Trigonometric (sin(πx), cos(πx))**: Weight 9-10 - periodic complexity
    /// - **Lambert W**: Weight 12 - most complex, rarely used
    ///
    /// ## Binary Operators
    /// - **Addition/Subtraction (+, -)**: Weight 3 - simplest operations
    /// - **Multiplication (*)**: Weight 3 - fundamental arithmetic
    /// - **Division (/)**: Weight 4 - slightly more complex than multiply
    /// - **Power (^)**: Weight 5 - exponentiation
    /// - **Root (ᵃ√b)**: Weight 6 - inverse of power, more notation
    /// - **Logarithm base (log_a b)**: Weight 7 - two transcendental ops
    /// - **Atan2**: Weight 7 - two-argument inverse trig
    ///
    /// # Example Weight Calculations
    ///
    /// ```text
    /// Expression    Postfix    Weight Calculation          Total
    /// x = 2         x2=        6(x) + 3(2)                 9
    /// x² = 4        xs4=       6(x) + 5(s) + 4(4)          15
    /// 2x = 5        2x*5=      3(2) + 6(x) + 3(*) + 5(5)   17
    /// e^x = π       xEep       6(x) + 8(E) + 8(p)          22
    /// x^x = π²      xx^ps      6+6+5+8+5                   30
    /// ```
    ///
    /// # Design Philosophy
    ///
    /// The weight system follows these principles:
    ///
    /// 1. **Pedagogical value**: Simpler concepts have lower weights
    /// 2. **Historical consistency**: Weights approximate original RIES behavior
    /// 3. **Practical usage**: Commonly-used operations are cheaper
    /// 4. **Composability**: Complex expressions = sum of symbol weights
    ///
    /// # See Also
    ///
    /// For a detailed explanation of the calibration process and rationale,
    /// see `docs/COMPLEXITY.md` in the source repository.
    #[inline]
    pub const fn weight(self) -> u32 {
        use Symbol::*;
        match self {
            // Small integers are cheap - they're fundamental building blocks
            // Original RIES treats these as nearly free
            One => 3,
            Two => 3,
            Three => 4,
            Four => 4,
            Five => 5,
            Six => 5,
            Seven => 6,
            Eight => 6,
            Nine => 6,

            // Transcendental/algebraic constants cost more
            Pi => 8,
            E => 8,
            Phi => 10,
            // New constants - similar weights to other special constants
            Gamma => 10,   // Euler-Mascheroni γ
            Plastic => 10, // Plastic constant (algebraic)
            Apery => 12,   // Apéry's constant (unknown type, treated as transcendental)
            Catalan => 10, // Catalan's constant

            // Variable
            X => 6,

            // User constants have a default weight (can be customized via profile)
            UserConstant0 | UserConstant1 | UserConstant2 | UserConstant3 | UserConstant4
            | UserConstant5 | UserConstant6 | UserConstant7 | UserConstant8 | UserConstant9
            | UserConstant10 | UserConstant11 | UserConstant12 | UserConstant13
            | UserConstant14 | UserConstant15 => 8,

            // Unary operators
            Neg => 4,
            Recip => 5,
            Sqrt => 6,
            Square => 5,
            Ln => 8,
            Exp => 8,
            SinPi => 9,
            CosPi => 9,
            TanPi => 10,
            LambertW => 12,

            // User-defined functions have a default weight (can be customized via profile)
            UserFunction0 | UserFunction1 | UserFunction2 | UserFunction3 | UserFunction4
            | UserFunction5 | UserFunction6 | UserFunction7 | UserFunction8 | UserFunction9
            | UserFunction10 | UserFunction11 | UserFunction12 | UserFunction13
            | UserFunction14 | UserFunction15 => 8,

            // Binary operators
            Add => 3,
            Sub => 3,
            Mul => 3,
            Div => 4,
            Pow => 5,
            Root => 6,
            Log => 7,
            Atan2 => 7,
        }
    }

    /// Get the result type when this operation is applied
    pub fn result_type(self, arg_types: &[NumType]) -> NumType {
        use NumType::*;
        use Symbol::*;

        match self {
            // Integer constants
            One | Two | Three | Four | Five | Six | Seven | Eight | Nine => Integer,

            // Transcendental constants
            Pi | E => Transcendental,

            // Algebraic constant
            Phi => Algebraic,

            // New constants
            // Euler-Mascheroni γ is believed to be transcendental
            Gamma => Transcendental,
            // Plastic constant is algebraic (root of x³ = x + 1)
            Plastic => Algebraic,
            // Apéry's constant ζ(3) is irrational but type unknown
            Apery => Transcendental,
            // Catalan's constant is believed to be transcendental
            Catalan => Transcendental,

            // Variable inherits from context
            X => Transcendental,

            // User constants - assume transcendental (most general)
            UserConstant0 | UserConstant1 | UserConstant2 | UserConstant3 | UserConstant4
            | UserConstant5 | UserConstant6 | UserConstant7 | UserConstant8 | UserConstant9
            | UserConstant10 | UserConstant11 | UserConstant12 | UserConstant13
            | UserConstant14 | UserConstant15 => Transcendental,

            // Operations that preserve integer-ness
            Neg | Add | Sub | Mul => {
                if arg_types.iter().all(|t| *t == Integer) {
                    Integer
                } else {
                    arg_types.iter().copied().fold(Integer, NumType::combine)
                }
            }

            // Division: integer -> rational
            Div | Recip => {
                let base = arg_types.iter().copied().fold(Integer, NumType::combine);
                if base == Integer {
                    Rational
                } else {
                    base
                }
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

            // User-defined functions - assume transcendental (most general)
            UserFunction0 | UserFunction1 | UserFunction2 | UserFunction3 | UserFunction4
            | UserFunction5 | UserFunction6 | UserFunction7 | UserFunction8 | UserFunction9
            | UserFunction10 | UserFunction11 | UserFunction12 | UserFunction13
            | UserFunction14 | UserFunction15 => Transcendental,
        }
    }

    /// Get the inherent numeric type of this symbol (for constants)
    /// Returns Transcendental for operators (since they can produce any type)
    pub const fn inherent_type(self) -> NumType {
        use NumType::*;
        use Symbol::*;

        match self {
            // Integer constants
            One | Two | Three | Four | Five | Six | Seven | Eight | Nine => Integer,

            // Transcendental constants
            Pi | E => Transcendental,

            // Algebraic constant
            Phi => Algebraic,

            // New constants
            Gamma => Transcendental,
            Plastic => Algebraic,
            Apery => Transcendental,
            Catalan => Transcendental,

            // Variable
            X => Transcendental,

            // User constants - assume transcendental (most general)
            UserConstant0 | UserConstant1 | UserConstant2 | UserConstant3 | UserConstant4
            | UserConstant5 | UserConstant6 | UserConstant7 | UserConstant8 | UserConstant9
            | UserConstant10 | UserConstant11 | UserConstant12 | UserConstant13
            | UserConstant14 | UserConstant15 => Transcendental,

            // All operators default to Transcendental (most general)
            // The actual result type depends on operands
            _ => Transcendental,
        }
    }

    /// Get the infix name for display
    pub const fn name(self) -> &'static str {
        use Symbol::*;
        match self {
            One => "1",
            Two => "2",
            Three => "3",
            Four => "4",
            Five => "5",
            Six => "6",
            Seven => "7",
            Eight => "8",
            Nine => "9",
            Pi => "pi",
            E => "e",
            Phi => "phi",
            Gamma => "gamma",
            Plastic => "plastic",
            Apery => "apery",
            Catalan => "catalan",
            X => "x",
            Neg => "-",
            Recip => "1/",
            Sqrt => "sqrt",
            Square => "^2",
            Ln => "ln",
            Exp => "e^",
            SinPi => "sinpi",
            CosPi => "cospi",
            TanPi => "tanpi",
            LambertW => "W",
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Pow => "^",
            Root => "\"/",
            Log => "log_",
            Atan2 => "atan2",
            // User constants - placeholder names (can be overridden by profile)
            UserConstant0 => "u0",
            UserConstant1 => "u1",
            UserConstant2 => "u2",
            UserConstant3 => "u3",
            UserConstant4 => "u4",
            UserConstant5 => "u5",
            UserConstant6 => "u6",
            UserConstant7 => "u7",
            UserConstant8 => "u8",
            UserConstant9 => "u9",
            UserConstant10 => "u10",
            UserConstant11 => "u11",
            UserConstant12 => "u12",
            UserConstant13 => "u13",
            UserConstant14 => "u14",
            UserConstant15 => "u15",
            // User functions - placeholder names (can be overridden by profile)
            UserFunction0 => "f0",
            UserFunction1 => "f1",
            UserFunction2 => "f2",
            UserFunction3 => "f3",
            UserFunction4 => "f4",
            UserFunction5 => "f5",
            UserFunction6 => "f6",
            UserFunction7 => "f7",
            UserFunction8 => "f8",
            UserFunction9 => "f9",
            UserFunction10 => "f10",
            UserFunction11 => "f11",
            UserFunction12 => "f12",
            UserFunction13 => "f13",
            UserFunction14 => "f14",
            UserFunction15 => "f15",
        }
    }

    /// Parse a symbol from its byte representation
    pub fn from_byte(b: u8) -> Option<Self> {
        use Symbol::*;
        Some(match b {
            b'1' => One,
            b'2' => Two,
            b'3' => Three,
            b'4' => Four,
            b'5' => Five,
            b'6' => Six,
            b'7' => Seven,
            b'8' => Eight,
            b'9' => Nine,
            b'p' => Pi,
            b'e' => E,
            b'f' => Phi,
            b'x' => X,
            b'g' => Gamma,
            b'P' => Plastic,
            b'z' => Apery,
            b'G' => Catalan,
            b'n' => Neg,
            b'r' => Recip,
            b'q' => Sqrt,
            b's' => Square,
            b'l' => Ln,
            b'E' => Exp,
            b'S' => SinPi,
            b'C' => CosPi,
            b'T' => TanPi,
            b'W' => LambertW,
            b'+' => Add,
            b'-' => Sub,
            b'*' => Mul,
            b'/' => Div,
            b'^' => Pow,
            b'v' => Root,
            b'L' => Log,
            b'A' => Atan2,
            // User constants (byte range 128-143)
            128 => UserConstant0,
            129 => UserConstant1,
            130 => UserConstant2,
            131 => UserConstant3,
            132 => UserConstant4,
            133 => UserConstant5,
            134 => UserConstant6,
            135 => UserConstant7,
            136 => UserConstant8,
            137 => UserConstant9,
            138 => UserConstant10,
            139 => UserConstant11,
            140 => UserConstant12,
            141 => UserConstant13,
            142 => UserConstant14,
            143 => UserConstant15,
            // User functions (byte range 144-159)
            // Also support printable aliases for CLI use:
            // 'H'-'W' (skipping used ones) and 'Y', 'Z'
            144 => UserFunction0,
            145 => UserFunction1,
            146 => UserFunction2,
            147 => UserFunction3,
            148 => UserFunction4,
            149 => UserFunction5,
            150 => UserFunction6,
            151 => UserFunction7,
            152 => UserFunction8,
            153 => UserFunction9,
            154 => UserFunction10,
            155 => UserFunction11,
            156 => UserFunction12,
            157 => UserFunction13,
            158 => UserFunction14,
            159 => UserFunction15,
            // Printable aliases for user functions (for CLI expression parsing)
            // H=0, I=1, J=2, K=3, M=4, N=5, O=6, Q=7, R=8, U=9, V=10, Y=11, Z=12, B=13, D=14, F=15
            b'H' => UserFunction0,
            b'I' => UserFunction1,
            b'J' => UserFunction2,
            b'K' => UserFunction3,
            b'M' => UserFunction4,
            b'N' => UserFunction5,
            b'O' => UserFunction6,
            b'Q' => UserFunction7,
            b'R' => UserFunction8,
            b'U' => UserFunction9,
            b'V' => UserFunction10,
            b'Y' => UserFunction11,
            b'Z' => UserFunction12,
            b'B' => UserFunction13,
            b'D' => UserFunction14,
            b'F' => UserFunction15,
            _ => return None,
        })
    }

    /// Get user constant index (0-15) if this is a user constant symbol
    pub fn user_constant_index(self) -> Option<u8> {
        use Symbol::*;
        match self {
            UserConstant0 => Some(0),
            UserConstant1 => Some(1),
            UserConstant2 => Some(2),
            UserConstant3 => Some(3),
            UserConstant4 => Some(4),
            UserConstant5 => Some(5),
            UserConstant6 => Some(6),
            UserConstant7 => Some(7),
            UserConstant8 => Some(8),
            UserConstant9 => Some(9),
            UserConstant10 => Some(10),
            UserConstant11 => Some(11),
            UserConstant12 => Some(12),
            UserConstant13 => Some(13),
            UserConstant14 => Some(14),
            UserConstant15 => Some(15),
            _ => None,
        }
    }

    /// Get user function index (0-15) if this is a user function symbol
    pub fn user_function_index(self) -> Option<u8> {
        use Symbol::*;
        match self {
            UserFunction0 => Some(0),
            UserFunction1 => Some(1),
            UserFunction2 => Some(2),
            UserFunction3 => Some(3),
            UserFunction4 => Some(4),
            UserFunction5 => Some(5),
            UserFunction6 => Some(6),
            UserFunction7 => Some(7),
            UserFunction8 => Some(8),
            UserFunction9 => Some(9),
            UserFunction10 => Some(10),
            UserFunction11 => Some(11),
            UserFunction12 => Some(12),
            UserFunction13 => Some(13),
            UserFunction14 => Some(14),
            UserFunction15 => Some(15),
            _ => None,
        }
    }

    /// Get all constant symbols (Seft::A)
    pub fn constants() -> &'static [Symbol] {
        use Symbol::*;
        &[
            One, Two, Three, Four, Five, Six, Seven, Eight, Nine, Pi, E, Phi, Gamma, Plastic,
            Apery, Catalan,
        ]
    }

    /// Get all unary operators (Seft::B)
    pub fn unary_ops() -> &'static [Symbol] {
        use Symbol::*;
        &[
            Neg, Recip, Sqrt, Square, Ln, Exp, SinPi, CosPi, TanPi, LambertW,
        ]
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
        for &sym in Symbol::constants()
            .iter()
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
