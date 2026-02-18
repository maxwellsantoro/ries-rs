//! Report generation for categorized match output
//!
//! Selects top-K matches per category and formats output.

use crate::expr::{Expression, OutputFormat};
use crate::metrics::{MatchMetrics, OperatorFrequency};
use crate::pool::{LhsKey, SignatureKey};
use crate::search::Match;
use crate::symbol::Symbol;
use std::collections::HashSet;

/// Display format for expressions (matches main.rs DisplayFormat)
#[derive(Debug, Clone, Copy)]
pub enum DisplayFormat {
    /// Infix with optional format variant
    Infix(OutputFormat),
    /// Compact postfix (like "52/")
    PostfixCompact,
    /// Verbose postfix (like "5 2 /")
    PostfixVerbose,
    /// Alias for PostfixCompact (-F1)
    Condensed,
}

/// Format an expression for display using the specified format
fn format_expression_for_display(expression: &Expression, format: DisplayFormat) -> String {
    match format {
        DisplayFormat::Infix(inner) => expression.to_infix_with_format(inner),
        DisplayFormat::PostfixCompact | DisplayFormat::Condensed => expression.to_postfix(),
        DisplayFormat::PostfixVerbose => expression
            .symbols()
            .iter()
            .map(|sym| postfix_verbose_token(*sym))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn postfix_verbose_token(sym: Symbol) -> String {
    use Symbol;
    match sym {
        Symbol::Neg => "neg".to_string(),
        Symbol::Recip => "recip".to_string(),
        Symbol::Sqrt => "sqrt".to_string(),
        Symbol::Square => "dup*".to_string(),
        Symbol::Pow => "**".to_string(),
        Symbol::Root => "root".to_string(),
        Symbol::Log => "logn".to_string(),
        Symbol::Exp => "exp".to_string(),
        _ => sym.display_name(),
    }
}

/// Report categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Exact matches (error < 1e-14)
    Exact,
    /// Best approximations (lowest error)
    Best,
    /// Elegant/efficient (lowest complexity)
    Elegant,
    /// Interesting/unexpected (high novelty)
    Interesting,
    /// Stable/robust (good conditioning)
    Stable,
}

impl Category {
    pub fn name(&self) -> &'static str {
        match self {
            Category::Exact => "Exact matches",
            Category::Best => "Best approximations",
            Category::Elegant => "Elegant/efficient",
            Category::Interesting => "Interesting/unexpected",
            Category::Stable => "Stable/robust",
        }
    }

    /// Get the description for this category
    ///
    /// This method is part of the public API for library consumers who want
    /// to display category descriptions in their output formatting.
    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            Category::Exact => "Equations that hold exactly at the target value",
            Category::Best => "Closest approximations to the target",
            Category::Elegant => "Simplest expressions with good accuracy",
            Category::Interesting => "Novel or unusual equation structures",
            Category::Stable => "Matches with robust numerical properties",
        }
    }
}

/// A categorized report of matches
pub struct Report {
    /// Top matches per category
    pub categories: Vec<(Category, Vec<MatchWithMetrics>)>,
    /// Target value
    pub target: f64,
}

/// Match with computed metrics
pub struct MatchWithMetrics {
    pub m: Match,
    pub metrics: MatchMetrics,
}

/// Configuration for report generation
#[derive(Clone)]
pub struct ReportConfig {
    /// Number of matches per category
    pub top_k: usize,
    /// Which categories to include
    pub categories: Vec<Category>,
    /// Error cap for "interesting" category
    pub interesting_error_cap: f64,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            top_k: 8,
            categories: vec![
                Category::Exact,
                Category::Best,
                Category::Elegant,
                Category::Interesting,
                Category::Stable,
            ],
            interesting_error_cap: 1e-6,
        }
    }
}

impl ReportConfig {
    /// Create config with all categories (including stable)
    ///
    /// This method is part of the public API for library consumers who want
    /// to ensure the stability category is included in their reports.
    #[allow(dead_code)]
    pub fn with_stable(mut self) -> Self {
        if !self.categories.contains(&Category::Stable) {
            self.categories.push(Category::Stable);
        }
        self
    }

