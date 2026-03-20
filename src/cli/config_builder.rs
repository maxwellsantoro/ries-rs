//! Configuration builders from CLI arguments
//!
//! Converts parsed CLI arguments into runtime configuration structs.
//!
//! This module provides builders for:
//! - [`GenConfig`]: Configuration for expression generation
//!
//! The builders handle:
//! - Symbol filtering (exclude, enable, only_symbols)
//! - RHS-specific symbol overrides
//! - Operator count limits
//! - User-defined constants and functions

use crate::gen::GenConfig;
use crate::profile::Profile;
use crate::symbol::{NumType, Symbol};

use super::args::{parse_symbol_count_limits, parse_symbol_sets};

/// Build GenConfig from CLI arguments
///
/// This function constructs a `GenConfig` from the various CLI options
/// related to symbol filtering, complexity limits, and user-defined elements.
///
/// # Arguments
///
/// * `max_lhs_complexity` - Maximum complexity for LHS expressions (containing x)
/// * `max_rhs_complexity` - Maximum complexity for RHS expressions (constants only)
/// * `min_type` - Minimum numeric type required (e.g., Integer, Rational)
/// * `exclude` - Symbols to exclude (from -N/--exclude)
/// * `enable` - Symbols to re-enable (from -E/--enable)
/// * `only_symbols` - Only use these symbols (from -S)
/// * `exclude_rhs` - RHS-specific symbols to exclude
/// * `enable_rhs` - RHS-specific symbols to re-enable
/// * `only_symbols_rhs` - RHS-specific only symbols
/// * `op_limits` - Per-symbol count limits (from -O)
/// * `op_limits_rhs` - RHS-specific per-symbol count limits
/// * `profile` - Loaded runtime profile with user-defined symbols and overrides
/// * `show_pruned_arith` - Whether to show pruned arithmetic diagnostics
///
/// # Returns
///
/// A `Result` containing the configured `GenConfig` or an error string.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::field_reassign_with_default)]
pub fn build_gen_config(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    min_type: NumType,
    exclude: Option<&str>,
    enable: Option<&str>,
    only_symbols: Option<&str>,
    exclude_rhs: Option<&str>,
    enable_rhs: Option<&str>,
    only_symbols_rhs: Option<&str>,
    op_limits: Option<&str>,
    op_limits_rhs: Option<&str>,
    profile: &Profile,
    show_pruned_arith: bool,
) -> Result<GenConfig, String> {
    let mut config =
        crate::gen::build_gen_config_from_profile(max_lhs_complexity, max_rhs_complexity, profile)?;
    config.min_num_type = min_type;
    config.show_pruned_arith = show_pruned_arith;

    // Parse effective symbol sets (with -E/--enable support).
    let (allowed, excluded) = parse_symbol_sets(only_symbols, exclude, enable);
    let (allowed_rhs, excluded_rhs) = parse_symbol_sets(only_symbols_rhs, exclude_rhs, enable_rhs);

    let all_constants = config.constants.clone();
    let all_unary = config.unary_ops.clone();
    let all_binary = config.binary_ops.clone();

    // Apply LHS symbol filtering
    config.constants = filter_symbols(&all_constants, allowed.as_ref(), excluded.as_ref());
    config.unary_ops = filter_symbols(&all_unary, allowed.as_ref(), excluded.as_ref());
    config.binary_ops = filter_symbols(&all_binary, allowed.as_ref(), excluded.as_ref());

    // Parse -O/--op-limits into per-expression max symbol counts.
    if let Some(spec) = op_limits {
        config.symbol_max_counts = parse_symbol_count_limits(spec)?;
    }
    if let Some(spec_rhs) = op_limits_rhs {
        config.rhs_symbol_max_counts = Some(parse_symbol_count_limits(spec_rhs)?);
    }

    if allowed_rhs.is_some() || excluded_rhs.is_some() || op_limits_rhs.is_some() {
        let constants_base = if allowed_rhs.is_some() {
            all_constants.clone()
        } else {
            config.constants.clone()
        };
        let unary_base = if allowed_rhs.is_some() {
            all_unary.clone()
        } else {
            config.unary_ops.clone()
        };
        let binary_base = if allowed_rhs.is_some() {
            all_binary.clone()
        } else {
            config.binary_ops.clone()
        };

        config.rhs_constants = Some(filter_symbols(
            &constants_base,
            allowed_rhs.as_ref(),
            excluded_rhs.as_ref(),
        ));
        config.rhs_unary_ops = Some(filter_symbols(
            &unary_base,
            allowed_rhs.as_ref(),
            excluded_rhs.as_ref(),
        ));
        config.rhs_binary_ops = Some(filter_symbols(
            &binary_base,
            allowed_rhs.as_ref(),
            excluded_rhs.as_ref(),
        ));
    }

    Ok(config)
}

