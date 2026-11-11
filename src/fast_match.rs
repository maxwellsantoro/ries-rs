//! Fast-path exact match detection
//!
//! Before doing expensive expression generation, check if the target
//! is a simple exact match (like pi, e, sqrt(2), etc.) that can be
//! found instantly.

use crate::eval::{evaluate_with_context, EvalContext};
use crate::expr::{EvaluatedExpr, Expression};
use crate::gen::GenConfig;
use crate::pool::{compare_matches, RankingMode};
use crate::profile::UserConstant;
use crate::search::Match;
use crate::symbol::{NumType, Symbol};
use crate::symbol_table::SymbolTable;
use std::collections::HashSet;

/// Build an expression from symbols using table-based weights
fn expr_from_symbols_with_table(symbols: &[Symbol], table: &SymbolTable) -> Expression {
    let mut expr = Expression::new();
    for &sym in symbols {
        expr.push_with_table(sym, table);
    }
    expr
}

fn contains_excluded(symbols: &[Symbol], excluded: &HashSet<u8>) -> bool {
    symbols.iter().any(|s| excluded.contains(&(*s as u8)))
}

/// A candidate for a fast exact match
struct FastCandidate {
    /// The expression (as symbols)
    symbols: &'static [Symbol],
}

/// Generate fast candidates for common constants and simple expressions
fn get_constant_candidates() -> &'static [FastCandidate] {
    static CANDIDATES: &[FastCandidate] = &[
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
    ];

    CANDIDATES
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
/// The result is the first exact candidate in a fixed list, not necessarily
/// the best-ranked equation; use [`find_certified_fast_match`] to replace a
/// search.
pub fn find_fast_match(
    target: f64,
    user_constants: &[UserConstant],
    config: &FastMatchConfig<'_>,
    table: &SymbolTable,
) -> Option<Match> {
    let context = EvalContext::from_slices(user_constants, &[]);
    find_fast_match_with_context(target, &context, config, table)
}

/// Try to find a fast exact match for the target value using an explicit evaluation context.
pub fn find_fast_match_with_context(
    target: f64,
    context: &EvalContext<'_>,
    config: &FastMatchConfig<'_>,
    table: &SymbolTable,
) -> Option<Match> {
    let make_candidate = |symbols: &[Symbol]| {
        if !passes_symbol_filters(symbols, config) {
            return None;
        }
        let m = make_match(symbols, target, table, context).filter(|m| m.exact)?;
        (m.rhs.num_type >= config.min_num_type).then_some(m)
    };

    // Try the nearest single digit first, then user-defined constants.
    let digit = target.round();
    if (1.0..=9.0).contains(&digit) {
        if let Some(m) = make_candidate(&[Symbol::from_byte(b'0' + digit as u8)?]) {
            return Some(m);
        }
    }
    let user_symbols = context
        .user_constants
        .iter()
        .enumerate()
        .filter_map(|(index, _)| crate::symbol_table::user_constant_symbol_opt(index));
    for symbol in user_symbols.clone() {
        if let Some(m) = make_candidate(&[symbol]) {
            return Some(m);
        }
    }

    for candidate in get_constant_candidates() {
        if let Some(m) = make_candidate(candidate.symbols) {
            return Some(m);
        }
    }

    for symbol in user_symbols {
        for op in [Symbol::Recip, Symbol::Sqrt] {
            if let Some(m) = make_candidate(&[symbol, op]) {
                return Some(m);
            }
        }
    }
    None
}

/// Lower bound on the complexity of every equation except `x = <atom>`.
///
/// Any equation has an LHS containing `x` and an RHS with at least one atom.
/// An equation other than `x = <atom>` has at least one more symbol: a unary
/// operator, or a binary operator with its extra operand. Minimums are taken
/// over both sides' enabled symbols, so the bound holds for either side.
pub fn non_atomic_equation_complexity_floor(config: &GenConfig) -> u32 {
    let table = &config.symbol_table;
    let rhs = |overridden: &Option<Vec<Symbol>>, base: &[Symbol]| -> Vec<Symbol> {
        overridden.as_deref().unwrap_or(base).to_vec()
    };
    let min_weight = |sets: [&[Symbol]; 2]| {
        sets.iter()
            .flat_map(|set| set.iter())
            .map(|&sym| table.weight(sym))
            .min()
    };
    let rhs_constants = rhs(&config.rhs_constants, &config.constants);
    let rhs_unary = rhs(&config.rhs_unary_ops, &config.unary_ops);
    let rhs_binary = rhs(&config.rhs_binary_ops, &config.binary_ops);

    let x = table.weight(Symbol::X);
    let min_atom = min_weight([&config.constants, &rhs_constants])
        .unwrap_or(x)
        .min(x);
    let unary_step = min_weight([&config.unary_ops, &rhs_unary]);
    let binary_step =
        min_weight([&config.binary_ops, &rhs_binary]).map(|w| w.saturating_add(min_atom));
    let step = match (unary_step, binary_step) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => u32::MAX,
    };
    x.saturating_add(min_atom).saturating_add(step)
}

