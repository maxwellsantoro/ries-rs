//! Report generation for categorized match output
//!
//! Selects top-K matches per category and formats output.

use crate::eval::EvalContext;
use crate::expr::{Expression, OutputFormat};
use crate::metrics::{MatchMetrics, OperatorFrequency};
use crate::pool::{compare_matches, LhsKey, RankingMode, SignatureKey};
use crate::search::Match;
use crate::symbol::Symbol;
use crate::symbol_table::SymbolTable;
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

impl DisplayFormat {
    /// Format an equation side using the active symbol names and trig scale.
    pub fn format_expression(
        self,
        expression: &Expression,
        explicit_multiply: bool,
        table: Option<&SymbolTable>,
        trig_scale: f64,
    ) -> String {
        match self {
            Self::Infix(format) => {
                let infix = expression.to_infix_with_context(format, table, trig_scale);
                if explicit_multiply {
                    apply_explicit_multiply(&infix)
                } else {
                    infix
                }
            }
            Self::PostfixCompact | Self::Condensed => expression.to_postfix(),
            Self::PostfixVerbose => expression
                .symbols()
                .iter()
                .map(|&symbol| {
                    if symbol.user_constant_index().is_some()
                        || symbol.user_function_index().is_some()
                    {
                        if let Some(table) = table {
                            return table.name(symbol).to_owned();
                        }
                    }
                    postfix_verbose_token(symbol)
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

/// Apply explicit '*' multiplication operators to infix output.
///
/// Converts implicit multiplication (e.g., "2 x") to explicit form (e.g., "2*x").
fn apply_explicit_multiply(infix: &str) -> String {
    let chars: Vec<char> = infix.chars().collect();
    let mut out = String::with_capacity(infix.len() + 8);
    for i in 0..chars.len() {
        let ch = chars[i];
        if ch != ' ' {
            out.push(ch);
            continue;
        }

        let prev = i.checked_sub(1).and_then(|idx| chars.get(idx).copied());
        let next = chars.get(i + 1).copied();
        let implicit_mul = prev.is_some_and(|c| c.is_ascii_digit() || c == ')')
            && next.is_some_and(|c| c.is_ascii_alphabetic() || c == '(');
        if implicit_mul {
            out.push('*');
        } else {
            out.push(' ');
        }
    }
    out
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
    /// Exact matches at the typed target
    Exact,
    /// Best approximations (lowest error bucket)
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
        let with_metrics: Vec<MatchWithMetrics> = matches
            .into_iter()
            .map(|m| {
                let metrics = MatchMetrics::from_match(&m, Some(&freq_map));
                MatchWithMetrics { m, metrics }
            })
            .collect();

        // Generate each category
        let mut categories = Vec::new();

        for &cat in &config.categories {
            let selected = select_category(&with_metrics, cat, config);
            categories.push((cat, selected));
        }

        Report { categories, target }
    }

    /// Print the report to stdout
    pub fn print(&self, absolute: bool, solve: bool, format: DisplayFormat) {
        self.print_with_context(absolute, solve, format, false, None, &EvalContext::new());
    }

    /// Print using the search's expression names, trig scale and solver context.
    pub fn print_with_context(
        &self,
        absolute: bool,
        solve: bool,
        format: DisplayFormat,
        explicit_multiply: bool,
        table: Option<&SymbolTable>,
        context: &EvalContext<'_>,
    ) {
        let render = |expression: &Expression| {
            format.format_expression(
                expression,
                explicit_multiply,
                table,
                context.trig_argument_scale,
            )
        };
        for (category, matches) in &self.categories {
            if matches.is_empty() {
                continue;
            }

            println!();
            println!("  -- {} ({}) --", category.name(), matches.len());
            println!();

            for mwm in matches {
                print_match(&mwm.m, &mwm.metrics, absolute, solve, &render, context);
            }
        }
    }
}

/// Select top-K matches for a category
fn select_category(
    matches: &[MatchWithMetrics],
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
    let mut seen_equations = HashSet::new();
    let mut seen_lhs: HashSet<LhsKey> = HashSet::new();
    let mut seen_sig: HashSet<SignatureKey> = HashSet::new();

    for mwm in candidates {
        if result.len() >= config.top_k {
            break;
        }

        let key = displayed_equation_key(&mwm.m);
        if seen_equations.contains(&key) {
            continue;
        }

        let accept = match category {
            Category::Exact => true,
            Category::Best | Category::Elegant | Category::Stable => {
                seen_lhs.insert(LhsKey::from_match(&mwm.m))
            }
            Category::Interesting => seen_sig.insert(SignatureKey::from_match(&mwm.m)),
        };

        if accept {
            seen_equations.insert(key);
            result.push(mwm.clone());
        }
    }

    result
}

/// Compare visible equation forms with leading negation moved to one side.
fn displayed_equation_key(m: &Match) -> (String, String, bool) {
    let side = |expr: &Expression| {
        let mut expr = expr.clone();
        let mut negative = false;
        while expr.symbols().last() == Some(&Symbol::Neg) {
            expr.pop();
            negative = !negative;
        }
        (expr.to_infix_or_postfix(), negative)
    };
    let (lhs, lhs_negative) = side(&m.lhs.expr);
    let (rhs, rhs_negative) = side(&m.rhs.expr);
    (lhs, rhs, lhs_negative ^ rhs_negative)
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
        Category::Best => compare_matches(&a.m, &b.m, RankingMode::Complexity),
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
    absolute: bool,
    solve: bool,
    render: &impl Fn(&Expression) -> String,
    context: &EvalContext<'_>,
) {
    let lhs_str = render(&m.lhs.expr);
    let rhs_str = render(&m.rhs.expr);

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
        if let Some(solved) =
            crate::solver::solve_for_x_at_with_context(&m.lhs.expr, &m.rhs.expr, m.x_value, context)
        {
            let solved_str = render(&solved);
            println!("     x = {:40} {} {}", solved_str, error_str, info);
        } else {
            println!("{:>24} = {:<24} {} {}", lhs_str, rhs_str, error_str, info);
        }
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
            exact: error.abs() < crate::thresholds::EXACT_MATCH_TOLERANCE,
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

    #[test]
    fn report_fills_slots_after_skipping_equivalent_rows() {
        let matches = vec![
            make_match("xCn", "1", 0.0),
            make_match("xC", "1n", 0.0),
            make_match("xs", "2", 0.0),
            make_match("x2^", "2", 0.0),
            make_match("1xs*", "2", 0.0),
            make_match("x", "2q", 0.0),
            make_match("xC", "1", 0.0),
        ];
        let config = ReportConfig {
            top_k: 4,
            categories: vec![Category::Exact],
            ..ReportConfig::default()
        };
        let report = Report::generate(matches, 2.5, &config);
        let rows: Vec<_> = report.categories[0]
            .1
            .iter()
            .map(|row| displayed_equation_key(&row.m))
            .collect();
        assert_eq!(rows.len(), 4);
        for (lhs, rhs, negative) in [
            ("cospi(x)", "1", true),
            ("cospi(x)", "1", false),
            ("x^2", "2", false),
            ("x", "sqrt(2)", false),
        ] {
            assert!(
                rows.contains(&(lhs.into(), rhs.into(), negative)),
                "{rows:?}"
            );
        }
    }
}