/// Filter a list of symbols based on allowed and excluded sets
///
/// # Arguments
///
/// * `symbols` - The base list of symbols to filter
/// * `allowed` - If set, only symbols in this set are kept
/// * `excluded` - If set, symbols in this set are removed
///
/// # Returns
///
/// A filtered vector of symbols
fn filter_symbols(
    symbols: &[Symbol],
    allowed: Option<&std::collections::HashSet<u8>>,
    excluded: Option<&std::collections::HashSet<u8>>,
) -> Vec<Symbol> {
    let mut result: Vec<Symbol> = symbols.to_vec();

    if let Some(allow_set) = allowed {
        result.retain(|s| allow_set.contains(&(*s as u8)));
    }

    if let Some(excl_set) = excluded {
        result.retain(|s| !excl_set.contains(&(*s as u8)));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gen_config_defaults() {
        let config = build_gen_config(
            10,
            12,
            NumType::Transcendental,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &Profile::new(),
            false,
        )
        .expect("should build default config");

        assert_eq!(config.max_lhs_complexity, 10);
        assert_eq!(config.max_rhs_complexity, 12);
        assert_eq!(config.min_num_type, NumType::Transcendental);
        // Should have all default symbols
        assert!(!config.constants.is_empty());
        assert!(!config.unary_ops.is_empty());
        assert!(!config.binary_ops.is_empty());
    }

    #[test]
    fn test_build_gen_config_with_exclude() {
        let config = build_gen_config(
            10,
            12,
            NumType::Transcendental,
            Some("p"), // exclude pi
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &Profile::new(),
            false,
        )
        .expect("should build config with exclude");

        // Pi should not be in constants
        assert!(!config.constants.contains(&Symbol::Pi));
    }

    #[test]
    fn test_build_gen_config_with_only_symbols() {
        let config = build_gen_config(
            10,
            12,
            NumType::Transcendental,
            None,
            None,
            Some("123"), // only 1, 2, 3
            None,
            None,
            None,
            None,
            None,
            &Profile::new(),
            false,
        )
        .expect("should build config with only symbols");

        // Should only have 1, 2, 3 as constants
        assert!(config.constants.contains(&Symbol::One));
        assert!(config.constants.contains(&Symbol::Two));
        assert!(config.constants.contains(&Symbol::Three));
        assert!(!config.constants.contains(&Symbol::Four));
        assert!(!config.constants.contains(&Symbol::Pi));
    }

    #[test]
    fn test_filter_symbols() {
        use std::collections::HashSet;

        let symbols = Symbol::constants();
        let allowed: HashSet<u8> = [b'1', b'2', b'3'].into_iter().collect();
        let excluded: HashSet<u8> = [b'2'].into_iter().collect();

        // Test with allowed only
        let filtered = filter_symbols(symbols, Some(&allowed), None);
        assert!(filtered.contains(&Symbol::One));
        assert!(filtered.contains(&Symbol::Two));
        assert!(filtered.contains(&Symbol::Three));
        assert!(!filtered.contains(&Symbol::Four));

        // Test with allowed and excluded
        let filtered = filter_symbols(symbols, Some(&allowed), Some(&excluded));
        assert!(filtered.contains(&Symbol::One));
        assert!(!filtered.contains(&Symbol::Two)); // excluded
        assert!(filtered.contains(&Symbol::Three));
    }

    #[test]
    fn test_build_gen_config_only_symbols_excludes_user_constant() {
        let mut profile = Profile::new();
        profile
            .add_constant(4, "k".to_string(), "custom".to_string(), 2.0)
            .unwrap();

        let config = build_gen_config(
            10,
            12,
            NumType::Transcendental,
            None,
            None,
            Some("123"),
            None,
            None,
            None,
            None,
            None,
            &profile,
            false,
        )
        .expect("should build config with user constant");

        assert!(!config.constants.contains(&Symbol::UserConstant0));
    }

    #[test]
    fn test_build_gen_config_only_symbols_excludes_user_function() {
        let mut profile = Profile::new();
        let udf = crate::udf::UserFunction::parse("4:sinh:hyperbolic sine:E|r-2/").unwrap();
        profile.add_function(udf).unwrap();

        let config = build_gen_config(
            10,
            12,
            NumType::Transcendental,
            None,
            None,
            Some("123"),
            None,
            None,
            None,
            None,
            None,
            &profile,
            false,
        )
        .expect("should build config with user function");

        assert!(!config.unary_ops.contains(&Symbol::UserFunction0));
    }

    #[test]
    fn test_build_gen_config_rhs_only_symbols_excludes_user_constant() {
        let mut profile = Profile::new();
        profile
            .add_constant(4, "k".to_string(), "custom".to_string(), 2.0)
            .unwrap();

        let config = build_gen_config(
            10,
            12,
            NumType::Transcendental,
            None,
            None,
            None,
            None,
            None,
            Some("123"),
            None,
            None,
            &profile,
            false,
        )
        .expect("should build config with RHS-only filter");

        let rhs_constants = config
            .rhs_constants
            .as_ref()
            .expect("RHS constants should be present");
        assert!(!rhs_constants.contains(&Symbol::UserConstant0));
    }

    #[test]
    fn test_build_gen_config_rejects_over_capacity() {
        let mut profile = Profile::new();
        for idx in 0..=16 {
            profile.constants.push(crate::profile::UserConstant {
                weight: 4,
                name: format!("c{}", idx),
                description: "custom".to_string(),
                value: idx as f64,
                num_type: NumType::Integer,
            });
        }

        let err = build_gen_config(
            10,
            12,
            NumType::Transcendental,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &profile,
            false,
        )
        .err()
        .expect("config builder should reject overflow profile");

        assert!(err.contains("At most 16 user constants"));
    }
}
