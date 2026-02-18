//! WebAssembly bindings for ries-rs
//!
//! This module provides WASM bindings using wasm-bindgen, allowing ries-rs
//! to be used from JavaScript/TypeScript in browsers and Node.js.
//!
//! # Installation
//!
//! ```bash
//! npm install ries-rs
//! ```
//!
//! # Usage
//!
//! ```javascript
//! import { search, WasmMatch, listPresets, version } from 'ries-rs';
//!
//! // Simple search
//! const results = search(3.1415926535);
//! for (const m of results) {
//!   console.log(`${m.lhs} = ${m.rhs} (error: ${m.error.toExponential(2)})`);
//! }
//!
//! // With options
//! const results = search(1.618033988, {
//!   level: 3,
//!   maxMatches: 20,
//!   preset: 'physics'
//! });
//! ```

use wasm_bindgen::prelude::*;
use serde::{Serialize, Deserialize};

/// A matched equation from the search
#[wasm_bindgen]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WasmMatch {
    /// Left-hand side expression (contains x)
    #[wasm_bindgen(getter_with_clone)]
    pub lhs: String,
    /// Right-hand side expression (constants only)
    #[wasm_bindgen(getter_with_clone)]
    pub rhs: String,
    /// Postfix representation of LHS
    #[wasm_bindgen(getter_with_clone)]
    pub lhs_postfix: String,
    /// Postfix representation of RHS
    #[wasm_bindgen(getter_with_clone)]
    pub rhs_postfix: String,
    /// Solved value of x
    pub x_value: f64,
    /// Error (x_value - target)
    pub error: f64,
    /// Complexity score
    pub complexity: u32,
    /// Whether this is an exact match
    pub is_exact: bool,
}

impl From<crate::search::Match> for WasmMatch {
    fn from(m: crate::search::Match) -> Self {
        Self {
            lhs: m.lhs.expr.to_infix(),
            rhs: m.rhs.expr.to_infix(),
            lhs_postfix: m.lhs.expr.to_postfix(),
            rhs_postfix: m.rhs.expr.to_postfix(),
            x_value: m.x_value,
            error: m.error,
            complexity: m.complexity,
            is_exact: m.error.abs() < crate::thresholds::EXACT_MATCH_TOLERANCE,
        }
    }
}

#[wasm_bindgen]
impl WasmMatch {
    /// Get a string representation
    pub fn to_string(&self) -> String {
        format!(
            "{} = {}  [error: {:.2e}] {{{}}}",
            self.lhs, self.rhs, self.error, self.complexity
        )
    }

    /// Convert to a plain JavaScript object
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(self).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

/// Search options for WASM
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct SearchOptions {
    /// Search level (0-5). Higher = more expressions searched
    pub level: u32,
    /// Maximum number of matches to return
    pub max_matches: usize,
    /// Domain preset name
    #[wasm_bindgen(getter_with_clone)]
    pub preset: Option<String>,
}

#[wasm_bindgen]
impl SearchOptions {
    /// Create default search options
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            level: 2,
            max_matches: 16,
            preset: None,
        }
    }

    /// Set the search level
    pub fn level(mut self, level: u32) -> Self {
        self.level = level;
        self
    }

    /// Set the maximum number of matches
    pub fn max_matches(mut self, max_matches: usize) -> Self {
        self.max_matches = max_matches;
        self
    }

    /// Set the domain preset
    pub fn preset(mut self, preset: String) -> Self {
        self.preset = Some(preset);
        self
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a GenConfig from simple parameters
fn build_gen_config(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    _preset: Option<&str>,
) -> crate::gen::GenConfig {
    use crate::symbol::{NumType, Symbol};
    use std::collections::HashMap;

    // Start with default symbols
    let constants: Vec<Symbol> = vec![
        Symbol::One, Symbol::Two, Symbol::Three, Symbol::Four, Symbol::Five,
        Symbol::Six, Symbol::Seven, Symbol::Eight, Symbol::Nine,
        Symbol::Pi, Symbol::E, Symbol::Phi, Symbol::Gamma,
        Symbol::Apery, Symbol::Catalan, Symbol::Plastic,
    ];

    let unary_ops: Vec<Symbol> = vec![
        Symbol::Neg, Symbol::Recip, Symbol::Sqrt, Symbol::Square,
        Symbol::Ln, Symbol::Exp,
        Symbol::SinPi, Symbol::CosPi, Symbol::TanPi,
        Symbol::LambertW,
    ];

    let binary_ops: Vec<Symbol> = vec![
        Symbol::Add, Symbol::Sub, Symbol::Mul, Symbol::Div, Symbol::Pow
    ];

    crate::gen::GenConfig {
        max_lhs_complexity,
        max_rhs_complexity,
        max_length: 21,
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
        user_constants: vec![],
        user_functions: vec![],
        show_pruned_arith: false,
    }
}

/// Search for algebraic equations given a target value
///
/// @param target - The target value to find equations for
/// @param options - Search options (level, maxMatches, preset)
/// @returns Array of WasmMatch objects sorted by error
///
/// @example
/// ```javascript
/// const results = search(3.14159);
/// console.log(results[0].lhs); // "x"
/// console.log(results[0].rhs); // "pi"
/// ```
#[wasm_bindgen]
pub fn search(target: f64, options: Option<SearchOptions>) -> Vec<WasmMatch> {
    let opts = options.unwrap_or_default();

    // Convert level to complexity limits
    let base_lhs: u32 = 10;
    let base_rhs: u32 = 12;
    let level_factor = (4.0 * opts.level as f32) as u32;
    let max_lhs_complexity = base_lhs + level_factor;
    let max_rhs_complexity = base_rhs + level_factor;

    // Build generation config
    let gen_config = build_gen_config(max_lhs_complexity, max_rhs_complexity, opts.preset.as_deref());

    // Build search config
    let max_error = (target.abs() * 0.01).max(1e-12);
    let search_config = crate::search::SearchConfig {
        target,
        max_matches: opts.max_matches * 2,
        max_error,
        stop_at_exact: false,
        stop_below: None,
        zero_value_threshold: 1e-4,
        newton_iterations: 15,
        user_constants: vec![],
        user_functions: vec![],
        refine_with_newton: true,
        rhs_allowed_symbols: None,
        rhs_excluded_symbols: None,
        show_newton: false,
        show_match_checks: false,
        show_pruned_arith: false,
        show_pruned_range: false,
        show_db_adds: false,
        match_all_digits: false,
        derivative_margin: crate::thresholds::DEGENERATE_DERIVATIVE,
        ranking_mode: crate::pool::RankingMode::Complexity,
    };

    // Perform search (use sequential for WASM - no rayon in browser)
    let (matches, _stats) = crate::search::search_with_stats_and_config(&gen_config, &search_config);

    // Convert to WasmMatch and limit to max_matches
    matches
        .into_iter()
        .take(opts.max_matches)
        .map(WasmMatch::from)
        .collect()
}

/// Get list of available domain presets
///
/// @returns Object mapping preset names to descriptions
#[wasm_bindgen]
pub fn list_presets() -> Result<JsValue, JsValue> {
    let presets: Vec<(&str, &str)> = crate::presets::Preset::all()
        .iter()
        .map(|p| (p.name(), p.description()))
        .collect();

    serde_wasm_bindgen::to_value(&presets).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Get version information
///
/// @returns Version string
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Initialize the WASM module (call this before using other functions)
#[wasm_bindgen]
pub fn init() {
    // Currently no initialization needed, but provides a hook for future use
}
