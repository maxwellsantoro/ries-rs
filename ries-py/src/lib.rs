//! Python bindings for ries-rs
//!
//! This module provides Python bindings using PyO3, allowing ries-rs
//! to be used from Python code.
//!
//! # Installation
//!
//! ```bash
//! pip install ries-rs
//! ```
//!
//! # Usage
//!
//! ```python
//! import ries_rs
//!
//! # Simple search
//! results = ries_rs.search(3.1415926535)
//! for r in results:
//!     print(f"{r.lhs} = {r.rhs}  (error: {r.error:.2e})")
//!
//! # With options
//! results = ries_rs.search(
//!     1.618033988,
//!     level=3,
//!     max_matches=20,
//!     preset="physics"
//! )
//! ```

use pyo3::prelude::*;
use pyo3::types::PyDict;

const MAX_API_LEVEL: u32 = 5;
const MAX_API_MATCHES: usize = 10_000;

/// A matched equation from the search.
///
/// Each match represents an equation of the form `lhs = rhs` where `lhs`
/// contains the variable `x` and `rhs` contains only constants. The equation
/// is solved such that `x` approximates the target value.
///
/// Attributes
/// ----------
/// lhs : str
///     Left-hand side expression in infix notation (contains x)
/// rhs : str
///     Right-hand side expression in infix notation (constants only)
/// lhs_postfix : str
///     Left-hand side expression in postfix (RPN) notation
/// rhs_postfix : str
///     Right-hand side expression in postfix (RPN) notation
/// x_value : float
///     The solved value of x that satisfies the equation
/// error : float
///     The difference between x_value and the original target
/// complexity : int
///     Complexity score (lower = simpler expression)
/// is_exact : bool
///     True if the match is within exact match tolerance (< 1e-14)
///
/// Examples
/// --------
/// >>> m = PyMatch(lhs='x', rhs='pi', x_value=3.14159..., error=8.98e-11, complexity=14)
/// >>> print(m)
/// x = pi  [error: 8.98e-11] {14}
/// >>> m.to_dict()
/// {'lhs': 'x', 'rhs': 'pi', 'x_value': 3.14159..., ...}
#[pyclass]
#[derive(Clone)]
pub struct PyMatch {
    /// Left-hand side expression (contains x)
    #[pyo3(get)]
    pub lhs: String,
    /// Right-hand side expression (constants only)
    #[pyo3(get)]
    pub rhs: String,
    /// Postfix representation of LHS
    #[pyo3(get)]
    pub lhs_postfix: String,
    /// Postfix representation of RHS
    #[pyo3(get)]
    pub rhs_postfix: String,
    /// Solved x = expression (if analytically solvable)
    #[pyo3(get)]
    pub solve_for_x: Option<String>,
    /// Solved x = expression in postfix
    #[pyo3(get)]
    pub solve_for_x_postfix: Option<String>,
    /// Canonical key for deduplication
    #[pyo3(get)]
    pub canonical_key: String,
    /// Solved value of x
    #[pyo3(get)]
    pub x_value: f64,
    /// Error (x_value - target)
    #[pyo3(get)]
    pub error: f64,
    /// Complexity score
    #[pyo3(get)]
    pub complexity: u32,
    /// Number of operators in equation
    #[pyo3(get)]
    pub operator_count: usize,
    /// Maximum tree depth of equation
    #[pyo3(get)]
    pub tree_depth: usize,
    /// Whether this is an exact match
    #[pyo3(get)]
    pub is_exact: bool,
}

#[pymethods]
impl PyMatch {
    fn __repr__(&self) -> String {
        format!(
            "PyMatch(lhs='{}', rhs='{}', error={:.2e}, complexity={})",
            self.lhs, self.rhs, self.error, self.complexity
        )
    }

    fn __str__(&self) -> String {
        format!(
            "{} = {}  [error: {:.2e}] {{{}}}",
            self.lhs, self.rhs, self.error, self.complexity
        )
    }

    /// Convert the match to a Python dictionary.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        dict.set_item("lhs", &self.lhs)?;
        dict.set_item("rhs", &self.rhs)?;
        dict.set_item("lhs_postfix", &self.lhs_postfix)?;
        dict.set_item("rhs_postfix", &self.rhs_postfix)?;
        dict.set_item("solve_for_x", &self.solve_for_x)?;
        dict.set_item("solve_for_x_postfix", &self.solve_for_x_postfix)?;
        dict.set_item("canonical_key", &self.canonical_key)?;
        dict.set_item("x_value", self.x_value)?;
        dict.set_item("error", self.error)?;
        dict.set_item("complexity", self.complexity)?;
        dict.set_item("operator_count", self.operator_count)?;
        dict.set_item("tree_depth", self.tree_depth)?;
        dict.set_item("is_exact", self.is_exact)?;
        Ok(dict.unbind())
    }
}

