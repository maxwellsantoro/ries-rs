//! Built-in domain presets for common mathematical domains
//!
//! These presets configure symbol sets, weights, and user constants
//! appropriate for different areas of mathematics.

use crate::profile::{Profile, UserConstant};
use crate::symbol::{NumType, Symbol};

/// Available built-in presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Analytic number theory: ζ values, Γ, log, π powers
    AnalyticNT,
    /// Elliptic/modular: K(k), E(k), Γ(1/4), q-series
    Elliptic,
    /// Combinatorics: Catalan, Apéry, polylog patterns
    Combinatorics,
    /// Physics: π, log, γ, ζ, Clausen-type constants
    Physics,
    /// Number theory: rational/algebraic focus
    NumberTheory,
    /// Calculus: standard functions, no exotic constants
    Calculus,
}

impl Preset {
    /// Get preset name for CLI
    pub fn name(&self) -> &'static str {
        match self {
            Preset::AnalyticNT => "analytic-nt",
            Preset::Elliptic => "elliptic",
            Preset::Combinatorics => "combinatorics",
            Preset::Physics => "physics",
            Preset::NumberTheory => "number-theory",
            Preset::Calculus => "calculus",
        }
    }

    /// Get preset description
    pub fn description(&self) -> &'static str {
        match self {
            Preset::AnalyticNT => "Analytic number theory: ζ values, Γ, log, π powers",
            Preset::Elliptic => "Elliptic/modular: K(k), E(k), Γ(1/4), q-series constants",
            Preset::Combinatorics => "Combinatorics: Catalan, Apéry, polylog patterns",
            Preset::Physics => "Physics: π, log, γ, ζ, Clausen-type constants",
            Preset::NumberTheory => "Number theory: rational/algebraic focus",
            Preset::Calculus => "Calculus: standard functions, no exotic constants",
        }
    }

    /// Parse from string (for CLI)
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "analytic-nt" | "ant" | "analytic" => Some(Preset::AnalyticNT),
            "elliptic" | "modular" => Some(Preset::Elliptic),
            "combinatorics" | "combo" | "catalan" => Some(Preset::Combinatorics),
            "physics" | "phys" => Some(Preset::Physics),
            "number-theory" | "nt" | "algebraic" => Some(Preset::NumberTheory),
            "calculus" | "calc" => Some(Preset::Calculus),
            _ => None,
        }
    }

    /// Generate the profile for this preset
    #[allow(clippy::wrong_self_convention)]
    pub fn to_profile(&self) -> Profile {
        let mut profile = Profile::new();

        match self {
            Preset::AnalyticNT => {
                // Analytic number theory uses zeta values, gamma, log, powers of pi
                // Lower weights for: pi, e, log, exp
                // Higher weights for: trig (less common)
                // Add zeta(2) = π²/6, zeta(3), zeta(4) as constants

                profile.symbol_weights = vec![
                    (Symbol::Pi, 6),        // Lower weight - very common
                    (Symbol::Ln, 6),        // Lower weight - very common
                    (Symbol::Exp, 6),       // Lower weight - very common
                    (Symbol::SinPi, 12),    // Higher weight - less common
                    (Symbol::CosPi, 12),    // Higher weight - less common
                    (Symbol::TanPi, 14),    // Higher weight - less common
                    (Symbol::LambertW, 16), // Rarely used
                ]
                .into_iter()
                .collect();

                // Add zeta(2) = π²/6 ≈ 1.644934
                profile.constants.push(UserConstant {
                    weight: 10,
                    name: "z2".to_string(),
                    description: "ζ(2) = π²/6 (Basel problem)".to_string(),
                    value: std::f64::consts::PI * std::f64::consts::PI / 6.0,
                    num_type: NumType::Transcendental,
                });

                // zeta(3) ≈ 1.202057 (Apéry's constant) - already built-in as 'z'
                // But let's ensure it has a good weight
                profile.symbol_weights.insert(Symbol::Apery, 8);

                // Euler-Mascheroni gamma - already built-in as 'g'
                profile.symbol_weights.insert(Symbol::Gamma, 8);
            }

            Preset::Elliptic => {
                // Elliptic integrals, modular forms
                // Focus on: sqrt, powers, log
                // Avoid: trig (elliptic integrals replace them)

                profile.symbol_weights = vec![
                    (Symbol::Sqrt, 4),   // Lower - very common
                    (Symbol::Square, 4), // Lower - very common
                    (Symbol::Ln, 6),     // Common
                    (Symbol::Pi, 6),     // Common
                    (Symbol::SinPi, 16), // Avoid - elliptic replaces
                    (Symbol::CosPi, 16), // Avoid - elliptic replaces
                    (Symbol::TanPi, 18), // Avoid - elliptic replaces
                ]
                .into_iter()
                .collect();

                // Γ(1/4) ≈ 3.6256 - important for elliptic integrals
                profile.constants.push(UserConstant {
                    weight: 12,
                    name: "g14".to_string(),
                    description: "Γ(1/4) - elliptic integral constant".to_string(),
                    value: 3.625609908221908,
                    num_type: NumType::Transcendental,
                });

                // K(1/√2) = Γ²(1/4)/(4√π) ≈ 1.85407
                profile.constants.push(UserConstant {
                    weight: 14,
                    name: "K1".to_string(),
                    description: "K(1/√2) - complete elliptic integral".to_string(),
                    value: 1.854074677301372,
                    num_type: NumType::Transcendental,
                });

                // Golden ratio - already built-in, ensure good weight
                profile.symbol_weights.insert(Symbol::Phi, 8);
            }

            Preset::Combinatorics => {
                // Combinatorics: Catalan, Apéry, polylog patterns
                // Focus on: integers, rationals, simple algebraic

                profile.symbol_weights = vec![
                    (Symbol::One, 2),   // Very common
                    (Symbol::Two, 2),   // Very common
                    (Symbol::Three, 2), // Very common
                    (Symbol::Four, 3),  // Common
                    (Symbol::Five, 3),  // Common
                    (Symbol::Pi, 12),   // Less common
                    (Symbol::E, 12),    // Less common
                    (Symbol::Ln, 14),   // Rare
                    (Symbol::Exp, 14),  // Rare
                ]
                .into_iter()
                .collect();

                // Catalan's constant G ≈ 0.915966 - already built-in
                profile.symbol_weights.insert(Symbol::Catalan, 8);

                // Apéry's constant ζ(3) ≈ 1.202057 - already built-in
                profile.symbol_weights.insert(Symbol::Apery, 8);
            }

            Preset::Physics => {
                // Physics constants: π, log, γ, ζ, combinations
                // Similar to analytic-nt but more focused on practical values

                profile.symbol_weights = vec![
                    (Symbol::Pi, 5),     // Very common
                    (Symbol::E, 6),      // Very common
                    (Symbol::Ln, 6),     // Common
                    (Symbol::Exp, 6),    // Common
                    (Symbol::Sqrt, 5),   // Common
                    (Symbol::SinPi, 10), // Moderate
                    (Symbol::CosPi, 10), // Moderate
                ]
                .into_iter()
                .collect();

                // Euler-Mascheroni gamma
                profile.symbol_weights.insert(Symbol::Gamma, 8);

                // Fine structure constant α ≈ 1/137.036 (inverse)
                // Not adding as it's very specific, but γ is important
            }

            Preset::NumberTheory => {
                // Number theory: focus on rationals, algebraic numbers
                // Avoid transcendentals, focus on integers

                profile.symbol_weights = vec![
                    (Symbol::One, 2),
                    (Symbol::Two, 2),
                    (Symbol::Three, 2),
                    (Symbol::Four, 3),
                    (Symbol::Five, 3),
                    (Symbol::Six, 3),
                    (Symbol::Seven, 3),
                    (Symbol::Eight, 4),
                    (Symbol::Nine, 4),
                    (Symbol::Sqrt, 6),   // Algebraic
                    (Symbol::Square, 6), // Algebraic
                    (Symbol::Pi, 16),    // Avoid transcendental
                    (Symbol::E, 16),     // Avoid transcendental
                    (Symbol::Ln, 18),    // Avoid
                    (Symbol::Exp, 18),   // Avoid
                ]
                .into_iter()
                .collect();

                // Golden ratio - algebraic
                profile.symbol_weights.insert(Symbol::Phi, 8);

                // Plastic constant - algebraic
                profile.symbol_weights.insert(Symbol::Plastic, 10);
            }

            Preset::Calculus => {
                // Standard calculus: all functions available, no exotic constants
                // Balanced weights for general use

                profile.symbol_weights = vec![
                    (Symbol::Pi, 7),
                    (Symbol::E, 7),
                    (Symbol::Ln, 7),
                    (Symbol::Exp, 7),
                    (Symbol::Sqrt, 5),
                    (Symbol::Square, 5),
                    (Symbol::SinPi, 8),
                    (Symbol::CosPi, 8),
                    (Symbol::TanPi, 9),
                ]
                .into_iter()
                .collect();
            }
        }

        profile
    }

    /// List all available presets
    pub fn all() -> &'static [Preset] {
        &[
            Preset::AnalyticNT,
            Preset::Elliptic,
            Preset::Combinatorics,
            Preset::Physics,
            Preset::NumberTheory,
            Preset::Calculus,
        ]
    }
}

