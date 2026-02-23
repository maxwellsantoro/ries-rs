//! Per-run symbol configuration table
//!
//! This module provides `SymbolTable`, an immutable configuration container that
//! stores symbol weights and display names for a single search run. This replaces
//! the process-global mutable state pattern with per-run configuration, enabling:
//!
//! - Concurrent searches with different profiles
//! - Library usage without side effects
//! - Reproducible results regardless of process state
//!
//! # Thread Safety
//!
//! `SymbolTable` is immutable after construction and can be freely shared across
//! threads via `Arc<SymbolTable>`. Each search run should construct its own table
//! from the relevant profile.

use std::sync::Arc;

use crate::profile::{Profile, UserConstant};
use crate::symbol::Symbol;
use crate::udf::UserFunction;

/// Number of possible symbol values (u8 range)
const SYMBOL_COUNT: usize = 256;

/// Immutable per-run symbol configuration
///
/// Stores weights and display names for all symbols used in a search.
/// Built from a profile and user-defined constants/functions, then
/// passed through the search pipeline for consistent behavior.
#[derive(Clone, Debug)]
pub struct SymbolTable {
    /// Complexity weights for each symbol (indexed by symbol byte value)
    weights: [u32; SYMBOL_COUNT],
    /// Display names for each symbol (indexed by symbol byte value)
    names: [String; SYMBOL_COUNT],
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    /// Create a new symbol table with default weights and names
    pub fn new() -> Self {
        let mut weights = [0u32; SYMBOL_COUNT];
        let mut names: [String; SYMBOL_COUNT] = std::array::from_fn(|_| String::new());

        // Initialize all symbols with their default values
        for &sym in Symbol::constants()
            .iter()
            .chain(Symbol::unary_ops().iter())
            .chain(Symbol::binary_ops().iter())
        {
            let idx = sym as usize;
            weights[idx] = sym.default_weight();
            names[idx] = sym.name().to_string();
        }

        // Initialize user constant placeholders
        for (i, sym) in [
            Symbol::UserConstant0,
            Symbol::UserConstant1,
            Symbol::UserConstant2,
            Symbol::UserConstant3,
            Symbol::UserConstant4,
            Symbol::UserConstant5,
            Symbol::UserConstant6,
            Symbol::UserConstant7,
            Symbol::UserConstant8,
            Symbol::UserConstant9,
            Symbol::UserConstant10,
            Symbol::UserConstant11,
            Symbol::UserConstant12,
            Symbol::UserConstant13,
            Symbol::UserConstant14,
            Symbol::UserConstant15,
        ]
        .iter()
        .enumerate()
        {
            let idx = *sym as usize;
            weights[idx] = (*sym).default_weight();
            names[idx] = format!("u{}", i);
        }

        // Initialize user function placeholders
        for (i, sym) in [
            Symbol::UserFunction0,
            Symbol::UserFunction1,
            Symbol::UserFunction2,
            Symbol::UserFunction3,
            Symbol::UserFunction4,
            Symbol::UserFunction5,
            Symbol::UserFunction6,
            Symbol::UserFunction7,
            Symbol::UserFunction8,
            Symbol::UserFunction9,
            Symbol::UserFunction10,
            Symbol::UserFunction11,
            Symbol::UserFunction12,
            Symbol::UserFunction13,
            Symbol::UserFunction14,
            Symbol::UserFunction15,
        ]
        .iter()
        .enumerate()
        {
            let idx = *sym as usize;
            weights[idx] = (*sym).default_weight();
            names[idx] = format!("f{}", i);
        }

        // Initialize X (variable) - not included in constants()
        weights[Symbol::X as usize] = Symbol::X.default_weight();
        names[Symbol::X as usize] = Symbol::X.name().to_string();

        Self { weights, names }
    }