impl From<ries_core::search::Match> for PyMatch {
    fn from(m: ries_core::search::Match) -> Self {
        let lhs_infix = m.lhs.expr.to_infix();
        let rhs_infix = m.rhs.expr.to_infix();

        // Analytical solver
        let solved = ries_core::solver::solve_for_x_rhs_expression(&m.lhs.expr, &m.rhs.expr);
        let solve_for_x = solved.as_ref().map(|e| format!("x = {}", e.to_infix()));
        let solve_for_x_postfix = solved.as_ref().map(|e| e.to_postfix());

        // Canonical key
        let canonical_key = ries_core::solver::canonical_expression_key(&m.lhs.expr)
            .zip(ries_core::solver::canonical_expression_key(&m.rhs.expr))
            .map(|(l, r)| format!("{}={}", l, r))
            .unwrap_or_else(|| format!("{}={}", m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix()));

        Self {
            lhs: lhs_infix,
            rhs: rhs_infix,
            lhs_postfix: m.lhs.expr.to_postfix(),
            rhs_postfix: m.rhs.expr.to_postfix(),
            solve_for_x,
            solve_for_x_postfix,
            canonical_key,
            x_value: m.x_value,
            error: m.error,
            complexity: m.complexity,
            operator_count: m.lhs.expr.operator_count() + m.rhs.expr.operator_count(),
            tree_depth: m.lhs.expr.tree_depth().max(m.rhs.expr.tree_depth()),
            is_exact: m.error.abs() < ries_core::thresholds::EXACT_MATCH_TOLERANCE,
        }
    }
}

fn build_symbol_table(
    profile: &ries_core::profile::Profile,
) -> ries_core::symbol_table::SymbolTable {
    ries_core::symbol_table::SymbolTable::from_profile(profile)
}

/// Build a GenConfig from simple parameters
fn build_gen_config(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    profile: &ries_core::profile::Profile,
) -> ries_core::gen::GenConfig {
    use ries_core::symbol::{NumType, Symbol};
    use std::collections::HashMap;
    use std::sync::Arc;

    let mut constants: Vec<Symbol> = Symbol::constants().to_vec();
    let mut unary_ops: Vec<Symbol> = Symbol::unary_ops().to_vec();
    let binary_ops: Vec<Symbol> = Symbol::binary_ops().to_vec();

    for idx in 0..profile.constants.len().min(16) {
        if let Some(sym) = Symbol::from_byte(128 + idx as u8) {
            constants.push(sym);
        }
    }
    for idx in 0..profile.functions.len().min(16) {
        if let Some(sym) = Symbol::from_byte(144 + idx as u8) {
            unary_ops.push(sym);
        }
    }

    let symbol_table = build_symbol_table(profile);

    ries_core::gen::GenConfig {
        max_lhs_complexity,
        max_rhs_complexity,
        max_length: 21, // Default max length
        constants,
        unary_ops,
        binary_ops,
        rhs_constants: None,
        rhs_unary_ops: None,
        rhs_binary_ops: None,
        symbol_max_counts: HashMap::new(),
        rhs_symbol_max_counts: None,
        min_num_type: NumType::Transcendental,
        generate_lhs: true,
        generate_rhs: true,
        user_constants: profile.constants.clone(),
        user_functions: profile.functions.clone(),
        show_pruned_arith: false,
        symbol_table: Arc::new(symbol_table),
    }
}

