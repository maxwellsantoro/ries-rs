//! Shared match presentation fields for binding surfaces.

use crate::search::Match;
use crate::solver::canonical_expression_key;
/// Presentation-ready fields derived from a search [`Match`].
///
/// Binding surfaces (WASM, Python) convert this into their own types with
/// trivial field copies, keeping derived-field logic in one place.
#[derive(Clone, Debug)]
pub struct MatchSummary {
    /// Left-hand side expression in infix notation
    pub lhs: String,
    /// Right-hand side expression in infix notation
    pub rhs: String,
    /// Left-hand side expression in postfix notation
    pub lhs_postfix: String,
    /// Right-hand side expression in postfix notation
    pub rhs_postfix: String,
    /// Solved `x = expression` in infix notation, when analytically solvable
    pub solve_for_x: Option<String>,
    /// Solved `x = expression` in postfix notation
    pub solve_for_x_postfix: Option<String>,
    /// Canonical key for deduplication
    pub canonical_key: String,
    /// Solved value of x
    pub x_value: f64,
    /// Error (x_value - target)
    pub error: f64,
    /// Complexity score
    pub complexity: u32,
    /// Number of operators in the equation
    pub operator_count: usize,
    /// Maximum tree depth of the equation
    pub tree_depth: usize,
    /// Preset constant names and values in postfix slot order.
    pub constants: Vec<(String, f64)>,
    /// Whether this is an exact match
    pub is_exact: bool,
}

impl From<Match> for MatchSummary {
    fn from(m: Match) -> Self {
        Self::from_match(&m)
    }
}

impl MatchSummary {
    /// Build presentation fields from a search match.
    pub fn from_match(m: &Match) -> Self {
        Self::build(m, &crate::eval::EvalContext::new(), None)
    }

    /// Build presentation fields using the same user symbols as the search.
    pub fn from_match_with_context(
        m: &Match,
        context: &crate::eval::EvalContext<'_>,
        table: &crate::symbol_table::SymbolTable,
    ) -> Self {
        Self::build(m, context, Some(table))
    }

    fn build(
        m: &Match,
        context: &crate::eval::EvalContext<'_>,
        table: Option<&crate::symbol_table::SymbolTable>,
    ) -> Self {
        let format = |expr: &crate::expr::Expression| {
            if crate::report::exact_zero_constant(expr, context.trig_argument_scale) {
                return "0".to_owned();
            }
            expr.try_to_infix_with_context(table, context.trig_argument_scale)
                .unwrap_or_else(|_| expr.to_postfix())
        };
        let lhs_infix = format(&m.lhs.expr);
        let rhs_infix = format(&m.rhs.expr);

        let solved = crate::solver::solve_for_x_at_with_context(
            &m.lhs.expr,
            &m.rhs.expr,
            m.x_value,
            context,
        );
        let solve_for_x = solved.as_ref().map(|e| format!("x = {}", format(e)));
        let solve_for_x_postfix = solved.as_ref().map(|e| e.to_postfix());

        let canonical_key = canonical_expression_key(&m.lhs.expr)
            .zip(canonical_expression_key(&m.rhs.expr))
            .map(|(l, r)| format!("{l}={r}"))
            .unwrap_or_else(|| format!("{}={}", m.lhs.expr.to_postfix(), m.rhs.expr.to_postfix()));

        Self {
            lhs: lhs_infix,
            rhs: rhs_infix,
            lhs_postfix: m.lhs.expr.to_postfix(),
            rhs_postfix: m.rhs.expr.to_postfix(),
            solve_for_x,
            solve_for_x_postfix,
            canonical_key,
            x_value: m.x_value,
            error: m.error,
            complexity: m.complexity,
            operator_count: m.lhs.expr.operator_count() + m.rhs.expr.operator_count(),
            tree_depth: m.lhs.expr.tree_depth().max(m.rhs.expr.tree_depth()),
            constants: context
                .user_constants
                .iter()
                .map(|c| (c.name.clone(), c.value))
                .collect(),
            is_exact: m.exact,
        }
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
        let complexity = lhs_expr.complexity() + rhs_expr.complexity();
        Match {
            lhs: EvaluatedExpr::new(lhs_expr, 0.0, 1.0, NumType::Integer),
            rhs: EvaluatedExpr::new(rhs_expr, 0.0, 0.0, NumType::Integer),
            x_value: 2.5,
            error,
            complexity,
            exact: error.abs() < crate::thresholds::EXACT_MATCH_TOLERANCE,
        }
    }

    #[test]
    fn match_summary_populates_core_fields() {
        let summary = MatchSummary::from_match(&make_match("x1+", "3", 0.0));
        assert_eq!(summary.lhs_postfix, "x1+");
        assert_eq!(summary.rhs_postfix, "3");
        assert!(summary.is_exact);
        assert!(summary.operator_count > 0);
        assert!(summary.tree_depth > 0);
        assert!(!summary.canonical_key.is_empty());
    }
    #[test]
    fn preset_slots_are_rendered_with_names_and_values() {
        let profile = crate::presets::Preset::AnalyticNT.to_profile();
        let table = crate::symbol_table::SymbolTable::from_profile(&profile);
        let constants = &profile.constants;
        let mut matched = make_match("x", "1", 0.0);
        matched.rhs.expr = Expression::from_symbols(&[crate::symbol::Symbol::UserConstant0]);
        matched.x_value = constants[0].value;
        let context = crate::eval::EvalContext::from_slices(constants, &[]);
        let summary = MatchSummary::from_match_with_context(&matched, &context, &table);
        assert_eq!(summary.rhs, "z2");
        assert_eq!(summary.solve_for_x.as_deref(), Some("x = z2"));
        assert_eq!(summary.constants[0].0, "z2");
        assert_eq!(summary.constants[0].1, constants[0].value);
    }
}