    /// Remove stability category
    pub fn without_stable(mut self) -> Self {
        self.categories.retain(|c| *c != Category::Stable);
        self
    }

    /// Set top-K
    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }

    /// Set interesting error cap based on target
    pub fn with_target(mut self, target: f64) -> Self {
        // Scale error cap with target magnitude
        self.interesting_error_cap = (1e-8_f64).max(1e-6 * target.abs());
        self
    }
}

impl Report {
    /// Generate a report from a pool of matches
    pub fn generate(matches: Vec<Match>, target: f64, config: &ReportConfig) -> Self {
        // Build frequency map for novelty scoring
        let mut freq_map = OperatorFrequency::new();
        for m in &matches {
            freq_map.add(m);
        }

        // Compute metrics for all matches
        let mut with_metrics: Vec<MatchWithMetrics> = matches
            .into_iter()
            .map(|m| {
                let metrics = MatchMetrics::from_match(&m, Some(&freq_map));
                MatchWithMetrics { m, metrics }
            })
            .collect();

        // Generate each category
        let mut categories = Vec::new();

        for &cat in &config.categories {
            let selected = select_category(&mut with_metrics, cat, config);
            categories.push((cat, selected));
        }

        Report { categories, target }
    }

    /// Print the report to stdout
    pub fn print(&self, absolute: bool, solve: bool, format: DisplayFormat) {
        for (category, matches) in &self.categories {
            if matches.is_empty() {
                continue;
            }

            println!();
            println!("  -- {} ({}) --", category.name(), matches.len());
            println!();

            for mwm in matches {
                print_match(&mwm.m, &mwm.metrics, self.target, absolute, solve, format);
            }
        }
    }
}

/// Select top-K matches for a category
fn select_category(
    matches: &mut [MatchWithMetrics],
    category: Category,
    config: &ReportConfig,
) -> Vec<MatchWithMetrics> {
    // Filter and sort based on category
    let mut candidates: Vec<_> = matches.iter().collect();

    // Filter
    candidates.retain(|mwm| category_filter(mwm, category, config));

    // Sort (best first)
    candidates.sort_by(|a, b| category_compare(a, b, category, config));

    // Dedupe based on category
    let mut result = Vec::new();
    let mut seen_lhs: HashSet<LhsKey> = HashSet::new();
    let mut seen_sig: HashSet<SignatureKey> = HashSet::new();

    for mwm in candidates {
        if result.len() >= config.top_k {
            break;
        }

        // Category-specific dedupe
        let accept = match category {
            Category::Exact => {
                // Dedupe by full equation (allow multiple forms)
                true
            }
            Category::Best | Category::Elegant => {
                // Dedupe by LHS (one match per LHS)
                let lhs_key = LhsKey::from_match(&mwm.m);
                if seen_lhs.contains(&lhs_key) {
                    false
                } else {
                    seen_lhs.insert(lhs_key);
                    true
                }
            }
            Category::Interesting => {
                // Dedupe by signature (force variety)
                let sig_key = SignatureKey::from_match(&mwm.m);
                if seen_sig.contains(&sig_key) {
                    false
                } else {
                    seen_sig.insert(sig_key);
                    true
                }
            }
            Category::Stable => {
                // Dedupe by LHS
                let lhs_key = LhsKey::from_match(&mwm.m);
                if seen_lhs.contains(&lhs_key) {
                    false
                } else {
                    seen_lhs.insert(lhs_key);
                    true
                }
            }
        };

        if accept {
            result.push(mwm.clone());
        }
    }

    result
}

/// Filter for category membership
fn category_filter(mwm: &MatchWithMetrics, category: Category, config: &ReportConfig) -> bool {
    match category {
        Category::Exact => mwm.metrics.is_exact,
        Category::Best => !mwm.metrics.is_exact, // Non-exact only (exact are in Exact)
        Category::Elegant => true,               // All matches eligible
        Category::Interesting => {
            mwm.metrics.error <= config.interesting_error_cap && !mwm.metrics.is_exact
        }
        Category::Stable => mwm.metrics.stability > 0.3, // Reasonable conditioning
    }
}

