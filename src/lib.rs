//! RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES program.
//!
//! # Modules
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