/// Print available presets (for --list-presets)
#[allow(clippy::print_literal)]
pub fn print_presets() {
    println!("Available domain presets:");
    println!();
    println!("  {:<15} {}", "PRESET", "DESCRIPTION");
    println!("  {}", "-".repeat(70));
    for preset in Preset::all() {
        println!("  {:<15} {}", preset.name(), preset.description());
    }
    println!();
    println!("Usage: ries-rs --preset <name> <target>");
    println!("Example: ries-rs --preset physics 6.67430e-11");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_parse() {
        assert_eq!(Preset::from_str("analytic-nt"), Some(Preset::AnalyticNT));
        assert_eq!(Preset::from_str("ANT"), Some(Preset::AnalyticNT));
        assert_eq!(Preset::from_str("physics"), Some(Preset::Physics));
        assert_eq!(Preset::from_str("PHYS"), Some(Preset::Physics));
        assert_eq!(Preset::from_str("invalid"), None);
    }

    #[test]
    fn test_preset_profile_non_empty() {
        for preset in Preset::all() {
            let profile = preset.to_profile();
            // Each preset should modify at least something
            assert!(
                !profile.symbol_weights.is_empty() || !profile.constants.is_empty(),
                "Preset {:?} should have some configuration",
                preset
            );
        }
    }

    #[test]
    fn test_analytic_nt_has_gamma() {
        let profile = Preset::AnalyticNT.to_profile();
        assert!(profile.symbol_weights.contains_key(&Symbol::Gamma));
    }

    #[test]
    fn test_number_theory_avoids_transcendentals() {
        let profile = Preset::NumberTheory.to_profile();
        // Pi and E should have high weights (discouraged)
        assert!(profile.symbol_weights.get(&Symbol::Pi) > Some(&10));
        assert!(profile.symbol_weights.get(&Symbol::E) > Some(&10));
    }
}
