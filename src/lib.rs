//! # RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES (RILYBOT Inverse Equation Solver).
//!
//! Given a numeric target value, RIES searches for algebraic equations
//! that have the target as a solution. For example, given π, it finds
//! equations like `x = π`, `x² = 10`, `sin(πx) = 0`, etc.
//!
//! ## Features
//!
//! - **Parallel search** using Rayon for multi-core speedup
//! - **Automatic differentiation** for Newton-Raphson refinement
//! - **User-defined constants and functions** via profiles
//! - **Multiple output formats**: default, pretty (Unicode), Mathematica, SymPy
//! - **Complexity scoring** to find simplest equations first
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use ries_rs::{search, gen::GenConfig};
//!
//! let config = GenConfig::default();
//! let matches = search(2.5, &config, 10);
//!
//! for m in &matches {
//!     println!("{} = {}", m.lhs.expr, m.rhs.expr);
//! }
//! ```
//!
//! ## Command-Line Usage

// Allow field reassignment with default in test code - common pattern for config building
#![cfg_attr(test, allow(clippy::field_reassign_with_default))]
//!
//! ```bash
//! # Find equations for π
//! ries-rs 3.141592653589793
//!
//! # Higher search level (more results)
//! ries-rs 2.5 -l 5
//!
//! # Restrict to algebraic solutions
//! ries-rs 1.41421356 -a
//! ```
//!
//! ## API Levels
//!
//! The library provides three API levels:
//!
//! ### High-Level API
//!
//! Simple functions for common use cases:
//! - [`search()`] - Find equations for a target value
//!
//! ### Mid-Level API
//!
//! Configuration and control structures:
//! - [`GenConfig`](gen::GenConfig) - Configure expression generation
//! - [`SearchConfig`](search::SearchConfig) - Configure search behavior
//! - [`Match`](search::Match) - A matched equation
//!
//! ### Low-Level API
//!
//! Building blocks for custom implementations:
//! - [`Expression`](expr::Expression) - Symbolic expression representation
//! - [`Symbol`](symbol::Symbol) - Individual symbols (constants, operators)
//! - [`evaluate()`](eval::evaluate) - Evaluate expressions with derivatives
//!
//! ## Modules
//!
//! - [`eval`] - Expression evaluation with automatic differentiation
//! - [`expr`] - Expression representation and manipulation
//! - [`gen`] - Expression generation
//! - [`metrics`] - Match scoring and categorization
//! - [`pool`] - Bounded priority pool for match collection
//! - [`precision`] - Precision abstraction for numeric types
//! - [`profile`] - Profile file support for configuration
//! - [`report`] - Categorized match output
//! - [`search`] - Search algorithms and matching
//! - [`symbol`] - Symbol definitions and type system
//! - [`thresholds`] - Named threshold constants
//! - [`udf`] - User-defined functions

pub mod eval;
pub mod expr;
pub mod fast_match;
pub mod gen;
pub mod highprec_verify;
pub mod manifest;
pub mod metrics;
pub mod pool;
#[cfg(feature = "highprec")]
pub mod precision;
pub mod presets;
pub mod profile;
pub mod report;
pub mod search;
pub mod stability;
pub mod symbol;
pub mod thresholds;
pub mod udf;

// =============================================================================
// Type Aliases
// =============================================================================

/// Type alias for complexity scores
///
/// Complexity scores measure how "simple" an expression is.
/// Lower values indicate simpler expressions that will be shown first.
///
/// Uses `u32` to allow for very long expressions without overflow risk,
/// though practical expressions typically have complexity < 500.
pub type Complexity = u32;

// =============================================================================
// Re-exports for convenience
// =============================================================================

// High-level API
pub use search::search;

// Fast exact match detection
pub use fast_match::find_fast_match;

// Common types
pub use eval::{EvalError, EvalResult};
pub use expr::{Expression, OutputFormat};
pub use gen::GenConfig;
pub use profile::{Profile, UserConstant};
pub use search::{Match, SearchConfig, SearchStats};
pub use symbol::{set_weight_overrides, NumType, Symbol};
pub use udf::UserFunction;

// Threshold constants
pub use thresholds::{DEGENERATE_DERIVATIVE, EXACT_MATCH_TOLERANCE, NEWTON_TOLERANCE};

// High-precision types (when feature is enabled)
#[cfg(feature = "highprec")]
pub use precision::{HighPrec, RiesFloat, DEFAULT_PRECISION};

// Manifest types for reproducibility
pub use manifest::{MatchInfo, RunManifest, SearchConfigInfo};