/// Search for algebraic equations given a target value
///
/// Parameters
/// ----------
/// target : float
///     The target value to find equations for
/// level : int, optional
///     Search level (default 2). Higher levels search more expressions.
///     Level 0 ≈ 89M expressions, Level 2 ≈ 11B, Level 5 ≈ 15T
/// max_matches : int, optional
///     Maximum number of matches to return (default 16).
///     Note: Internally, the search requests 2*max_matches candidates
///     to ensure high-quality results after filtering and ranking,
///     then returns the best max_matches.
/// preset : str, optional
///     Domain preset: "analytic-nt", "elliptic", "combinatorics",
///     "physics", "number-theory", "calculus"
/// parallel : bool, optional
///     Use parallel search (default True)
///
/// Returns
/// -------
/// list[PyMatch]
///     List of matches sorted by error
///
/// Examples
/// --------
/// >>> import ries_rs
/// >>> results = ries_rs.search(3.1415926535)
/// >>> print(results[0])
/// x = pi  [error: 8.98e-11] {14}
#[pyfunction]
#[pyo3(signature = (target, level=2, max_matches=16, preset=None, parallel=true))]
fn search(
    target: f64,
    level: u32,
    max_matches: usize,
    preset: Option<&str>,
    parallel: bool,
) -> PyResult<Vec<PyMatch>> {
    if level > MAX_API_LEVEL {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid level {}. Supported range is 0..={}.",
            level, MAX_API_LEVEL
        )));
    }
    if max_matches > MAX_API_MATCHES {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "max_matches {} is too large. Maximum supported value is {}.",
            max_matches, MAX_API_MATCHES
        )));
    }
    let internal_max_matches = max_matches
        .checked_mul(2)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("max_matches is too large"))?;

    // Use the standard level-to-complexity mapping
    let (max_lhs_complexity, max_rhs_complexity) = ries_core::search::level_to_complexity(level);

    let mut profile = ries_core::profile::Profile::new();
    if let Some(preset_name) = preset {
        let parsed = ries_core::presets::Preset::from_str(preset_name).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Unknown preset '{}'. Use list_presets() for available options.",
                preset_name
            ))
        })?;
        profile = profile.merge(parsed.to_profile());
    }

    // Build generation config
    let gen_config = build_gen_config(max_lhs_complexity, max_rhs_complexity, &profile);

    // Build search config
    let max_error = (target.abs() * 0.01).max(1e-12);
    let search_config = ries_core::search::SearchConfig {
        target,
        max_matches: internal_max_matches, // Get more to filter later
        max_error,
        stop_at_exact: false,
        stop_below: None,
        zero_value_threshold: 1e-4,
        newton_iterations: 15,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        refine_with_newton: true,
        rhs_allowed_symbols: None,
        rhs_excluded_symbols: None,
        show_newton: false,
        show_match_checks: false,
        show_pruned_arith: false,
        show_pruned_range: false,
        show_db_adds: false,
        match_all_digits: false,
        derivative_margin: ries_core::thresholds::DEGENERATE_DERIVATIVE,
        ranking_mode: ries_core::pool::RankingMode::Complexity,
    };

    // Perform search
    let (matches, _stats) = if parallel {
        #[cfg(feature = "parallel")]
        {
            ries_core::search::search_parallel_with_stats_and_config(&gen_config, &search_config)
        }
        #[cfg(not(feature = "parallel"))]
        {
            ries_core::search::search_with_stats_and_config(&gen_config, &search_config)
        }
    } else {
        ries_core::search::search_with_stats_and_config(&gen_config, &search_config)
    };

    // Convert to PyMatch and limit to max_matches
    let py_matches: Vec<PyMatch> = matches
        .into_iter()
        .take(max_matches)
        .map(PyMatch::from)
        .collect();

    Ok(py_matches)
}

/// Get list of available domain presets
///
/// Returns
/// -------
/// dict[str, str]
///     Dictionary mapping preset names to descriptions
#[pyfunction]
fn list_presets() -> PyResult<Py<PyDict>> {
    Python::with_gil(|py| {
        let dict = PyDict::new_bound(py);
        for preset in ries_core::presets::Preset::all() {
            dict.set_item(preset.name(), preset.description())?;
        }
        Ok(dict.unbind())
    })
}

/// Get version information
///
/// Returns
/// -------
/// str
///     Version string
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// RIES-RS Python module
///
/// Find algebraic equations given their solution.
///
/// This is a Python binding for the ries-rs Rust library, which implements
/// an inverse symbolic calculator similar to Robert Munafo's RIES.
///
/// Examples
/// --------
/// >>> import ries_rs
/// >>> results = ries_rs.search(3.14159)
/// >>> for r in results[:5]:
/// ...     print(f"{r.lhs} = {r.rhs}")
/// x = pi
/// x-3 = 1/7
/// 6-x = 2*phi
/// x*7 = 22
/// x/phi = phi+1
#[pymodule]
fn ries_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMatch>()?;
    m.add_function(wrap_pyfunction!(search, m)?)?;
    m.add_function(wrap_pyfunction!(list_presets, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gen_config_includes_preset_user_constants() {
        let profile = ries_core::profile::Profile::new()
            .merge(ries_core::presets::Preset::AnalyticNT.to_profile());
        let config = build_gen_config(18, 20, &profile);

        assert!(
            !config.user_constants.is_empty(),
            "preset profile constants should flow into GenConfig user_constants"
        );
        assert!(
            config
                .constants
                .contains(&ries_core::symbol::Symbol::UserConstant0),
            "user constant symbol slots should be added to generation constants"
        );
    }

    #[test]
    fn test_search_rejects_unknown_preset() {
        let result = search(std::f64::consts::PI, 2, 8, Some("does-not-exist"), false);
        assert!(
            result.is_err(),
            "unknown preset should return a Python error"
        );
    }
}