/// Compare for category ranking (return Ordering for sort)
fn category_compare(
    a: &MatchWithMetrics,
    b: &MatchWithMetrics,
    category: Category,
    config: &ReportConfig,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match category {
        Category::Exact => {
            // Sort by complexity, then by equation length (shorter first)
            a.metrics
                .complexity
                .cmp(&b.metrics.complexity)
                .then_with(|| {
                    (a.m.lhs.expr.len() + a.m.rhs.expr.len())
                        .cmp(&(b.m.lhs.expr.len() + b.m.rhs.expr.len()))
                })
        }
        Category::Best => {
            // Sort by error (lower first)
            a.metrics
                .error
                .partial_cmp(&b.metrics.error)
                .unwrap_or(Ordering::Equal)
        }
        Category::Elegant => {
            // Sort by elegant score (lower first)
            a.metrics
                .elegant_score()
                .partial_cmp(&b.metrics.elegant_score())
                .unwrap_or(Ordering::Equal)
        }
        Category::Interesting => {
            // Sort by interesting score (higher first)
            b.metrics
                .interesting_score(config.interesting_error_cap)
                .partial_cmp(&a.metrics.interesting_score(config.interesting_error_cap))
                .unwrap_or(Ordering::Equal)
        }
        Category::Stable => {
            // Sort by stability score (higher first), then by error
            b.metrics
                .stable_score()
                .partial_cmp(&a.metrics.stable_score())
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    a.metrics
                        .error
                        .partial_cmp(&b.metrics.error)
                        .unwrap_or(Ordering::Equal)
                })
        }
    }
}

/// Clone implementation for MatchWithMetrics
impl Clone for MatchWithMetrics {
    fn clone(&self) -> Self {
        Self {
            m: self.m.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

/// Print a single match
fn print_match(
    m: &Match,
    metrics: &MatchMetrics,
    _target: f64,
    absolute: bool,
    solve: bool,
    format: DisplayFormat,
) {
    let lhs_str = format_expression_for_display(&m.lhs.expr, format);
    let rhs_str = format_expression_for_display(&m.rhs.expr, format);

    let error_str = if metrics.is_exact {
        "('exact' match)".to_string()
    } else if absolute {
        format!("for x = {:.15}", m.x_value)
    } else {
        let sign = if m.error >= 0.0 { "+" } else { "-" };
        format!("for x = T {} {:.6e}", sign, m.error.abs())
    };

    // Compact info string
    let info = format!("{{{}}}", m.complexity);

    if solve {
        println!("     x = {:40} {} {}", rhs_str, error_str, info);
    } else {
        println!("{:>24} = {:<24} {} {}", lhs_str, rhs_str, error_str, info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{EvaluatedExpr, Expression};
    use crate::symbol::NumType;

    fn make_match(lhs: &str, rhs: &str, error: f64) -> Match {
        let lhs_expr = Expression::parse(lhs).unwrap();
        let rhs_expr = Expression::parse(rhs).unwrap();
        Match {
            lhs: EvaluatedExpr::new(lhs_expr.clone(), 0.0, 1.0, NumType::Integer),
            rhs: EvaluatedExpr::new(rhs_expr.clone(), 0.0, 0.0, NumType::Integer),
            x_value: 2.5,
            error,
            complexity: lhs_expr.complexity() + rhs_expr.complexity(),
        }
    }

    #[test]
    fn test_report_generation() {
        let matches = vec![
            make_match("2x*", "5", 0.0),      // Exact
            make_match("xx^", "ps", 0.00066), // Interesting
            make_match("x1+", "35/", 1e-10),  // Best approx
        ];

        let config = ReportConfig::default().with_target(2.5);
        let report = Report::generate(matches, 2.5, &config);

        // Should have entries in multiple categories
        assert!(!report.categories.is_empty());
    }
}
