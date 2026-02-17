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
//! ## Modules
//!
//! - [`eval`] - Expression evaluation with automatic differentiation
//! - [`expr`] - Expression representation and manipulation
//! - [`gen`] - Expression generation
//! - [`metrics`] - Match scoring and categorization
//! - [`pool`] - Bounded priority pool for match collection
//! - [`profile`] - Profile file support for configuration
//! - [`report`] - Categorized match output
//! - [`search`] - Search algorithms and matching
//! - [`symbol`] - Symbol definitions and type system
//! - [`udf`] - User-defined functions

pub mod eval;
pub mod expr;
pub mod gen;
pub mod metrics;
pub mod pool;
pub mod symbol;
pub mod udf;
pub mod profile;
pub mod report;
pub mod search;

pub use expr::OutputFormat;
pub use profile::{Profile, UserConstant};
pub use search::{Match, SearchConfig, SearchStats};
pub use udf::UserFunction;