/// Find an exact `x = <atom>` match that provably ranks first under the
/// search's stop contract with `stop_at_exact` (and `stop_below`, if set).
///
/// Every enabled RHS atom is tried. Candidates must pass the symbol filters,
/// the numeric type floor, and `accept` (the caller's full match eligibility).
/// Among those meeting a stop condition (exact, or error below `stop_below`),
/// the stop match is the simplest, with ties broken by [`compare_matches`]
/// under `ranking_mode`. It is returned only when it is exact and its
/// complexity is below [`non_atomic_equation_complexity_floor`]: then no other
/// equation shape can be as simple, so the search would lead with the same
/// match. Otherwise the caller must run the search.
pub fn find_certified_fast_match(
    target: f64,
    context: &EvalContext<'_>,
    config: &FastMatchConfig<'_>,
    gen_config: &GenConfig,
    ranking_mode: RankingMode,
    stop_below: Option<f64>,
    accept: impl Fn(&Match) -> bool,
) -> Option<Match> {
    let table = &gen_config.symbol_table;
    let atoms = gen_config
        .rhs_constants
        .as_deref()
        .unwrap_or(&gen_config.constants);
    let meets_stop = |m: &Match| m.exact || stop_below.is_some_and(|t| m.error.abs() < t);
    let best = atoms
        .iter()
        .filter(|&&sym| sym != Symbol::X && passes_symbol_filters(&[sym], config))
        .filter_map(|&sym| make_match(&[sym], target, table, context))
        .filter(|m| m.rhs.num_type >= config.min_num_type && meets_stop(m) && accept(m))
        .min_by(|a, b| {
            a.complexity
                .cmp(&b.complexity)
                .then_with(|| compare_matches(a, b, ranking_mode))
        })?;
    (best.exact && best.complexity < non_atomic_equation_complexity_floor(gen_config))
        .then_some(best)
}

/// Create a Match from symbols representing the RHS value
fn make_match(
    symbols: &[Symbol],
    target: f64,
    table: &SymbolTable,
    context: &EvalContext<'_>,
) -> Option<Match> {
    let lhs_expr = expr_from_symbols_with_table(&[Symbol::X], table);
    let rhs_expr = expr_from_symbols_with_table(symbols, table);
    let complexity = lhs_expr.complexity().saturating_add(rhs_expr.complexity());

    let lhs_eval = evaluate_with_context(&lhs_expr, target, context).ok()?;
    let rhs_eval = evaluate_with_context(&rhs_expr, target, context).ok()?;

    let x_value = rhs_eval.value;
    let exact = crate::search::match_exactness(
        lhs_eval.value,
        rhs_eval.value,
        lhs_eval.derivative,
        x_value,
        target,
    )?;

    Some(Match {
        lhs: EvaluatedExpr {
            expr: lhs_expr,
            value: lhs_eval.value,
            derivative: lhs_eval.derivative,
            num_type: lhs_eval.num_type,
        },
        rhs: EvaluatedExpr {
            expr: rhs_expr,
            value: rhs_eval.value,
            derivative: 0.0,
            num_type: rhs_eval.num_type,
        },
        x_value,
        error: x_value - target,
        complexity,
        exact,
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
        assert_eq!(m.lhs.num_type, NumType::Integer);
    }

    #[test]
    fn test_division_candidate_is_rational() {
        let m = find_fast_match(2.0 / 3.0, &[], &default_config(), &default_table());
        assert!(m.is_some());
        let m = m.unwrap();
        assert_eq!(m.rhs.expr.to_postfix(), "23/");
        assert_eq!(m.rhs.num_type, NumType::Rational);
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