    /// Build a symbol table from a profile
    ///
    /// Applies:
    /// - Profile weight overrides
    /// - Profile name overrides
    /// - User constant names and weights
    /// - User function names and weights
    pub fn from_profile(profile: &Profile) -> Self {
        let mut table = Self::new();

        // Apply profile weight overrides
        for (&sym, &weight) in &profile.symbol_weights {
            let idx = sym as usize;
            if idx < SYMBOL_COUNT {
                table.weights[idx] = weight;
            }
        }

        // Apply profile name overrides
        for (&sym, name) in &profile.symbol_names {
            let idx = sym as usize;
            if idx < SYMBOL_COUNT {
                table.names[idx] = name.clone();
            }
        }

        // Apply user constant names and weights
        for (i, uc) in profile.constants.iter().enumerate() {
            if i >= 16 {
                break;
            }
            let sym = match i {
                0 => Symbol::UserConstant0,
                1 => Symbol::UserConstant1,
                2 => Symbol::UserConstant2,
                3 => Symbol::UserConstant3,
                4 => Symbol::UserConstant4,
                5 => Symbol::UserConstant5,
                6 => Symbol::UserConstant6,
                7 => Symbol::UserConstant7,
                8 => Symbol::UserConstant8,
                9 => Symbol::UserConstant9,
                10 => Symbol::UserConstant10,
                11 => Symbol::UserConstant11,
                12 => Symbol::UserConstant12,
                13 => Symbol::UserConstant13,
                14 => Symbol::UserConstant14,
                15 => Symbol::UserConstant15,
                _ => continue,
            };
            let idx = sym as usize;
            table.weights[idx] = uc.weight;
            table.names[idx] = uc.name.clone();
        }

        // Apply user function names and weights
        for (i, uf) in profile.functions.iter().enumerate() {
            if i >= 16 {
                break;
            }
            let sym = match i {
                0 => Symbol::UserFunction0,
                1 => Symbol::UserFunction1,
                2 => Symbol::UserFunction2,
                3 => Symbol::UserFunction3,
                4 => Symbol::UserFunction4,
                5 => Symbol::UserFunction5,
                6 => Symbol::UserFunction6,
                7 => Symbol::UserFunction7,
                8 => Symbol::UserFunction8,
                9 => Symbol::UserFunction9,
                10 => Symbol::UserFunction10,
                11 => Symbol::UserFunction11,
                12 => Symbol::UserFunction12,
                13 => Symbol::UserFunction13,
                14 => Symbol::UserFunction14,
                15 => Symbol::UserFunction15,
                _ => continue,
            };
            let idx = sym as usize;
            table.weights[idx] = uf.weight as u32;
            table.names[idx] = uf.name.clone();
        }

        table
    }

    /// Build from profile with explicit user constants and functions
    ///
    /// This is useful when user constants/functions come from CLI args
    /// rather than a profile file.
    pub fn from_parts(
        profile: &Profile,
        user_constants: &[UserConstant],
        user_functions: &[UserFunction],
    ) -> Self {
        let mut table = Self::new();

        // Apply profile weight overrides
        for (&sym, &weight) in &profile.symbol_weights {
            let idx = sym as usize;
            if idx < SYMBOL_COUNT {
                table.weights[idx] = weight;
            }
        }

        // Apply profile name overrides
        for (&sym, name) in &profile.symbol_names {
            let idx = sym as usize;
            if idx < SYMBOL_COUNT {
                table.names[idx] = name.clone();
            }
        }

        // Apply user constant names and weights
        for (i, uc) in user_constants.iter().enumerate() {
            if i >= 16 {
                break;
            }
            let sym = user_constant_symbol(i);
            let idx = sym as usize;
            table.weights[idx] = uc.weight;
            table.names[idx] = uc.name.clone();
        }

        // Apply user function names and weights
        for (i, uf) in user_functions.iter().enumerate() {
            if i >= 16 {
                break;
            }
            let sym = user_function_symbol(i);
            let idx = sym as usize;
            table.weights[idx] = uf.weight as u32;
            table.names[idx] = uf.name.clone();
        }

        table
    }

    /// Get the weight for a symbol
    #[inline]
    pub fn weight(&self, sym: Symbol) -> u32 {
        self.weights[sym as usize]
    }

    /// Get the display name for a symbol
    #[inline]
    pub fn name(&self, sym: Symbol) -> &str {
        &self.names[sym as usize]
    }

