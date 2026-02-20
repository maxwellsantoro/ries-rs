//! Fast-path exact match detection
//!
//! Before doing expensive expression generation, check if the target
//! is a simple exact match (like pi, e, sqrt(2), etc.) that can be
//! found instantly.

use crate::eval::evaluate;
use crate::expr::{EvaluatedExpr, Expression};
use crate::profile::UserConstant;
use crate::search::Match;
use crate::symbol::{NumType, Symbol};
use crate::symbol_table::SymbolTable;
use std::collections::HashSet;

/// Tolerance for exact match detection
const EXACT_TOLERANCE: f64 = 1e-14;

/// Build an expression from symbols using table-based weights
fn expr_from_symbols_with_table(symbols: &[Symbol], table: &SymbolTable) -> Expression {
    let mut expr = Expression::new();
    for &sym in symbols {
        expr.push_with_table(sym, table);
    }
    expr
}

/// Get the num_type of an expression based on its symbols
/// This is a simplified type inference for the fast_match candidates
fn get_num_type(symbols: &[Symbol]) -> NumType {
    // For fast match candidates, we use simplified type inference:
    // - Integer constants → Integer
    // - Rational operations (division) → Rational
    // - Sqrt of integer → Algebraic (constructible)
    // - Transcendental constants → Transcendental
    // - Any transcendental function → Transcendental

    use Symbol::*;

    // Handle simple patterns
    if symbols.len() == 1 {
        return symbols[0].inherent_type();
    }

    // Check for sqrt of integer (like 2q = sqrt(2))
    if symbols.len() == 2 {
        if matches!(symbols[1], Sqrt) {
            if matches!(
                symbols[0],
                One | Two | Three | Four | Five | Six | Seven | Eight | Nine
            ) {
                return NumType::Algebraic; // sqrt of integer is algebraic
            }
            // sqrt of transcendental constant is transcendental
            if matches!(symbols[0], Pi | E | Gamma | Apery | Catalan) {
                return NumType::Transcendental;
            }
        }
        // Check for reciprocal of integer
        if matches!(symbols[1], Recip)
            && matches!(
                symbols[0],
                One | Two | Three | Four | Five | Six | Seven | Eight | Nine
            )
        {
            return NumType::Rational;
        }
        // Division: integer / integer = rational
        if matches!(symbols[1], Div)
            && matches!(
                symbols[0],
                One | Two | Three | Four | Five | Six | Seven | Eight | Nine
            )
            && symbols.len() >= 3
        {
            // This is more complex, but for simple cases it's rational
            return NumType::Rational;
        }
    }

    // Check for division pattern (3 symbols: num, denom, /)
    if symbols.len() == 3 && matches!(symbols[2], Div) {
        // Integer / Integer = Rational
        if matches!(
            symbols[0],
            One | Two | Three | Four | Five | Six | Seven | Eight | Nine
        ) && matches!(
            symbols[1],
            One | Two | Three | Four | Five | Six | Seven | Eight | Nine
        ) {
            return NumType::Rational;
        }
    }

    // Default: check if any symbol is transcendental
    for &sym in symbols {
        let sym_type = sym.inherent_type();
        if sym_type == NumType::Transcendental {
            return NumType::Transcendental;
        }
    }

    // If we have any algebraic constants (phi, plastic), result is algebraic
    for &sym in symbols {
        if matches!(sym, Phi | Plastic) {
            return NumType::Algebraic;
        }
    }

    // Default to transcendental (most general)
    NumType::Transcendental
}

/// Check if any symbol in the expression is excluded
fn contains_excluded(symbols: &[Symbol], excluded: &HashSet<u8>) -> bool {
    symbols.iter().any(|s| excluded.contains(&(*s as u8)))
}

/// A candidate for a fast exact match
struct FastCandidate {
    /// The expression (as symbols)
    symbols: &'static [Symbol],
}

