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

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

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
        // DEBUG: Print symbols to see what's being converted
        #[cfg(debug_assertions)]
        {
            eprintln!("DEBUG: Converting match to WASM");
            eprintln!("  LHS symbols: {:?}", m.lhs.expr.symbols());
            eprintln!("  RHS symbols: {:?}", m.rhs.expr.symbols());
        }

        // Convert with error handling to catch problematic expressions
        let lhs_infix = m.lhs.expr.to_infix();
        let rhs_infix = m.rhs.expr.to_infix();
        Self {
            lhs: lhs_infix,
            rhs: rhs_infix,
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
    #[allow(clippy::inherent_to_string)]
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
#[derive(Clone, Debug, Serialize)]
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

    /// Convert to a plain JavaScript object (for passing to search() or serialization)
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(self).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
struct SearchOptionsInput {
    level: u32,
    #[serde(rename = "maxMatches", alias = "max_matches")]
    max_matches: usize,
    preset: Option<String>,
}

impl Default for SearchOptionsInput {
    fn default() -> Self {
        Self {
            level: 2,
            max_matches: 16,
            preset: None,
        }
    }
}

fn parse_search_options(options: Option<JsValue>) -> Result<SearchOptionsInput, JsValue> {
    match options {
        None => Ok(SearchOptionsInput::default()),
        Some(value) if value.is_null() || value.is_undefined() => Ok(SearchOptionsInput::default()),
        Some(value) => serde_wasm_bindgen::from_value(value)
            .map_err(|e| JsValue::from_str(&format!("Invalid search options: {}", e))),
    }
}

fn build_symbol_table(profile: &crate::profile::Profile) -> crate::symbol_table::SymbolTable {
    crate::symbol_table::SymbolTable::from_profile(profile)
}

/// Build a GenConfig from simple parameters
fn build_gen_config(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    profile: &crate::profile::Profile,
) -> crate::gen::GenConfig {
    use crate::symbol::{NumType, Symbol};
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
        user_constants: profile.constants.clone(),
        user_functions: profile.functions.clone(),
        show_pruned_arith: false,
        symbol_table: Arc::new(symbol_table),
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
pub fn search(target: f64, options: Option<JsValue>) -> Result<Vec<WasmMatch>, JsValue> {
    let opts = parse_search_options(options)?;

    // Use the standard level-to-complexity mapping
    let (max_lhs_complexity, max_rhs_complexity) = crate::search::level_to_complexity(opts.level);

    let mut profile = crate::profile::Profile::new();
    if let Some(preset_name) = opts.preset.as_deref() {
        let parsed = crate::presets::Preset::from_str(preset_name).ok_or_else(|| {
            JsValue::from_str(&format!(
                "Unknown preset '{}'. Use listPresets() for available options.",
                preset_name
            ))
        })?;
        profile = profile.merge(parsed.to_profile());
    }

    // Build generation config
    let gen_config = build_gen_config(max_lhs_complexity, max_rhs_complexity, &profile);

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
        derivative_margin: crate::thresholds::DEGENERATE_DERIVATIVE,
        ranking_mode: crate::pool::RankingMode::Complexity,
    };

    // Perform search: parallel when wasm-threads (wasm-bindgen-rayon), else sequential
    let (matches, _stats) = {
        #[cfg(feature = "wasm-threads")]
        {
            crate::search::search_parallel_with_stats_and_config(&gen_config, &search_config)
        }
        #[cfg(not(feature = "wasm-threads"))]
        {
            crate::search::search_with_stats_and_config(&gen_config, &search_config)
        }
    };

    // Convert to WasmMatch and limit to max_matches
    Ok(matches
        .into_iter()
        .take(opts.max_matches)
        .map(WasmMatch::from)
        .collect())
}

/// Get list of available domain presets
///
/// @returns Object mapping preset names to descriptions
#[wasm_bindgen(js_name = listPresets)]
pub fn list_presets() -> Result<JsValue, JsValue> {
    let presets: std::collections::BTreeMap<String, String> = crate::presets::Preset::all()
        .iter()
        .map(|p| (p.name().to_string(), p.description().to_string()))
        .collect();

    serde_wasm_bindgen::to_value(&presets).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = list_presets)]
pub fn list_presets_compat() -> Result<JsValue, JsValue> {
    list_presets()
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
    // Set up panic hook for better error messages in browser console
    console_error_panic_hook::set_once();
}

// Re-export for threaded WASM build. JS must call initThreadPool(n) after init().
#[cfg(feature = "wasm-threads")]
pub use wasm_bindgen_rayon::init_thread_pool;