    /// Wrap this table in an Arc for sharing
    pub fn into_shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

/// Get the user constant symbol for an index (0-15)
///
/// # Panics
///
/// Panics if index >= 16. Use `user_constant_symbol_opt` for a non-panicking version.
#[inline]
pub fn user_constant_symbol(index: usize) -> Symbol {
    user_constant_symbol_opt(index)
        .unwrap_or_else(|| panic!("User constant index out of bounds: {}", index))
}

/// Get the user constant symbol for an index (0-15), returning None if out of bounds
#[inline]
pub fn user_constant_symbol_opt(index: usize) -> Option<Symbol> {
    match index {
        0 => Some(Symbol::UserConstant0),
        1 => Some(Symbol::UserConstant1),
        2 => Some(Symbol::UserConstant2),
        3 => Some(Symbol::UserConstant3),
        4 => Some(Symbol::UserConstant4),
        5 => Some(Symbol::UserConstant5),
        6 => Some(Symbol::UserConstant6),
        7 => Some(Symbol::UserConstant7),
        8 => Some(Symbol::UserConstant8),
        9 => Some(Symbol::UserConstant9),
        10 => Some(Symbol::UserConstant10),
        11 => Some(Symbol::UserConstant11),
        12 => Some(Symbol::UserConstant12),
        13 => Some(Symbol::UserConstant13),
        14 => Some(Symbol::UserConstant14),
        15 => Some(Symbol::UserConstant15),
        _ => None,
    }
}

/// Get the user function symbol for an index (0-15)
///
/// # Panics
///
/// Panics if index >= 16. Use `user_function_symbol_opt` for a non-panicking version.
#[inline]
pub fn user_function_symbol(index: usize) -> Symbol {
    user_function_symbol_opt(index)
        .unwrap_or_else(|| panic!("User function index out of bounds: {}", index))
}

/// Get the user function symbol for an index (0-15), returning None if out of bounds
#[inline]
pub fn user_function_symbol_opt(index: usize) -> Option<Symbol> {
    match index {
        0 => Some(Symbol::UserFunction0),
        1 => Some(Symbol::UserFunction1),
        2 => Some(Symbol::UserFunction2),
        3 => Some(Symbol::UserFunction3),
        4 => Some(Symbol::UserFunction4),
        5 => Some(Symbol::UserFunction5),
        6 => Some(Symbol::UserFunction6),
        7 => Some(Symbol::UserFunction7),
        8 => Some(Symbol::UserFunction8),
        9 => Some(Symbol::UserFunction9),
        10 => Some(Symbol::UserFunction10),
        11 => Some(Symbol::UserFunction11),
        12 => Some(Symbol::UserFunction12),
        13 => Some(Symbol::UserFunction13),
        14 => Some(Symbol::UserFunction14),
        15 => Some(Symbol::UserFunction15),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_table() {
        let table = SymbolTable::new();

        // Check some default weights (matching original RIES calibration)
        assert_eq!(table.weight(Symbol::One), 10);
        assert_eq!(table.weight(Symbol::Pi), 14);
        assert_eq!(table.weight(Symbol::Add), 4);

        // Check some default names
        assert_eq!(table.name(Symbol::One), "1");
        assert_eq!(table.name(Symbol::Pi), "pi");
        assert_eq!(table.name(Symbol::Add), "+");
    }

    #[test]
    fn test_profile_overrides() {
        let mut profile = Profile::new();
        profile.symbol_weights.insert(Symbol::Pi, 20);
        profile.symbol_names.insert(Symbol::Pi, "π".to_string());

        let table = SymbolTable::from_profile(&profile);

        assert_eq!(table.weight(Symbol::Pi), 20);
        assert_eq!(table.name(Symbol::Pi), "π");
    }

    #[test]
    fn test_user_constant_overrides() {
        let mut profile = Profile::new();
        profile.constants.push(UserConstant {
            weight: 15,
            name: "myconst".to_string(),
            description: "My constant".to_string(),
            value: 1.234,
            num_type: crate::symbol::NumType::Transcendental,
        });

        let table = SymbolTable::from_profile(&profile);

        assert_eq!(table.weight(Symbol::UserConstant0), 15);
        assert_eq!(table.name(Symbol::UserConstant0), "myconst");
    }

    #[test]
    fn test_concurrent_tables_dont_interfere() {
        // Create two tables with different configurations
        let mut profile1 = Profile::new();
        profile1
            .symbol_names
            .insert(Symbol::Pi, "pi_one".to_string());

        let mut profile2 = Profile::new();
        profile2
            .symbol_names
            .insert(Symbol::Pi, "pi_two".to_string());

        let table1 = SymbolTable::from_profile(&profile1);
        let table2 = SymbolTable::from_profile(&profile2);

        // Verify they have different names for Pi
        assert_eq!(table1.name(Symbol::Pi), "pi_one");
        assert_eq!(table2.name(Symbol::Pi), "pi_two");

        // Verify they still work independently
        assert_eq!(table1.name(Symbol::E), "e");
        assert_eq!(table2.name(Symbol::E), "e");
    }

    #[test]
    fn test_shared_table() {
        let table = SymbolTable::new().into_shared();

        // Can clone Arc cheaply
        let table2 = Arc::clone(&table);

        assert_eq!(table.weight(Symbol::One), table2.weight(Symbol::One));
        assert_eq!(table.name(Symbol::Pi), table2.name(Symbol::Pi));
    }

    #[test]
    fn test_expression_formatting_with_different_tables() {
        use crate::expr::Expression;

        // Create two tables with different names for pi
        let mut profile2 = Profile::new();
        profile2.symbol_names.insert(Symbol::Pi, "PI".to_string());

        let table1 = SymbolTable::new();
        let table2 = SymbolTable::from_profile(&profile2);

        // Build expression using table1: x + pi (postfix: X Pi Add)
        let mut expr = Expression::new();
        expr.push_with_table(Symbol::X, &table1);
        expr.push_with_table(Symbol::Pi, &table1);
        expr.push_with_table(Symbol::Add, &table1);

        // Format with different tables - the key insight is that the same expression
        // can be formatted differently based on the table used
        let formatted1 = expr.to_infix_with_table(&table1);
        let formatted2 = expr.to_infix_with_table(&table2);

        // With default table, pi is "pi"
        // With table2, pi is "PI"
        assert!(formatted1.contains("pi") || formatted1.contains("x"));
        assert!(formatted2.contains("PI"));

        // Verify the tables are independent - no global state pollution
        assert_ne!(formatted1, formatted2);
    }

    #[test]
    fn test_complexity_with_different_tables() {
        use crate::expr::Expression;

        // Create two tables with different weights for Pi
        let mut profile2 = Profile::new();
        profile2.symbol_weights.insert(Symbol::Pi, 20); // Heavier weight

        let table1 = SymbolTable::new(); // Default weights
        let table2 = SymbolTable::from_profile(&profile2);

        // Verify the tables have different weights for Pi
        assert_eq!(table1.weight(Symbol::Pi), 14); // default = original RIES value
        assert_eq!(table2.weight(Symbol::Pi), 20); // overridden

        // Build expressions using each table
        let mut expr1 = Expression::new();
        expr1.push_with_table(Symbol::X, &table1); // 15
        expr1.push_with_table(Symbol::Pi, &table1); // 14
        expr1.push_with_table(Symbol::Add, &table1); // 4
                                                     // Total: 15 + 14 + 4 = 33

        let mut expr2 = Expression::new();
        expr2.push_with_table(Symbol::X, &table2); // 15
        expr2.push_with_table(Symbol::Pi, &table2); // 20
        expr2.push_with_table(Symbol::Add, &table2); // 4
                                                     // Total: 15 + 20 + 4 = 39

        // Same symbols, different complexity due to different tables
        assert_eq!(expr1.complexity(), 33);
        assert_eq!(expr2.complexity(), 39);
    }

    #[test]
    fn test_user_constant_symbol_out_of_bounds() {
        // Test that out-of-bounds indices return None instead of panicking
        let result = user_constant_symbol_opt(16);
        assert!(result.is_none(), "Index 16 should return None");

        let result = user_constant_symbol_opt(100);
        assert!(result.is_none(), "Index 100 should return None");

        // Valid indices should work
        let result = user_constant_symbol_opt(0);
        assert_eq!(result, Some(Symbol::UserConstant0));

        let result = user_constant_symbol_opt(15);
        assert_eq!(result, Some(Symbol::UserConstant15));
    }

    #[test]
    fn test_user_function_symbol_out_of_bounds() {
        // Test that out-of-bounds indices return None instead of panicking
        let result = user_function_symbol_opt(16);
        assert!(result.is_none(), "Index 16 should return None");

        let result = user_function_symbol_opt(100);
        assert!(result.is_none(), "Index 100 should return None");

        // Valid indices should work
        let result = user_function_symbol_opt(0);
        assert_eq!(result, Some(Symbol::UserFunction0));

        let result = user_function_symbol_opt(15);
        assert_eq!(result, Some(Symbol::UserFunction15));
    }
}