/// Generate fast candidates for common constants and simple expressions
fn get_constant_candidates() -> Vec<FastCandidate> {
    vec![
        // Integers
        FastCandidate {
            symbols: &[Symbol::One],
        },
        FastCandidate {
            symbols: &[Symbol::Two],
        },
        FastCandidate {
            symbols: &[Symbol::Three],
        },
        FastCandidate {
            symbols: &[Symbol::Four],
        },
        FastCandidate {
            symbols: &[Symbol::Five],
        },
        FastCandidate {
            symbols: &[Symbol::Six],
        },
        FastCandidate {
            symbols: &[Symbol::Seven],
        },
        FastCandidate {
            symbols: &[Symbol::Eight],
        },
        FastCandidate {
            symbols: &[Symbol::Nine],
        },
        // Named constants
        FastCandidate {
            symbols: &[Symbol::Pi],
        },
        FastCandidate {
            symbols: &[Symbol::E],
        },
        FastCandidate {
            symbols: &[Symbol::Phi],
        },
        FastCandidate {
            symbols: &[Symbol::Gamma],
        },
        FastCandidate {
            symbols: &[Symbol::Plastic],
        },
        FastCandidate {
            symbols: &[Symbol::Apery],
        },
        FastCandidate {
            symbols: &[Symbol::Catalan],
        },
        // Simple rationals (common ones)
        FastCandidate {
            symbols: &[Symbol::One, Symbol::Two, Symbol::Div],
        },
        FastCandidate {
            symbols: &[Symbol::One, Symbol::Three, Symbol::Div],
        },
        FastCandidate {
            symbols: &[Symbol::Two, Symbol::Three, Symbol::Div],
        },
        FastCandidate {
            symbols: &[Symbol::One, Symbol::Four, Symbol::Div],
        },
        FastCandidate {
            symbols: &[Symbol::Three, Symbol::Four, Symbol::Div],
        },
        // Simple roots
        FastCandidate {
            symbols: &[Symbol::Two, Symbol::Sqrt],
        },
        FastCandidate {
            symbols: &[Symbol::Three, Symbol::Sqrt],
        },
        FastCandidate {
            symbols: &[Symbol::Five, Symbol::Sqrt],
        },
        FastCandidate {
            symbols: &[Symbol::Six, Symbol::Sqrt],
        },
        FastCandidate {
            symbols: &[Symbol::Seven, Symbol::Sqrt],
        },
        FastCandidate {
            symbols: &[Symbol::Eight, Symbol::Sqrt],
        },
        FastCandidate {
            symbols: &[Symbol::Pi, Symbol::Sqrt],
        },
        FastCandidate {
            symbols: &[Symbol::E, Symbol::Sqrt],
        },
        // Simple logs
        FastCandidate {
            symbols: &[Symbol::Two, Symbol::Ln],
        },
        FastCandidate {
            symbols: &[Symbol::Pi, Symbol::Ln],
        },
        // e ± small integers
        FastCandidate {
            symbols: &[Symbol::E, Symbol::One, Symbol::Sub],
        },
        FastCandidate {
            symbols: &[Symbol::E, Symbol::One, Symbol::Add],
        },
        // pi ± small integers
        FastCandidate {
            symbols: &[Symbol::Pi, Symbol::One, Symbol::Sub],
        },
        FastCandidate {
            symbols: &[Symbol::Pi, Symbol::One, Symbol::Add],
        },
        FastCandidate {
            symbols: &[Symbol::Pi, Symbol::Two, Symbol::Sub],
        },
        // Common combinations
        FastCandidate {
            symbols: &[Symbol::One, Symbol::Two, Symbol::Add],
        },
        FastCandidate {
            symbols: &[Symbol::One, Symbol::Sqrt, Symbol::One, Symbol::Add],
        },
        FastCandidate {
            symbols: &[Symbol::Two, Symbol::Sqrt, Symbol::One, Symbol::Add],
        },
        // phi combinations (golden ratio)
        FastCandidate {
            symbols: &[Symbol::Phi, Symbol::One, Symbol::Add],
        },
        FastCandidate {
            symbols: &[Symbol::Phi, Symbol::Two, Symbol::Add],
        },
        FastCandidate {
            symbols: &[Symbol::Phi, Symbol::Square],
        },
        // Reciprocals of constants
        FastCandidate {
            symbols: &[Symbol::Pi, Symbol::Recip],
        },
        FastCandidate {
            symbols: &[Symbol::E, Symbol::Recip],
        },
        FastCandidate {
            symbols: &[Symbol::Phi, Symbol::Recip],
        },
    ]
}

/// Check if target matches a simple integer
fn check_integer(target: f64) -> Option<(i64, f64)> {
    let rounded = target.round();
    let error = (target - rounded).abs();
    if error < EXACT_TOLERANCE && rounded.abs() < 1000.0 {
        Some((rounded as i64, error))
    } else {
        None
    }
}

/// Configuration for fast match filtering
pub struct FastMatchConfig<'a> {
    /// Symbols that are excluded (via -N flag)
    pub excluded_symbols: &'a HashSet<u8>,
    /// Symbols that are explicitly allowed (all symbols must be in set)
    pub allowed_symbols: Option<&'a HashSet<u8>>,
    /// Minimum numeric type required (via -a, -r, -i flags)
    pub min_num_type: NumType,
}

#[inline]
fn passes_symbol_filters(symbols: &[Symbol], config: &FastMatchConfig<'_>) -> bool {
    if contains_excluded(symbols, config.excluded_symbols) {
        return false;
    }
    if let Some(allowed) = config.allowed_symbols {
        if symbols.iter().any(|s| !allowed.contains(&(*s as u8))) {
            return false;
        }
    }
    true
}

