//! CLI module for RIES (Rust Implementation of Equation Solver)
//!
//! This module contains all command-line interface handling, including:
//! - Argument parsing with clap
//! - Output formatting and display
//! - Diagnostics flag handling
//!
//! # Structure
//!
//! - [`args`] - Command-line argument definitions and parsing helpers
//! - [`output`] - Output formatting functions for matches and expressions
//! - [`diagnostics`] - `-D` flag handling for diagnostic output channels

pub mod args;
pub mod diagnostics;
pub mod output;

// Re-export the main public API
pub use args::{
    canon_reduction_enabled, parse_memory_size_bytes, parse_symbol_count_limits,
    parse_symbol_names_from_cli, parse_symbol_sets, parse_symbol_weights_from_cli,
    parse_user_constant_from_cli, parse_user_function_from_cli, print_symbol_table, Args,
};

pub use diagnostics::parse_diagnostics;

pub use output::{
    compute_significant_digits_tolerance, format_value, parse_display_format, print_footer,
    print_header, print_match_absolute, print_match_relative, print_show_work_details,
    DisplayFormat,
};