/// Try to find a fast exact match for the target value
///
/// Returns a Match if found, or None if no simple exact match exists.
/// This function is designed to be called before expensive generation.
pub fn find_fast_match(
    target: f64,
    user_constants: &[UserConstant],
    config: &FastMatchConfig<'_>,
    table: &SymbolTable,
) -> Option<Match> {
    // First check integers (fastest)
    if let Some((n, error)) = check_integer(target) {
        if (1..=9).contains(&n) {
            // We have a direct constant for 1-9
            let symbols: &[Symbol] = match n {
                1 => &[Symbol::One],
                2 => &[Symbol::Two],
                3 => &[Symbol::Three],
                4 => &[Symbol::Four],
                5 => &[Symbol::Five],
                6 => &[Symbol::Six],
                7 => &[Symbol::Seven],
                8 => &[Symbol::Eight],
                9 => &[Symbol::Nine],
                _ => return None,
            };
            // Check if excluded or wrong type
            if passes_symbol_filters(symbols, config)
                && get_num_type(symbols) >= config.min_num_type
            {
                if let Some(m) = make_match(symbols, target, error, table) {
                    return Some(m);
                }
            }
        }
        // For other integers, check if they match user constants
        for (idx, uc) in user_constants.iter().enumerate() {
            if idx < 16 && (uc.value - target).abs() < EXACT_TOLERANCE {
                if let Some(sym) = Symbol::from_byte(128 + idx as u8) {
                    let symbols = [sym];
                    if passes_symbol_filters(&symbols, config) && uc.num_type >= config.min_num_type
                    {
                        if let Some(m) =
                            make_match(&symbols, target, (uc.value - target).abs(), table)
                        {
                            return Some(m);
                        }
                    }
                }
            }
        }
    }

    // Check user constants first (they're explicitly defined)
    for (idx, uc) in user_constants.iter().enumerate() {
        if idx >= 16 {
            break;
        }
        if (uc.value - target).abs() < EXACT_TOLERANCE {
            if let Some(sym) = Symbol::from_byte(128 + idx as u8) {
                let symbols = [sym];
                if passes_symbol_filters(&symbols, config) && uc.num_type >= config.min_num_type {
                    if let Some(m) = make_match(&symbols, target, (uc.value - target).abs(), table)
                    {
                        return Some(m);
                    }
                }
            }
        }
    }

    // Check known constant candidates
    let candidates = get_constant_candidates();
    for candidate in candidates {
        // Skip if contains excluded symbols
        if !passes_symbol_filters(candidate.symbols, config) {
            continue;
        }
        // Skip if type doesn't meet requirement
        if get_num_type(candidate.symbols) < config.min_num_type {
            continue;
        }

        let expr = expr_from_symbols_with_table(candidate.symbols, table);
        if let Ok(result) = evaluate(&expr, target) {
            let error = (result.value - target).abs();
            if error < EXACT_TOLERANCE {
                if let Some(m) = make_match(candidate.symbols, target, error, table) {
                    return Some(m);
                }
            }
        }
    }

    // Check user constants with simple operations
    for (idx, uc) in user_constants.iter().enumerate() {
        if idx >= 16 {
            break;
        }
        if let Some(sym) = Symbol::from_byte(128 + idx as u8) {
            // Check 1/constant
            if uc.value != 0.0 {
                let recip_val = 1.0 / uc.value;
                if (recip_val - target).abs() < EXACT_TOLERANCE {
                    let symbols = [sym, Symbol::Recip];
                    if passes_symbol_filters(&symbols, config) && uc.num_type >= config.min_num_type
                    {
                        if let Some(m) =
                            make_match(&symbols, target, (recip_val - target).abs(), table)
                        {
                            return Some(m);
                        }
                    }
                }
            }
            // Check sqrt(constant)
            if uc.value > 0.0 {
                let sqrt_val = uc.value.sqrt();
                if (sqrt_val - target).abs() < EXACT_TOLERANCE {
                    let symbols = [sym, Symbol::Sqrt];
                    if passes_symbol_filters(&symbols, config) && uc.num_type >= config.min_num_type
                    {
                        if let Some(m) =
                            make_match(&symbols, target, (sqrt_val - target).abs(), table)
                        {
                            return Some(m);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Create a Match from symbols representing the RHS value
fn make_match(symbols: &[Symbol], target: f64, error: f64, table: &SymbolTable) -> Option<Match> {
    let lhs_expr = expr_from_symbols_with_table(&[Symbol::X], table);
    let rhs_expr = expr_from_symbols_with_table(symbols, table);
    let complexity = lhs_expr.complexity() + rhs_expr.complexity();

    let lhs_eval = evaluate(&lhs_expr, target).ok()?;
    let rhs_eval = evaluate(&rhs_expr, target).ok()?;

    Some(Match {
        lhs: EvaluatedExpr {
            expr: lhs_expr,
            value: lhs_eval.value,
            derivative: lhs_eval.derivative,
            num_type: NumType::Transcendental,
        },
        rhs: EvaluatedExpr {
            expr: rhs_expr,
            value: rhs_eval.value,
            derivative: 0.0,
            num_type: rhs_eval.num_type,
        },
        x_value: target,
        error,
        complexity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> FastMatchConfig<'static> {
        static EMPTY: std::sync::OnceLock<HashSet<u8>> = std::sync::OnceLock::new();
        let empty = EMPTY.get_or_init(HashSet::new);
        FastMatchConfig {
            excluded_symbols: empty,
            allowed_symbols: None,
            min_num_type: NumType::Transcendental,
        }
    }

    fn default_table() -> SymbolTable {
        SymbolTable::new()
    }

    #[test]
    fn test_pi_match() {
        let m = find_fast_match(
            std::f64::consts::PI,
            &[],
            &default_config(),
            &default_table(),
        );
        assert!(m.is_some());
        let m = m.unwrap();
        assert!(m.error.abs() < 1e-14);
        assert_eq!(m.rhs.expr.to_postfix(), "p");
    }

    #[test]
    fn test_pi_excluded() {
        let excluded: HashSet<u8> = vec![b'p'].into_iter().collect();
        let config = FastMatchConfig {
            excluded_symbols: &excluded,
            allowed_symbols: None,
            min_num_type: NumType::Transcendental,
        };
        let m = find_fast_match(std::f64::consts::PI, &[], &config, &default_table());
        assert!(m.is_none(), "Should not find pi when it's excluded");
    }

    #[test]
    fn test_pi_algebraic_only() {
        static EMPTY: std::sync::OnceLock<HashSet<u8>> = std::sync::OnceLock::new();
        let empty = EMPTY.get_or_init(HashSet::new);
        let config = FastMatchConfig {
            excluded_symbols: empty,
            allowed_symbols: None,
            min_num_type: NumType::Algebraic,
        };
        let m = find_fast_match(std::f64::consts::PI, &[], &config, &default_table());
        assert!(
            m.is_none(),
            "Should not find pi when only algebraic allowed"
        );
    }

    #[test]
    fn test_sqrt2_algebraic_ok() {
        static EMPTY: std::sync::OnceLock<HashSet<u8>> = std::sync::OnceLock::new();
        let empty = EMPTY.get_or_init(HashSet::new);
        let config = FastMatchConfig {
            excluded_symbols: empty,
            allowed_symbols: None,
            min_num_type: NumType::Algebraic,
        };
        let m = find_fast_match(2.0_f64.sqrt(), &[], &config, &default_table());
        assert!(m.is_some(), "sqrt(2) should be found with algebraic-only");
    }

    #[test]
    fn test_e_match() {
        let m = find_fast_match(
            std::f64::consts::E,
            &[],
            &default_config(),
            &default_table(),
        );
        assert!(m.is_some());
        let m = m.unwrap();
        assert!(m.error.abs() < 1e-14);
        assert_eq!(m.rhs.expr.to_postfix(), "e");
    }

    #[test]
    fn test_sqrt2_match() {
        let m = find_fast_match(2.0_f64.sqrt(), &[], &default_config(), &default_table());
        assert!(m.is_some());
        let m = m.unwrap();
        assert!(m.error.abs() < 1e-14);
        assert_eq!(m.rhs.expr.to_postfix(), "2q");
    }

    #[test]
    fn test_phi_match() {
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        let m = find_fast_match(phi, &[], &default_config(), &default_table());
        assert!(m.is_some());
        let m = m.unwrap();
        assert!(m.error.abs() < 1e-14);
        assert_eq!(m.rhs.expr.to_postfix(), "f");
    }

    #[test]
    fn test_integer_match() {
        let m = find_fast_match(5.0, &[], &default_config(), &default_table());
        assert!(m.is_some());
        let m = m.unwrap();
        assert!(m.error.abs() < 1e-14);
        assert_eq!(m.rhs.expr.to_postfix(), "5");
    }

    #[test]
    fn test_no_match_for_random() {
        // 2.506314 is not a simple constant
        let m = find_fast_match(2.506314, &[], &default_config(), &default_table());
        assert!(m.is_none());
    }

    #[test]
    fn test_user_constant_match() {
        let uc = UserConstant {
            weight: 4,
            name: "myconst".to_string(),
            description: "Test constant".to_string(),
            value: std::f64::consts::E,
            num_type: NumType::Transcendental,
        };
        let m = find_fast_match(
            std::f64::consts::E,
            &[uc],
            &default_config(),
            &default_table(),
        );
        assert!(m.is_some());
    }
}
