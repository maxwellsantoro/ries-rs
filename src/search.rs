//! Search and matching algorithms
//!
//! Finds equations by matching LHS and RHS expressions.

use crate::eval::EvalContext;
use crate::expr::{EvaluatedExpr, Expression};
use crate::pool::{RankingMode, TopKPool};
use crate::profile::UserConstant;
use crate::thresholds::{DEGENERATE_DERIVATIVE, DEGENERATE_TEST_THRESHOLD, NEWTON_FINAL_TOLERANCE};
use std::collections::HashSet;
use std::time::Duration;

mod db;
mod newton;
#[cfg(test)]
mod tests;

use crate::gen::compare_expression_order;
use db::calculate_adaptive_search_radius;
pub use db::{ComplexityTier, ExprDatabase, TieredExprDatabase};

/// Extra ulps of slope gap before a flush check is worth an evaluation at 0.
///
/// `x^2` has `|f' * x| = 2 |f - f(0)|`, so the 8-ulp boundary still looks like
/// about 16 ulps of slope. 64 keeps that boundary inside the limit check, and
/// an ordinary O(1) derivative returns before the extra evaluation.
const FLUSH_SLOPE_MARGIN_ULPS: f64 = 64.0;

/// Returns true when a near-zero-derivative LHS should be skipped as degenerate.
///
/// When `target + e` is still a non-integer, that step is a real perturbation
/// and the historical rule (unchanged value or a still-zero derivative) is
/// kept so ordinary searches see the same LHS set.
///
/// From 2^52 upward every finite float is an integer, so `target + e` lands on
/// another integer (or does not move at all once the ulp exceeds `e`). A
/// derivative that stays zero there is not evidence of a constant: `cospi`
/// flips between adjacent integers, and above 2^53 every representable sample
/// is even so the flip is invisible. Those targets are kept when a neighbor
/// changes the value, or when the same value holds at a small integer of the
/// same parity but not off that lattice.
///
/// A value that holds only because a term has underflowed is skipped. For a
/// huge float that is `2 + 1/x` (equals 2 everywhere large, and 2.5 at 2).
/// For any other non-integer the `x` term is gone when the value is still
/// within a few ulps of the limit at 0 (`2 + x^2`, `4 + x^2`, `cospi(x^2)`),
/// while a probe at `0.5` still moves. There is no fixed `|x|` cutoff: `2 +
/// x^2` is bitwise 2 at `1.1e-8` and still within 8 ulps of 2 out near `6e-8`.
/// Sampling off the lattice alone cannot see the large-float case: both the
/// identity and the underflow are non-constant at 0.5. The flush check runs
/// before the derivative test, because at `1e-8` the derivative of `2 + x^2`
/// is `2e-8`.
pub(crate) fn should_skip_degenerate_lhs(
    lhs: &EvaluatedExpr,
    target: f64,
    eval: &EvalContext<'_>,
) -> bool {
    if x_dependence_underflowed(lhs, target, eval) {
        return true;
    }
    if lhs.derivative.abs() >= DEGENERATE_TEST_THRESHOLD {
        return false;
    }
    let standard = target + std::f64::consts::E;
    if standard.is_finite() && standard != target && standard.fract() != 0.0 {
        return probe_matches_legacy_degenerate_rule(lhs, standard, eval);
    }
    if !locally_constant_on_integer_neighbors(lhs, target, eval) {
        return false;
    }
    if reduced_parity_value_differs(lhs, target, eval) {
        return true;
    }
    !off_lattice_sample_moves(lhs, eval)
}

fn probe_matches_legacy_degenerate_rule(
    lhs: &EvaluatedExpr,
    probe: f64,
    eval: &EvalContext<'_>,
) -> bool {
    match crate::eval::evaluate_search(&lhs.expr, probe, eval) {
        Ok(test_result) => {
            let value_unchanged = (test_result.value - lhs.value).abs() < DEGENERATE_TEST_THRESHOLD;
            let deriv_still_zero = test_result.derivative.abs() < DEGENERATE_TEST_THRESHOLD;
            deriv_still_zero || value_unchanged
        }
        Err(_) => true,
    }
}

fn probe_moves(lhs: &EvaluatedExpr, probe: f64, eval: &EvalContext<'_>) -> bool {
    if !probe.is_finite() {
        return false;
    }
    match crate::eval::evaluate_search(&lhs.expr, probe, eval) {
        Ok(test_result) => {
            let value_unchanged = (test_result.value - lhs.value).abs() < DEGENERATE_TEST_THRESHOLD;
            let deriv_still_zero = test_result.derivative.abs() < DEGENERATE_TEST_THRESHOLD;
            !value_unchanged || !deriv_still_zero
        }
        Err(_) => false,
    }
}

fn locally_constant_on_integer_neighbors(
    lhs: &EvaluatedExpr,
    target: f64,
    eval: &EvalContext<'_>,
) -> bool {
    let neighbors = [target.next_up(), target.next_down()];
    for probe in neighbors {
        if probe == target {
            continue;
        }
        if probe_moves(lhs, probe, eval) {
            return false;
        }
    }
    true
}

/// Small integer with the same sign and parity as `target`.
///
/// Large finite floats are integers. An identity that is really about that
/// integer (`cospi` on evens) still holds here; a term that only vanished
/// because `|x|` is huge (`1/x` added to a constant) does not.
fn reduced_parity_probe(target: f64) -> f64 {
    let even = crate::eval::f64_exact_integer_is_even(target).unwrap_or(true);
    let magnitude = if even { 2.0 } else { 1.0 };
    if target.is_sign_negative() {
        -magnitude
    } else {
        magnitude
    }
}

fn reduced_parity_value_differs(lhs: &EvaluatedExpr, target: f64, eval: &EvalContext<'_>) -> bool {
    let probe = reduced_parity_probe(target);
    if probe == target {
        return false;
    }
    match crate::eval::evaluate_search(&lhs.expr, probe, eval) {
        Ok(test_result) => (test_result.value - lhs.value).abs() >= DEGENERATE_TEST_THRESHOLD,
        Err(_) => false,
    }
}

fn off_lattice_sample_moves(lhs: &EvaluatedExpr, eval: &EvalContext<'_>) -> bool {
    [0.5_f64, 1.0, -0.5]
        .into_iter()
        .any(|probe| probe_moves(lhs, probe, eval))
}

/// True when `x` has flushed out of `lhs` at `target`.
///
/// The value is within the exact-match tolerance of the limit at 0, a point
/// closer to 0 (`0.9 x`) still is, and a moderate probe (`0.25`, `0.5`, `±1`)
/// moves. The closer point keeps a real root that only happens to equal that
/// limit (`cospi(x^2)` at `sqrt(2)`, `cospi(1/x)` at `0.1`). The far probes
/// separate underflow from an identity (`x*(1/x)`, `log_x(x^2) = 2` hold
/// there and stay). `0^phi` and `1/x` are undefined at 0; a subnormal stand-in
/// is the same limit when the expression is defined for a tiny nonzero `x`.
///
/// Exact nonzero integers are excluded. Every `|x| >= 2^52` is an integer, and
/// `cospi(x) = 1` on the evens equals `cospi(0)` while a probe at `0.5` moves.
/// Those stay on the integer-lattice rules. Target 0 is excluded too: `x^2`
/// is flat there but the value is genuine.
fn x_dependence_underflowed(lhs: &EvaluatedExpr, target: f64, eval: &EvalContext<'_>) -> bool {
    if !target.is_finite() || target == 0.0 || target.fract() == 0.0 {
        return false;
    }
    // A near-linear change from 0 already exceeds many ulps of the value, so
    // this cannot be a flushed constant. `cospi` near an even integer returns
    // to 1 with a small derivative, but `|f' * x|` is still huge compared with
    // an ulp, and that root must stay.
    let slope_gap = lhs.derivative.abs() * target.abs();
    if slope_gap.is_finite() && slope_gap > FLUSH_SLOPE_MARGIN_ULPS * ulp_of(lhs.value.abs()) {
        return false;
    }
    let Some(limit) = value_at_zero_limit(lhs, target, eval) else {
        return false;
    };
    if !sides_agree_within_ulps(lhs.value, limit) {
        return false;
    }
    // A root can equal the limit at 0 without a flushed term: cospi(x^2) = 1
    // at sqrt(2), cospi(1/x) = 1 at 1/10. Closer to 0, a flushed term is still
    // gone, while those roots move. 0.9 stays on the flushed side of the
    // 8-ulp boundary (6e-8 for 2 + x^2) and off an even-integer reciprocal.
    let closer = target * 0.9;
    if closer == target || !closer.is_finite() {
        return false;
    }
    match crate::eval::evaluate_search(&lhs.expr, closer, eval) {
        Ok(near) if sides_agree_within_ulps(near.value, limit) => {}
        _ => return false,
    }
    // 0.25 sits off the integers and half-integers, so cospi(0.25) is not
    // 0, ±1 and Lambert W stays defined. 0.5 alone misses 1/W(cospi(x)).
    [0.25_f64, 0.5, 1.0, -0.5].into_iter().any(|probe| {
        match crate::eval::evaluate_search(&lhs.expr, probe, eval) {
            Ok(moved) => (moved.value - lhs.value).abs() >= DEGENERATE_TEST_THRESHOLD,
            Err(_) => false,
        }
    })
}

fn value_at_zero_limit(lhs: &EvaluatedExpr, target: f64, eval: &EvalContext<'_>) -> Option<f64> {
    if let Ok(at_zero) = crate::eval::evaluate_search(&lhs.expr, 0.0, eval) {
        return Some(at_zero.value);
    }
    // 0 is outside the domain. A subnormal is still small enough that a
    // positive power of x underflows, and it keeps the sign of the target.
    let stand_in = target.signum() * 1e-300;
    if stand_in == target || !stand_in.is_finite() {
        return None;
    }
    crate::eval::evaluate_search(&lhs.expr, stand_in, eval)
        .ok()
        .map(|result| result.value)
}

/// Half-width of the window in which a flat LHS may be paired with an RHS.
///
/// The residual has to be only a few ulps. A wider window used to stamp
/// near-misses (`cospi(sqrt(x)) ≈ 0.997`) as exact.
pub(crate) fn flat_exact_half_width(lhs_value: f64) -> f64 {
    let scale = lhs_value.abs().max(1.0);
    (8.0 * f64::EPSILON * scale).max(f64::EPSILON)
}

/// True when `lhs_value` and `rhs_value` agree to a few ulps of a unit scale.
///
/// The scale is at least 1, so this is the flat-match window. Exactness of a
/// reported row uses [`sides_agree_within_ulps`], which has no floor: a
/// sub-unit residual can be many ulps of the two sides.
pub(crate) fn residual_is_exact(lhs_value: f64, rhs_value: f64) -> bool {
    let diff = (lhs_value - rhs_value).abs();
    if diff == 0.0 {
        return true;
    }
    let scale = lhs_value.abs().max(rhs_value.abs()).max(1.0);
    diff <= (8.0 * f64::EPSILON * scale).max(f64::EPSILON)
}

/// True when the two sides agree to a few ulps of `max(|lhs|, |rhs|)`.
///
/// Both values are the ones at the typed target. Necessary for an exact row,
/// and not sufficient: Newton can land on a different root of a function whose
/// sides barely move (`e^x = 1` at `1e-15`). [`match_is_exact`] also requires
/// the solved `x`.
pub(crate) fn sides_agree_within_ulps(lhs_value: f64, rhs_value: f64) -> bool {
    let diff = (lhs_value - rhs_value).abs();
    if diff == 0.0 {
        return true;
    }
    if !diff.is_finite() {
        return false;
    }
    let scale = lhs_value.abs().max(rhs_value.abs());
    scale.is_finite() && scale > 0.0 && diff <= 8.0 * f64::EPSILON * scale
}

/// One ulp of a non-negative magnitude.
fn ulp_of(mag: f64) -> f64 {
    if mag <= 0.0 || !mag.is_finite() {
        return f64::from_bits(1);
    }
    let up = mag.next_up();
    if !up.is_finite() {
        return mag - mag.next_down();
    }
    up - mag
}

/// True when `solved_x` is within 8 ulps of `target`.
///
/// Eight ulps of `|target|` still covers a flat root: `x^(1/3)` at `1e6` is
/// about 7 ulps off, and `x^(1/3) = 4` at `64` is about 2. It rejects a walk
/// to 0 (`e^x = 1` at `1e-15`, error about `1e-15`).
pub(crate) fn solved_x_within_ulps(solved_x: f64, target: f64) -> bool {
    let diff = (solved_x - target).abs();
    if diff == 0.0 {
        return true;
    }
    if !diff.is_finite() || !solved_x.is_finite() || !target.is_finite() {
        return false;
    }
    diff <= 8.0 * ulp_of(target.abs())
}

/// Exact when the sides agree at the typed target and the solved `x` is that target.
pub(crate) fn match_is_exact(lhs_value: f64, rhs_value: f64, solved_x: f64, target: f64) -> bool {
    sides_agree_within_ulps(lhs_value, rhs_value) && solved_x_within_ulps(solved_x, target)
}
#[cfg(test)]
use newton::newton_raphson;
use newton::newton_raphson_with_constants;

#[derive(Clone, Copy, Debug)]
struct SearchTimer {
    #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
    start_ms: f64,
    #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
    start: std::time::Instant,
}

impl SearchTimer {
    #[inline]
    fn start() -> Self {
        #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
        {
            Self {
                start_ms: js_sys::Date::now(),
            }
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
        {
            Self {
                start: std::time::Instant::now(),
            }
        }
    }

    #[inline]
    fn elapsed(self) -> Duration {
        #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
        {
            let elapsed_ms = (js_sys::Date::now() - self.start_ms).max(0.0);
            Duration::from_secs_f64(elapsed_ms / 1000.0)
        }

        #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
        {
            self.start.elapsed()
        }
    }
}

/// Search path that produced the reported matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatcherPath {
    /// Materialized serial generation and matching.
    Serial,
    /// Parallel generation followed by materialized serial matching.
    Parallel,
    Turbo,
    Adaptive,
    Streaming,
    OneSided,
    FastMatch,
}

impl MatcherPath {
    /// Whether this path applies the requested LHS value/derivative policy.
    pub fn applies_lhs_value_dedup(self, requested: bool) -> bool {
        requested
            && matches!(
                self,
                Self::Serial | Self::Parallel | Self::Turbo | Self::Adaptive
            )
    }
}

/// Statistics collected during search
#[derive(Clone, Debug, Default)]
pub struct SearchStats {
    /// Actual matcher used, after any fallback.
    pub matcher: Option<MatcherPath>,
    /// Time spent generating expressions
    pub gen_time: Duration,
    /// Time spent searching/matching
    pub search_time: Duration,
    /// Number of LHS expressions generated
    pub lhs_count: usize,
    /// Number of RHS expressions generated
    pub rhs_count: usize,
    /// Number of LHS expressions tested (after filtering)
    pub lhs_tested: usize,
    /// Number of candidate pairs tested (coarse filter)
    pub candidates_tested: usize,
    /// Total number of RHS expressions scanned across all candidate windows
    pub candidate_window_total: usize,
    /// Largest RHS candidate window scanned for any single LHS
    pub candidate_window_max: usize,
    /// Postfix form of the LHS that produced the largest candidate window
    pub candidate_window_max_lhs_postfix: Option<String>,
    /// LHS value for the largest candidate window
    pub candidate_window_max_lhs_value: Option<f64>,
    /// LHS derivative for the largest candidate window
    pub candidate_window_max_lhs_derivative: Option<f64>,
    /// LHS complexity for the largest candidate window
    pub candidate_window_max_lhs_complexity: Option<u32>,
    /// Number of candidates rejected by the strict pre-Newton gate
    pub strict_gate_rejections: usize,
    /// Number of Newton-Raphson calls
    pub newton_calls: usize,
    /// Number of successful Newton convergences
    pub newton_success: usize,
    /// Number of matches inserted into pool
    pub pool_insertions: usize,
    /// Number of matches rejected by pool (error threshold)
    pub pool_rejections_error: usize,
    /// Number of matches rejected by pool (dedupe)
    pub pool_rejections_dedupe: usize,
    /// Number of matches evicted from pool
    pub pool_evictions: usize,
    /// Final pool size
    pub pool_final_size: usize,
    /// Final best error in pool
    pub pool_best_error: f64,
    /// Final adaptive error threshold used to size candidate windows.
    pub pool_error_threshold: Option<f64>,
    /// Whether search exited early
    pub early_exit: bool,
}

impl SearchStats {
    pub fn new() -> Self {
        Self::default()
    }

    fn record_pool(&mut self, pool: &TopKPool) {
        self.pool_insertions = pool.stats.insertions;
        self.pool_rejections_error = pool.stats.rejections_error;
        self.pool_rejections_dedupe = pool.stats.rejections_dedupe;
        self.pool_evictions = pool.stats.evictions;
        self.pool_final_size = pool.len();
        self.pool_best_error = pool.minimum_error().unwrap_or(f64::INFINITY);
        self.pool_error_threshold = Some(pool.best_error);
    }

    #[inline]
    pub fn record_candidate_window(&mut self, lhs: &EvaluatedExpr, width: usize) {
        self.candidate_window_total = self.candidate_window_total.saturating_add(width);
        if width > self.candidate_window_max {
            self.candidate_window_max = width;
            self.candidate_window_max_lhs_postfix = Some(lhs.expr.to_postfix());
            self.candidate_window_max_lhs_value = Some(lhs.value);
            self.candidate_window_max_lhs_derivative = Some(lhs.derivative);
            self.candidate_window_max_lhs_complexity = Some(lhs.expr.complexity());
        }
    }

    /// Merge another stats record's additive search/pool counters into this one.
    ///
    /// Generation-side counts and timing are owned by the caller and are not
    /// merged. The largest-candidate-window record keeps whichever side observed
    /// the wider window.
    #[cfg(feature = "parallel")]
    pub fn merge_search_counters(&mut self, other: &SearchStats) {
        self.lhs_tested = self.lhs_tested.saturating_add(other.lhs_tested);
        self.candidates_tested = self
            .candidates_tested
            .saturating_add(other.candidates_tested);
        self.candidate_window_total = self
            .candidate_window_total
            .saturating_add(other.candidate_window_total);
        if other.candidate_window_max > self.candidate_window_max {
            self.candidate_window_max = other.candidate_window_max;
            self.candidate_window_max_lhs_postfix = other.candidate_window_max_lhs_postfix.clone();
            self.candidate_window_max_lhs_value = other.candidate_window_max_lhs_value;
            self.candidate_window_max_lhs_derivative = other.candidate_window_max_lhs_derivative;
            self.candidate_window_max_lhs_complexity = other.candidate_window_max_lhs_complexity;
        }
        self.strict_gate_rejections = self
            .strict_gate_rejections
            .saturating_add(other.strict_gate_rejections);
        self.newton_calls = self.newton_calls.saturating_add(other.newton_calls);
        self.newton_success = self.newton_success.saturating_add(other.newton_success);
        self.early_exit |= other.early_exit;
    }

    #[inline]
    pub fn candidate_window_avg(&self) -> f64 {
        if self.lhs_tested == 0 {
            0.0
        } else {
            self.candidate_window_total as f64 / self.lhs_tested as f64
        }
    }

    #[inline]
    pub fn candidates_per_pool_insertion(&self) -> f64 {
        if self.pool_insertions == 0 {
            0.0
        } else {
            self.candidates_tested as f64 / self.pool_insertions as f64
        }
    }

    #[inline]
    pub fn newton_success_rate(&self) -> f64 {
        if self.newton_calls == 0 {
            0.0
        } else {
            self.newton_success as f64 / self.newton_calls as f64
        }
    }

    #[inline]
    pub fn pool_attempts(&self) -> usize {
        self.pool_insertions
            .saturating_add(self.pool_rejections_error)
            .saturating_add(self.pool_rejections_dedupe)
    }

    #[inline]
    pub fn pool_acceptance_rate(&self) -> f64 {
        let attempts = self.pool_attempts();
        if attempts == 0 {
            0.0
        } else {
            self.pool_insertions as f64 / attempts as f64
        }
    }

    /// Print stats to stdout
    pub fn print(&self) {
        println!();
        println!("  === Search Statistics ===");
        println!();
        println!("  Generation:");
        println!(
            "    Time:            {:>10.3}ms",
            self.gen_time.as_secs_f64() * 1000.0
        );
        println!("    LHS expressions: {:>10}", self.lhs_count);
        println!("    RHS expressions: {:>10}", self.rhs_count);
        println!();
        println!("  Search:");
        println!(
            "    Time:            {:>10.3}ms",
            self.search_time.as_secs_f64() * 1000.0
        );
        println!("    LHS tested:      {:>10}", self.lhs_tested);
        println!("    Candidates:      {:>10}", self.candidates_tested);
        println!("    Window avg:      {:>10.2}", self.candidate_window_avg());
        println!("    Window max:      {:>10}", self.candidate_window_max);
        if let Some(lhs) = &self.candidate_window_max_lhs_postfix {
            println!("    Window max lhs:  {:>10}", lhs);
        }
        println!("    Gate rejects:    {:>10}", self.strict_gate_rejections);
        println!(
            "    Cand/insert:     {:>10.2}",
            self.candidates_per_pool_insertion()
        );
        println!("    Newton calls:    {:>10}", self.newton_calls);
        println!(
            "    Newton success:  {:>10} ({:.1}%)",
            self.newton_success,
            100.0 * self.newton_success_rate()
        );
        if self.early_exit {
            println!("    Early exit:      yes");
        }
        println!();
        println!("  Pool:");
        println!("    Insertions:      {:>10}", self.pool_insertions);
        println!("    Rejected (err):  {:>10}", self.pool_rejections_error);
        println!("    Rejected (dup):  {:>10}", self.pool_rejections_dedupe);
        println!("    Evictions:       {:>10}", self.pool_evictions);
        println!(
            "    Acceptance:      {:>9.1}%",
            100.0 * self.pool_acceptance_rate()
        );
        println!("    Final size:      {:>10}", self.pool_final_size);
        if self.pool_final_size != 0 {
            println!("    Best error:      {:>14.2e}", self.pool_best_error);
        }
    }
}

/// Compute complexity limits from a search level.
///
/// This function provides a consistent mapping from integer search levels
/// to complexity limits used by the library API (Python, WASM, and adaptive mode).
///
/// # Formula
///
/// Both sides use the same budget:
///
/// - Base complexity: 40
/// - Level step: 2
/// - `max_lhs = max_rhs = 40 + 2 * level`
///
/// # Level Guidelines
///
/// Counts are right-hand sides generated on a full scan (a target with no
/// cheap exact match). Left-hand side counts are a few hundred to a few
/// thousand and depend on the target.
///
/// | Level | Both sides | RHS count (approx) |
/// |-------|------------|--------------------|
/// | 0     | 40         | ~1.0K              |
/// | 1     | 42         | ~1.6K              |
/// | 2     | 44         | ~2.6K              |
/// | 3     | 46         | ~4.2K              |
/// | 4     | 48         | ~6.7K              |
/// | 5     | 50         | ~11K               |
///
/// Level 3 covers plain equations such as `2*x = 5` (solved `x = 5/2`),
/// `x = 3/2`, `x = 2*6`, and `x = 6*7`, plus one extra add or multiply past
/// the heaviest single-digit product (`9*9`, complexity 42). Level 5 is the
/// top of the interactive range: a full scan stays within a few seconds.
///
/// # Note
///
/// The CLI uses a separate formula (`35 + 10*L` on each side). That scale is
/// not shared here. CLI level 3 is already large enough that a target with no
/// cheap exact match runs for minutes, which is the wrong budget for the
/// browser demo. Callers that want the CLI bounds should pass them to
/// [`GenConfig`](crate::gen::GenConfig) directly.
#[inline]
pub fn level_to_complexity(level: u32) -> (u32, u32) {
    const BASE: u32 = 40;
    const LEVEL_STEP: u32 = 2;

    // Saturating arithmetic avoids panics/wraparound if an API caller passes
    // an out-of-range level without validation.
    let budget = BASE.saturating_add(LEVEL_STEP.saturating_mul(level));
    (budget, budget)
}

/// Half the place value of the last decimal digit in a target.
///
/// Pass a validated original decimal literal to retain trailing zeros and
/// scientific exponents. Numeric callers use the shortest round-trip decimal spelling.
/// Numeric zero without a spelling uses a tolerance of `1e-15`.
pub fn significant_digits_tolerance(target: f64, target_text: Option<&str>) -> f64 {
    let numeric_text;
    let text = match target_text {
        Some(text) => text.trim(),
        None if target == 0.0 => return 1e-15,
        None => {
            numeric_text = target.to_string();
            &numeric_text
        }
    };
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let exponent = exponent.parse::<i64>().unwrap_or_else(|_| {
        if exponent.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    });
    let decimal_places = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as i64);
    let half_place_exponent = exponent.saturating_sub(decimal_places).saturating_sub(1);
    // Parse directly so tolerances in the subnormal range retain their scale.
    // A value below that range rounds to zero, requiring an exact f64 root.
    format!("5e{half_place_exponent}")
        .parse::<f64>()
        .unwrap_or(0.0)
        .min(f64::MAX)
}

/// Whether an LHS value should be dropped before matching.
///
/// Values under `threshold` match every constant near zero and flood the pool
/// (`cospi(2.5) = 0` against many tiny right-hand sides). Target zero is the
/// exception: `x` evaluates to 0, and dropping it makes `x = ln(1)` unreachable.
/// `-0.0` compares equal to `0.0`, so a signed zero target is kept too.
#[inline]
pub(crate) fn prune_near_zero_lhs(value: f64, target: f64, threshold: f64) -> bool {
    value.abs() < threshold && target != 0.0
}

/// A matched equation
#[derive(Clone, Debug)]
pub struct Match {
    /// Left-hand side expression (contains x)
    pub lhs: EvaluatedExpr,
    /// Right-hand side expression (constant)
    pub rhs: EvaluatedExpr,
    /// Solved value of x
    pub x_value: f64,
    /// Difference from target: x_value - target
    pub error: f64,
    /// Total complexity (LHS + RHS)
    pub complexity: u32,
    /// Reported and ranked as an exact root.
    ///
    /// The two sides, both evaluated at the typed target, agree to a few ulps
    /// of `max(|lhs|, |rhs|)`, and `x_value` is within 8 ulps of the typed
    /// target. A Newton hit that only passed the loose local-root test is
    /// false, and so is a nearby root whose sides do not match at the target
    /// or whose solved `x` walked off to a different root.
    pub exact: bool,
}

impl Match {
    /// Format the match for display (used in tests)
    #[cfg(test)]
    pub fn display(&self, _target: f64) -> String {
        let lhs_str = self.lhs.expr.to_infix();
        let rhs_str = self.rhs.expr.to_infix();

        let error_str = if self.exact {
            "('exact' match)".to_string()
        } else {
            let sign = if self.error >= 0.0 { "+" } else { "-" };
            format!("for x = T {} {:.6e}", sign, self.error.abs())
        };

        format!(
            "{:>24} = {:<24} {} {{{}}}",
            lhs_str, rhs_str, error_str, self.complexity
        )
    }
}

/// Configuration for the search process.
///
/// Controls matching thresholds, result limits, symbol filtering, and search behavior.
/// This struct is the main entry point for customizing how RIES searches for equations.
///
/// # Example
///
/// ```rust
/// use ries_rs::search::SearchConfig;
/// use ries_rs::pool::RankingMode;
///
/// let config = SearchConfig {
///     target: std::f64::consts::PI,
///     max_matches: 50,
///     max_error: 1e-10,
///     stop_at_exact: true,
///     ranking_mode: RankingMode::Complexity,
///     ..SearchConfig::default()
/// };
/// ```
///
/// # Fields Overview
///
/// - **Target**: `target` - the value to search for equations matching it
/// - **Limits**: `max_matches`, `max_error` - control result quantity and quality
/// - **Stopping**: `stop_at_exact`, `stop_below` - early termination conditions
/// - **Refinement**: `newton_iterations`, `refine_with_newton`, `derivative_margin` - Newton-Raphson settings
/// - **Filtering**: `zero_value_threshold`, `rhs_allowed_symbols`, `rhs_excluded_symbols` - prune unwanted results
/// - **Extensions**: `user_constants`, `user_functions` - custom symbols
/// - **Diagnostics**: `show_newton`, `show_match_checks`, etc. - debug output
/// - **Ranking**: `ranking_mode` - how to order results
#[derive(Clone, Debug)]
pub struct SearchConfig {
    /// Target value to find equations for.
    ///
    /// The search will find equations where LHS ≈ RHS ≈ target.
    /// This is the number you're trying to "solve" or represent symbolically.
    ///
    /// Default: 0.0
    pub target: f64,

    /// Maximum number of matches to return in results.
    ///
    /// Once this many matches are found and confirmed, the pool will start
    /// evicting lower-quality matches to make room for better ones.
    ///
    /// Default: 100
    pub max_matches: usize,

    /// Maximum acceptable error for a match to be included.
    ///
    /// Only expressions within this absolute error from the target are considered matches.
    /// Smaller values give more precise but fewer results.
    ///
    /// Default: 1.0
    pub max_error: f64,

    /// Stop search when an exact match is found.
    ///
    /// Batch search can terminate early on an exact match. Streaming scans
    /// the full LHS stream and prioritizes the simplest qualifying match.
    ///
    /// Default: false
    pub stop_at_exact: bool,

    /// Stop search when error goes below this threshold.
    ///
    /// Batch search can terminate early on a match below this threshold.
    /// Streaming scans the full LHS stream and prioritizes the simplest
    /// qualifying match.
    ///
    /// Default: None
    pub stop_below: Option<f64>,

    /// Threshold for pruning LHS expressions with near-zero values.
    ///
    /// LHS expressions with absolute values below this threshold are pruned
    /// to prevent flooding results with trivial matches like `cospi(2.5) = 0`.
    /// The prune is skipped when the target itself is zero, so `x = ln(1)` can
    /// be found. Set to 0.0 to disable this filtering for every target.
    ///
    /// Default: 1e-4
    pub zero_value_threshold: f64,

    /// Maximum Newton-Raphson iterations for root refinement.
    ///
    /// Controls how many iterations are used to refine candidate solutions.
    /// Higher values may find more precise roots but take longer.
    ///
    /// Default: 15
    pub newton_iterations: usize,

    /// User-defined constants for evaluation.
    ///
    /// Custom constants that can be used in expressions, defined via `-N\'name=value\'`.
    /// Each constant has a name, value, and optional description.
    ///
    /// Default: empty
    pub user_constants: Vec<UserConstant>,

    /// User-defined functions for evaluation.
    ///
    /// Custom functions that can be used in expressions, defined via `-F\'name:formula\'`.
    /// These extend the built-in functions (sin, cos, etc.).
    ///
    /// Default: empty
    pub user_functions: Vec<crate::udf::UserFunction>,

    /// Argument scale for `sinpi/cospi/tanpi` evaluation.
    ///
    /// The default is π, matching original `sinpi(x) = sin(πx)` semantics.
    /// Override this for alternate trig conventions without relying on global state.
    pub trig_argument_scale: f64,

    /// Whether to refine candidate roots with Newton-Raphson iteration.
    ///
    /// When true, initial coarse matches are refined using Newton-Raphson
    /// to find more precise solutions. Disable for faster but less precise search.
    ///
    /// Default: true
    pub refine_with_newton: bool,

    /// Optional RHS-only allowed symbol set.
    ///
    /// If set, all symbols used on the RHS must be in this set (specified as ASCII bytes).
    /// This allows restricting RHS expressions to a subset of available symbols.
    /// Combined with `rhs_excluded_symbols`, both filters apply.
    ///
    /// Default: None (all symbols allowed)
    pub rhs_allowed_symbols: Option<HashSet<u8>>,

    /// Optional RHS-only excluded symbol set.
    ///
    /// If set, RHS expressions using any of these symbols are rejected.
    /// This allows excluding specific symbols from RHS expressions.
    /// Combined with `rhs_allowed_symbols`, both filters apply.
    ///
    /// Default: None (no symbols excluded)
    pub rhs_excluded_symbols: Option<HashSet<u8>>,

    /// Show Newton-Raphson iteration diagnostic output.
    ///
    /// When true, prints detailed information about each Newton-Raphson iteration.
    /// Enabled by `-Dn` command-line flag. Useful for debugging convergence issues.
    ///
    /// Default: false
    pub show_newton: bool,

    /// Show match check diagnostic output.
    ///
    /// When true, prints information about each candidate match evaluation.
    /// Enabled by `-Do` command-line flag.
    ///
    /// Default: false
    pub show_match_checks: bool,

    /// Show pruned arithmetic diagnostic output.
    ///
    /// When true, prints information about arithmetic expressions that were pruned.
    /// Enabled by `-DA` command-line flag.
    ///
    /// Default: false
    #[allow(dead_code)]
    pub show_pruned_arith: bool,

    /// Show pruned range/zero diagnostic output.
    ///
    /// When true, prints information about expressions pruned due to range
    /// constraints or near-zero values. Enabled by `-DB` command-line flag.
    ///
    /// Default: false
    pub show_pruned_range: bool,

    /// Show database adds diagnostic output.
    ///
    /// When true, prints information about expressions added to the database.
    /// Enabled by `-DG` command-line flag.
    ///
    /// Default: false
    pub show_db_adds: bool,

    /// When true, matches must match all significant digits of the target.
    ///
    /// When enabled, the tolerance is computed based on the magnitude of the
    /// target value to require full precision matching. The actual tolerance
    /// is computed in `main.rs` and passed as `max_error`.
    ///
    /// Default: false
    #[allow(dead_code)]
    pub match_all_digits: bool,

    /// Threshold below which derivative is considered degenerate in Newton-Raphson.
    ///
    /// If `|derivative| < derivative_margin` during Newton-Raphson iteration,
    /// the refinement is skipped to avoid numerical instability and division
    /// by near-zero values.
    ///
    /// Default: 1e-12 (from `DEGENERATE_DERIVATIVE` constant)
    pub derivative_margin: f64,

    /// Ranking mode for pool ordering and eviction.
    ///
    /// Determines how matches are ranked in the result pool:
    /// - `Complexity`: Sort by expression complexity (simpler is better)
    /// - `Error`: Sort by match error (more precise is better)
    ///
    /// Default: `RankingMode::Complexity`
    pub ranking_mode: RankingMode,

    /// Reduce memory and matching work in materialized searches by keeping
    /// one LHS per target-local (value, derivative) key. This heuristic can
    /// change ranked equations and solved roots. Streaming preserves the
    /// generated LHS stream. Defaults to false.
    pub lhs_value_dedup: bool,

    /// Eligibility and canonical deduplication rules applied before ranking.
    pub filters: MatchFilters,
}

/// Optional constraints on the equations admitted to the search pool.
#[derive(Clone, Copy, Debug, Default)]
pub struct MatchFilters {
    pub min_equate_value: Option<f64>,
    pub max_equate_value: Option<f64>,
    pub min_match_distance: Option<f64>,
    pub expression_constraints: Option<crate::gen::ExpressionConstraintOptions>,
    pub numeric_anagram: bool,
    pub canonical: bool,
}

impl MatchFilters {
    pub(crate) fn allows_side(&self, side: &EvaluatedExpr) -> bool {
        self.min_equate_value.is_none_or(|min| side.value >= min)
            && self.max_equate_value.is_none_or(|max| side.value <= max)
            && self.expression_constraints.is_none_or(|constraints| {
                crate::gen::expression_respects_constraints(&side.expr, constraints)
            })
    }

    pub(crate) fn allows_pair(&self, lhs: &Expression, rhs: &Expression) -> bool {
        if !self.numeric_anagram {
            return true;
        }
        let digits = digit_signature(lhs);
        digits != [0; 9] && digits == digit_signature(rhs)
    }

    pub(crate) fn allows_error(&self, error: f64) -> bool {
        self.min_match_distance.is_none_or(|min| error.abs() >= min)
    }

    /// Check all filters for a match produced outside the regular search.
    pub fn allows_match(&self, m: &Match) -> bool {
        self.allows_side(&m.lhs)
            && self.allows_side(&m.rhs)
            && self.allows_pair(&m.lhs.expr, &m.rhs.expr)
            && self.allows_error(m.error)
    }
}

pub(crate) fn digit_signature(expression: &Expression) -> [usize; 9] {
    let mut counts = [0; 9];
    for symbol in expression.symbols() {
        let byte = *symbol as u8;
        if (b'1'..=b'9').contains(&byte) {
            counts[(byte - b'1') as usize] += 1;
        }
    }
    counts
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            target: 0.0,
            max_matches: 100,
            max_error: 1.0,
            stop_at_exact: false,
            stop_below: None,
            zero_value_threshold: 1e-4,
            newton_iterations: 15,
            user_constants: Vec::new(),
            user_functions: Vec::new(),
            trig_argument_scale: crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE,
            refine_with_newton: true,
            rhs_allowed_symbols: None,
            rhs_excluded_symbols: None,
            show_newton: false,
            show_match_checks: false,
            show_pruned_arith: false,
            show_pruned_range: false,
            show_db_adds: false,
            match_all_digits: false,
            derivative_margin: DEGENERATE_DERIVATIVE,
            ranking_mode: RankingMode::Complexity,
            lhs_value_dedup: false,
            filters: MatchFilters::default(),
        }
    }
}

impl SearchConfig {
    /// Build an explicit search context for this configuration.
    pub fn context(&self) -> SearchContext<'_> {
        SearchContext::new(self)
    }

    fn pool(&self) -> TopKPool {
        TopKPool::new_with_diagnostics(
            self.max_matches,
            self.max_error,
            self.show_db_adds,
            self.ranking_mode,
        )
        .with_canonical_dedup(self.filters.canonical)
    }

    #[inline]
    pub(crate) fn rhs_symbol_allowed(&self, rhs: &crate::expr::Expression) -> bool {
        let symbols = rhs.symbols();

        if let Some(allowed) = &self.rhs_allowed_symbols {
            if symbols.iter().any(|s| !allowed.contains(&(*s as u8))) {
                return false;
            }
        }

        if let Some(excluded) = &self.rhs_excluded_symbols {
            if symbols.iter().any(|s| excluded.contains(&(*s as u8))) {
                return false;
            }
        }

        true
    }
}

/// Explicit per-run search context.
#[derive(Clone, Copy, Debug)]
pub struct SearchContext<'a> {
    /// Immutable search configuration for this run.
    pub config: &'a SearchConfig,
    /// Evaluation context derived from the search configuration.
    pub eval: EvalContext<'a>,
}

impl<'a> SearchContext<'a> {
    pub fn new(config: &'a SearchConfig) -> Self {
        Self {
            config,
            eval: EvalContext::from_slices(&config.user_constants, &config.user_functions)
                .with_trig_argument_scale(config.trig_argument_scale),
        }
    }
}

/// Perform a complete search
///
/// This function is part of the public API for library consumers who want a simple
/// search interface without statistics collection.
#[allow(dead_code)]
pub fn search(target: f64, gen_config: &crate::gen::GenConfig, max_matches: usize) -> Vec<Match> {
    let (matches, _stats) = search_with_stats(target, gen_config, max_matches);
    matches
}

/// Perform a complete search with statistics collection
///
/// This function is part of the public API for library consumers who want
/// detailed statistics about the search process.
#[allow(dead_code)]
pub fn search_with_stats(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> (Vec<Match>, SearchStats) {
    search_with_stats_and_options(target, gen_config, max_matches, false, None)
}

/// Perform a complete search with statistics collection and early exit options
pub fn search_with_stats_and_options(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
    stop_at_exact: bool,
    stop_below: Option<f64>,
) -> (Vec<Match>, SearchStats) {
    if !target.is_finite() {
        return (Vec::new(), SearchStats::default());
    }
    let config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    search_with_stats_and_config(gen_config, &config)
}

/// Perform a complete search with a fully specified search configuration.
///
/// This function includes a safety fallback: if expression generation would
/// exceed ~2M expressions (which would risk OOM), it automatically switches
/// to streaming mode to avoid memory exhaustion.
pub fn search_with_stats_and_config(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    search_with_batch_limit(gen_config, config, 2_000_000)
}

fn search_with_batch_limit(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
    batch_limit: usize,
) -> (Vec<Match>, SearchStats) {
    if !config.target.is_finite() {
        return (Vec::new(), SearchStats::default());
    }

    use crate::gen::generate_bounded_with_rhs_policy;

    let context = SearchContext::new(config);

    let gen_start = SearchTimer::start();

    // Try bounded generation first — if limit exceeded, fall back to streaming
    if let Some(generated) = generate_bounded_with_rhs_policy(
        gen_config,
        config.target,
        &context.eval,
        batch_limit,
        false,
        Some(config),
    ) {
        let gen_time = gen_start.elapsed();

        // Build database
        let mut db = ExprDatabase::new();
        db.insert_rhs(generated.rhs);

        // Find matches with stats
        let (matches, mut stats) = db.find_matches_with_stats_and_context(&generated.lhs, &context);

        // Add generation stats
        stats.matcher = Some(MatcherPath::Serial);
        stats.gen_time = gen_time;
        stats.lhs_count = generated.lhs.len();
        stats.rhs_count = db.rhs_count();

        (matches, stats)
    } else {
        // Limit exceeded — fall back to streaming mode which avoids OOM
        search_streaming_with_config(gen_config, config)
    }
}

// =============================================================================
// ADAPTIVE SEARCH - Iterative LHS/RHS expansion like original RIES
// =============================================================================

/// Perform an adaptive search that iteratively expands LHS/RHS complexity
///
/// This implements the original RIES algorithm where expressions are generated
/// incrementally, expanding the side (LHS or RHS) that has fewer expressions.
/// This ensures balanced growth and matches the original's expression counts.
///
/// # Algorithm
///
/// 1. Start with minimal complexity limits (LHS: 1, RHS: 1)
/// 2. Generate expressions at current complexity
/// 3. Track how many expressions were generated for each side
/// 4. Expand the side with fewer expressions (increase complexity by 1)
/// 5. Repeat until target expression count is reached
/// 6. Then perform matching on all generated expressions
///
/// # Advantages
///
/// - Matches original RIES behavior exactly
/// - Generates similar number of expressions as original
/// - Better memory efficiency than generating all at once
/// - Can leverage parallelization more effectively
///
/// # Expression Count Formula
///
/// Target expressions = 2000 × 4^(2 + level)
/// - Level 0: ~32,000 expressions
/// - Level 1: ~128,000 expressions
/// - Level 2: ~512,000 expressions
/// - Level 3: ~2,048,000 expressions
pub fn search_adaptive(
    base_config: &crate::gen::GenConfig,
    search_config: &SearchConfig,
    level: u32,
) -> (Vec<Match>, SearchStats) {
    search_adaptive_with_parallelism(base_config, search_config, level, true)
}

/// Adaptive search with explicit control over parallel expression generation.
pub fn search_adaptive_with_parallelism(
    base_config: &crate::gen::GenConfig,
    search_config: &SearchConfig,
    level: u32,
    parallel: bool,
) -> (Vec<Match>, SearchStats) {
    use crate::expr::EvaluatedExpr;
    use crate::gen::{lhs_dedup_key, RhsRepresentatives};
    use std::borrow::Cow;
    use std::collections::HashMap;

    if !search_config.target.is_finite() {
        return (Vec::new(), SearchStats::default());
    }

    let gen_start = SearchTimer::start();
    let context = SearchContext::new(search_config);
    // Reuse each side's representative policy when merging successive bounds.
    let mut lhs_map: HashMap<_, EvaluatedExpr> = HashMap::new();
    let mut all_lhs = Vec::new();
    let mut rhs_map = RhsRepresentatives::new(Some(search_config));

    // Target expression count: 2000 × 4^(2 + level)
    // Level 0 ≈ 32 K, level 1 ≈ 128 K, level 2 ≈ 512 K, level 3 ≈ 2 M
    let target_count = adaptive_target_count(level);

    // Iterative adaptive growth: start at complexity (1, 1) and expand the side
    // with fewer expressions until the target count is reached.  Each iteration
    // re-generates all expressions up to the current bounds and retains one
    // representative per key.
    // Re-generation cost follows a geometric series so the overhead is ≤ 33 %
    // of the final-iteration cost (sum 1 + 1/4 + 1/16 + … = 4/3).
    let mut lhs_c: u32 = 1;
    let mut rhs_c: u32 = 1;
    const MAX_ADAPTIVE_COMPLEXITY: u32 = 60;

    loop {
        let mut config = base_config.clone();
        // Use lhs_c/rhs_c directly — do NOT clamp against base_config bounds.
        // Clamping would pin the loop to the caller's pre-computed level bounds and
        // prevent growth from (1, 1), defeating the adaptive algorithm entirely.
        config.max_lhs_complexity = lhs_c;
        config.max_rhs_complexity = rhs_c;

        let generate_serial = || {
            crate::gen::generate_all_with_rhs_policy(
                &config,
                search_config.target,
                &context.eval,
                Some(search_config),
            )
        };
        #[cfg(feature = "parallel")]
        let generated = if parallel {
            crate::gen::generate_parallel_with_rhs_policy(
                &config,
                search_config.target,
                &context.eval,
                Some(search_config),
            )
        } else {
            generate_serial()
        };
        #[cfg(not(feature = "parallel"))]
        let generated = {
            let _ = parallel;
            generate_serial()
        };

        let lhs_count = if search_config.lhs_value_dedup {
            for expr in generated.lhs {
                let key = lhs_dedup_key(&expr, search_config.filters);
                lhs_map
                    .entry(key)
                    .and_modify(|existing| {
                        if compare_expression_order(&expr, existing).is_lt() {
                            *existing = expr.clone();
                        }
                    })
                    .or_insert(expr);
            }
            lhs_map.len()
        } else {
            // Each iteration regenerates the full LHS at the new bounds.
            all_lhs = generated.lhs;
            all_lhs.len()
        };
        for expr in generated.rhs {
            rhs_map.insert(Cow::Owned(expr));
        }

        if lhs_count + rhs_map.len() >= target_count {
            break;
        }
        if lhs_c >= MAX_ADAPTIVE_COMPLEXITY && rhs_c >= MAX_ADAPTIVE_COMPLEXITY {
            break;
        }
        // Expand the side with fewer expressions (original RIES algorithm)
        if lhs_count <= rhs_map.len() && lhs_c < MAX_ADAPTIVE_COMPLEXITY {
            lhs_c += 1;
        } else if rhs_c < MAX_ADAPTIVE_COMPLEXITY {
            rhs_c += 1;
        } else {
            break;
        }
    }

    if search_config.lhs_value_dedup {
        all_lhs = lhs_map.into_values().collect();
    }
    let all_rhs: Vec<EvaluatedExpr> = rhs_map.into_values().collect();

    let gen_time = gen_start.elapsed();

    // Now perform the actual matching
    let mut db = ExprDatabase::new();
    db.insert_rhs(all_rhs);

    let search_start = SearchTimer::start();
    let (matches, mut stats) = db.find_matches_with_stats_and_context(&all_lhs, &context);
    let search_time = search_start.elapsed();

    // Overwrite the generation-side counts for the adaptive wrapper.
    stats.matcher = Some(MatcherPath::Adaptive);
    stats.gen_time = gen_time;
    stats.search_time = search_time;
    stats.lhs_count = all_lhs.len();
    stats.rhs_count = db.rhs_count();

    (matches, stats)
}

#[inline]
fn adaptive_target_count(level: u32) -> usize {
    2000_usize.saturating_mul(4_usize.saturating_pow(level.saturating_add(2)))
}

// =============================================================================
// STREAMING SEARCH - Memory-efficient search for high complexity levels
// =============================================================================

/// Perform a streaming search that processes expressions as they're generated
///
/// This is the memory-efficient version of search that builds the RHS database
/// incrementally. Instead of generating ALL expressions into memory first,
/// it processes RHS expressions immediately and matches LHS expressions as
/// they arrive.
///
/// # Memory Efficiency
///
/// - Traditional: O(expressions) memory - all expressions stored
/// - Streaming: O(database) memory - only RHS database stored, LHS processed on-the-fly
///
/// # When to Use
///
/// Use streaming search when:
/// - Complexity levels are high (L4+, where expressions count > 10M)
/// - Memory is limited
/// - You want to see results progressively
///
/// # Example
///
/// ```no_run
/// use ries_rs::gen::GenConfig;
/// use ries_rs::search::search_streaming;
/// let config = GenConfig::default();
/// let (matches, stats) = search_streaming(2.5, &config, 100, false, None);
/// ```
#[allow(dead_code)]
pub fn search_streaming(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
    stop_at_exact: bool,
    stop_below: Option<f64>,
) -> (Vec<Match>, SearchStats) {
    let config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    search_streaming_with_config(gen_config, &config)
}

fn stop_condition_met(m: &Match, search_config: &SearchConfig) -> bool {
    (search_config.stop_at_exact && m.exact)
        || search_config
            .stop_below
            .is_some_and(|threshold| m.error.abs() < threshold)
}

fn prefer_streaming_stop_match(candidate: &Match, current: &Match) -> bool {
    use std::cmp::Ordering;

    candidate
        .lhs
        .expr
        .complexity()
        .cmp(&current.lhs.expr.complexity())
        .then_with(|| {
            candidate
                .rhs
                .expr
                .complexity()
                .cmp(&current.rhs.expr.complexity())
        })
        .then_with(|| {
            candidate
                .error
                .abs()
                .partial_cmp(&current.error.abs())
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            candidate
                .lhs
                .expr
                .to_postfix()
                .cmp(&current.lhs.expr.to_postfix())
        })
        .then_with(|| {
            candidate
                .rhs
                .expr
                .to_postfix()
                .cmp(&current.rhs.expr.to_postfix())
        })
        .is_lt()
}

fn merge_priority_match(
    mut matches: Vec<Match>,
    priority: Match,
    max_matches: usize,
    canonical: bool,
) -> Vec<Match> {
    let key = canonical.then(|| crate::pool::canonical_match_key(&priority));
    if let Some(idx) = matches.iter().position(|m| match &key {
        Some(key) => crate::pool::canonical_match_key(m) == *key,
        None => m.lhs.expr == priority.lhs.expr && m.rhs.expr == priority.rhs.expr,
    }) {
        matches.remove(idx);
    }
    matches.insert(0, priority);
    matches.truncate(max_matches);
    matches
}

/// Perform a streaming search with a fully specified search configuration.
///
/// Memory model: O(rhs_database) — LHS expressions are matched on-the-fly in a
/// second generator pass and never buffered. The only persistent allocation is the
/// deduplicated RHS database.
pub fn search_streaming_with_config(
    gen_config: &crate::gen::GenConfig,
    search_config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::gen::{generate_streaming_with_context, RhsRepresentatives, StreamingCallbacks};
    use std::borrow::Cow;

    if !search_config.target.is_finite() {
        return (Vec::new(), SearchStats::default());
    }

    let gen_start = SearchTimer::start();
    let mut stats = SearchStats::new();
    stats.matcher = Some(MatcherPath::Streaming);
    let context = SearchContext::new(search_config);

    let mut pool = search_config.pool();

    // === Pass 1: Build RHS database ===
    // Pass the original gen_config unchanged so the generator's internal
    // `has_rhs_symbol_overrides` branch fires when needed (--S-RHS, --N-RHS, etc.),
    // applying the rhs_only_config split.  The on_lhs no-op discards any LHS
    // expressions that the generator produces as part of that split.
    let mut rhs_db = TieredExprDatabase::new();
    let mut rhs_map = RhsRepresentatives::new(Some(search_config));

    {
        let mut callbacks = StreamingCallbacks {
            on_rhs: &mut |expr| {
                rhs_map.insert(Cow::Borrowed(expr));
                true
            },
            on_lhs: &mut |_| true,
        };
        generate_streaming_with_context(
            gen_config,
            search_config.target,
            &context.eval,
            &mut callbacks,
        );
    }

    for expr in rhs_map.into_values() {
        rhs_db.insert(expr);
    }
    rhs_db.finalize();
    stats.rhs_count = rhs_db.total_count();
    stats.gen_time = gen_start.elapsed();

    // === Pass 2: Stream LHS and match on-the-fly ===
    //
    // Streaming must keep O(rhs_db) memory even in classic/exact modes, so we never
    // buffer the full LHS set. stop_at_exact / stop_below therefore cannot perform a
    // literal early exit here because generator order is not complexity ordered.
    //
    // Instead, we remember the best qualifying match seen so far under a
    // complexity-first ordering and splice it to the front of the final results.
    // That preserves the "simplest acceptable match first" behavior without
    // reintroducing O(all_lhs) memory usage.
    let mut lhs_gen_config = gen_config.clone();
    lhs_gen_config.generate_rhs = false;

    let search_start = SearchTimer::start();
    let mut best_stop_match: Option<Match> = None;

    {
        let mut callbacks = StreamingCallbacks {
            on_rhs: &mut |_| true,
            on_lhs: &mut |lhs| {
                stats.lhs_count += 1;
                if !search_config.filters.allows_side(lhs) {
                    return true;
                }

                // Skip LHS with value too close to 0. Target 0 is exempt so
                // `x = ln(1)` is reachable. See `prune_near_zero_lhs`.
                if prune_near_zero_lhs(
                    lhs.value,
                    search_config.target,
                    search_config.zero_value_threshold,
                ) {
                    if search_config.show_pruned_range {
                        eprintln!(
                            "  [pruned range] value={:.6e} reason=\"near-zero\" expr=\"{}\"",
                            lhs.value,
                            lhs.expr.to_infix()
                        );
                    }
                    return true;
                }

                // Skip degenerate expressions
                if should_skip_degenerate_lhs(lhs, search_config.target, &context.eval) {
                    return true;
                }
                if lhs.derivative.abs() < DEGENERATE_TEST_THRESHOLD {
                    stats.lhs_tested += 1;
                    let half = flat_exact_half_width(lhs.value);
                    stats.record_candidate_window(
                        lhs,
                        rhs_db.count_in_range(lhs.value - half, lhs.value + half),
                    );
                    for rhs in rhs_db.iter_tiers_in_range(lhs.value - half, lhs.value + half) {
                        if !search_config.filters.allows_pair(&lhs.expr, &rhs.expr) {
                            continue;
                        }
                        stats.candidates_tested += 1;
                        if search_config.show_match_checks {
                            eprintln!(
                                "  [match] checking lhs={:.6} rhs={:.6}",
                                lhs.value, rhs.value
                            );
                        }
                        // `residual_is_exact` is the named flat window. Exactness
                        // still requires the sides to agree, and the solved x
                        // is the target on this path so the ulp window holds.
                        if residual_is_exact(lhs.value, rhs.value)
                            && match_is_exact(
                                lhs.value,
                                rhs.value,
                                search_config.target,
                                search_config.target,
                            )
                            && pool.would_accept(0.0, true)
                            && search_config.filters.allows_error(0.0)
                        {
                            let m = Match {
                                lhs: lhs.clone(),
                                rhs: rhs.clone(),
                                x_value: search_config.target,
                                error: 0.0,
                                complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                                exact: true,
                            };
                            if stop_condition_met(&m, search_config)
                                && best_stop_match
                                    .as_ref()
                                    .is_none_or(|current| prefer_streaming_stop_match(&m, current))
                            {
                                best_stop_match = Some(m.clone());
                            }
                            pool.try_insert(m);
                        }
                    }
                    return true;
                }

                stats.lhs_tested += 1;

                let search_radius = calculate_adaptive_search_radius(
                    lhs.derivative,
                    lhs.expr.complexity(),
                    pool.len(),
                    search_config.max_matches,
                    pool.best_error,
                    pool.accept_error,
                );
                let low = lhs.value - search_radius;
                let high = lhs.value + search_radius;
                stats.record_candidate_window(lhs, rhs_db.count_in_range(low, high));

                for rhs in rhs_db.iter_tiers_in_range(low, high) {
                    if !search_config.filters.allows_pair(&lhs.expr, &rhs.expr) {
                        continue;
                    }
                    stats.candidates_tested += 1;
                    if search_config.show_match_checks {
                        eprintln!(
                            "  [match] checking lhs={:.6} rhs={:.6}",
                            lhs.value, rhs.value
                        );
                    }

                    let val_diff = lhs.value - rhs.value;
                    let x_delta = -val_diff / lhs.derivative;
                    let coarse_error = x_delta.abs();

                    let is_potentially_exact = coarse_error < NEWTON_FINAL_TOLERANCE;
                    if !pool.would_accept_strict(coarse_error, is_potentially_exact) {
                        stats.strict_gate_rejections += 1;
                        continue;
                    }

                    if !search_config.refine_with_newton {
                        let refined_x = search_config.target + x_delta;
                        let refined_error = x_delta;
                        let is_exact =
                            match_is_exact(lhs.value, rhs.value, refined_x, search_config.target);

                        if pool.would_accept(refined_error.abs(), is_exact)
                            && search_config.filters.allows_error(refined_error)
                        {
                            let m = Match {
                                lhs: lhs.clone(),
                                rhs: rhs.clone(),
                                x_value: refined_x,
                                error: refined_error,
                                complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                                exact: is_exact,
                            };
                            if stop_condition_met(&m, search_config)
                                && best_stop_match
                                    .as_ref()
                                    .is_none_or(|current| prefer_streaming_stop_match(&m, current))
                            {
                                best_stop_match = Some(m.clone());
                            }
                            pool.try_insert(m);
                        }
                        continue;
                    }

                    stats.newton_calls += 1;
                    let initial_guess = search_config.target + x_delta;
                    if let Some(hit) = newton_raphson_with_constants(
                        &lhs.expr,
                        rhs.value,
                        initial_guess,
                        search_config.newton_iterations,
                        &context.eval,
                        search_config.show_newton,
                        search_config.derivative_margin,
                    ) {
                        if !hit.real {
                            continue;
                        }
                        let refined_x = hit.x;
                        stats.newton_success += 1;
                        let refined_error = refined_x - search_config.target;
                        let is_exact =
                            match_is_exact(lhs.value, rhs.value, refined_x, search_config.target);

                        if pool.would_accept(refined_error.abs(), is_exact)
                            && search_config.filters.allows_error(refined_error)
                        {
                            let m = Match {
                                lhs: lhs.clone(),
                                rhs: rhs.clone(),
                                x_value: refined_x,
                                error: refined_error,
                                complexity: lhs.expr.complexity() + rhs.expr.complexity(),
                                exact: is_exact,
                            };
                            if stop_condition_met(&m, search_config)
                                && best_stop_match
                                    .as_ref()
                                    .is_none_or(|current| prefer_streaming_stop_match(&m, current))
                            {
                                best_stop_match = Some(m.clone());
                            }
                            pool.try_insert(m);
                        }
                    }
                }
                true
            },
        };
        generate_streaming_with_context(
            &lhs_gen_config,
            search_config.target,
            &context.eval,
            &mut callbacks,
        );
    }

    stats.record_pool(&pool);
    stats.search_time = search_start.elapsed();
    stats.early_exit = false;

    let matches = pool.into_sorted();
    let matches = if let Some(priority_match) = best_stop_match {
        merge_priority_match(
            matches,
            priority_match,
            search_config.max_matches,
            search_config.filters.canonical,
        )
    } else {
        matches
    };

    (matches, stats)
}

/// Perform one-sided search: generate RHS expressions only and match `x = RHS`.
pub fn search_one_sided_with_stats_and_config(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::eval::evaluate_with_context;
    use crate::expr::Expression;
    use crate::gen::generate_all_with_rhs_policy;
    use crate::symbol::Symbol;

    let gen_start = SearchTimer::start();
    let context = SearchContext::new(config);

    let mut rhs_only = gen_config.clone();
    rhs_only.generate_lhs = false;
    rhs_only.generate_rhs = true;

    let mut generated =
        generate_all_with_rhs_policy(&rhs_only, config.target, &context.eval, Some(config));
    let gen_time = gen_start.elapsed();

    let search_start = SearchTimer::start();
    let mut pool = config.pool();
    let mut stats = SearchStats::new();
    stats.matcher = Some(MatcherPath::OneSided);
    let mut early_exit = false;

    let mut lhs_expr = Expression::new();
    lhs_expr.push_with_table(Symbol::X, &gen_config.symbol_table);
    let lhs_eval = evaluate_with_context(&lhs_expr, config.target, &context.eval);
    let lhs_eval = match lhs_eval {
        Ok(v) => v,
        Err(_) => {
            stats.gen_time = gen_time;
            stats.search_time = search_start.elapsed();
            return (Vec::new(), stats);
        }
    };
    let lhs = EvaluatedExpr::new(
        lhs_expr,
        lhs_eval.value,
        lhs_eval.derivative,
        lhs_eval.num_type,
    );

    stats.lhs_count = 1;
    stats.rhs_count = generated.rhs.len();
    stats.lhs_tested = 1;

    if !config.filters.allows_side(&lhs) {
        stats.gen_time = gen_time;
        stats.search_time = search_start.elapsed();
        return (Vec::new(), stats);
    }
    generated.rhs.sort_unstable_by(compare_expression_order);
    for rhs in generated.rhs {
        if !config.filters.allows_pair(&lhs.expr, &rhs.expr) {
            continue;
        }
        stats.candidates_tested += 1;
        if config.show_match_checks {
            eprintln!(
                "  [match] checking lhs={:.6} rhs={:.6}",
                lhs.value, rhs.value
            );
        }

        let error = rhs.value - config.target;
        let is_exact = match_is_exact(lhs.value, rhs.value, rhs.value, config.target);
        if !pool.would_accept(error.abs(), is_exact) || !config.filters.allows_error(error) {
            continue;
        }

        let m = Match {
            lhs: lhs.clone(),
            rhs: rhs.clone(),
            x_value: rhs.value,
            error,
            complexity: lhs.expr.complexity() + rhs.expr.complexity(),
            exact: is_exact,
        };

        pool.try_insert(m);

        if config.stop_at_exact && is_exact {
            early_exit = true;
            break;
        }
        if let Some(threshold) = config.stop_below {
            if error.abs() < threshold {
                early_exit = true;
                break;
            }
        }
    }

    stats.record_pool(&pool);
    stats.gen_time = gen_time;
    stats.search_time = search_start.elapsed();
    stats.early_exit = early_exit;

    (pool.into_sorted(), stats)
}

/// Perform a search whose generation phase uses Rayon.
///
/// This function is part of the public API for library consumers who want
/// parallel generation without statistics collection. Matching/Newton remains
/// the same serial batch matcher used by the sequential path.
#[cfg(feature = "parallel")]
#[allow(dead_code)]
pub fn search_parallel(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> Vec<Match> {
    let (matches, _stats) = search_parallel_with_stats(target, gen_config, max_matches);
    matches
}

/// Perform a search with parallel generation and statistics collection.
///
/// This function is part of the public API for library consumers who want
/// detailed statistics about the search process. Matching/Newton remains serial.
#[cfg(feature = "parallel")]
#[allow(dead_code)]
pub fn search_parallel_with_stats(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
) -> (Vec<Match>, SearchStats) {
    search_parallel_with_stats_and_options(target, gen_config, max_matches, false, None)
}

/// Perform a parallel search with statistics collection and early exit options
#[cfg(feature = "parallel")]
pub fn search_parallel_with_stats_and_options(
    target: f64,
    gen_config: &crate::gen::GenConfig,
    max_matches: usize,
    stop_at_exact: bool,
    stop_below: Option<f64>,
) -> (Vec<Match>, SearchStats) {
    let config = SearchConfig {
        target,
        max_matches,
        stop_at_exact,
        stop_below,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        ..Default::default()
    };

    search_parallel_with_stats_and_config(gen_config, &config)
}

/// Perform a search with parallel generation and a fully specified search configuration.
///
/// Generation uses Rayon for parallelism, while RHS database construction and
/// LHS/RHS matching/Newton remain the same serial batch pipeline used by the
/// sequential path. A sequential count-only OOM-safety check runs first: if it
/// estimates the expression space would exceed ~2M entries the function falls
/// back to streaming mode automatically.
#[cfg(feature = "parallel")]
pub fn search_parallel_with_stats_and_config(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::gen::{
        count_expressions_with_limit_and_context, generate_parallel_with_rhs_policy,
        has_rhs_symbol_overrides,
    };

    if !config.target.is_finite() {
        return (Vec::new(), SearchStats::default());
    }

    // Parallel generation does not preserve the sequential expression order.
    // A first-hit stop would therefore select a different match even though
    // this mode promises byte-identical results. Use the sequential pipeline
    // whenever the requested result depends on discovery order.
    if config.stop_at_exact || config.stop_below.is_some() {
        return search_with_stats_and_config(gen_config, config);
    }

    const MAX_EXPRESSIONS_BEFORE_STREAMING: usize = 2_000_000;
    let context = SearchContext::new(config);

    // OOM-safety gate: do a sequential count-only preflight to check whether the
    // search space fits in memory before committing to the parallel run.
    if count_expressions_with_limit_and_context(
        gen_config,
        config.target,
        &context.eval,
        MAX_EXPRESSIONS_BEFORE_STREAMING,
    )
    .is_none()
    {
        return search_streaming_with_config(gen_config, config);
    }

    // Within the limit — generate in parallel using Rayon.
    let gen_start = SearchTimer::start();
    let generated =
        generate_parallel_with_rhs_policy(gen_config, config.target, &context.eval, Some(config));
    let gen_time = gen_start.elapsed();

    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);

    let (matches, mut stats) = db.find_matches_with_stats_and_context(&generated.lhs, &context);
    stats.matcher = Some(if has_rhs_symbol_overrides(gen_config) {
        MatcherPath::Serial
    } else {
        MatcherPath::Parallel
    });
    stats.gen_time = gen_time;
    stats.lhs_count = generated.lhs.len();
    stats.rhs_count = db.rhs_count();

    (matches, stats)
}

/// Turbo search: materialized generation with parallel Newton refinement for wide windows.
///
/// Candidate acceptance and ranking follow the serial matcher. Turbo tolerates
/// a larger materialized expression set before falling back to streaming.
#[cfg(feature = "parallel")]
pub fn search_turbo_with_stats_and_config(
    gen_config: &crate::gen::GenConfig,
    config: &SearchConfig,
) -> (Vec<Match>, SearchStats) {
    use crate::gen::generate_bounded_with_rhs_policy;

    if !config.target.is_finite() {
        return (Vec::new(), SearchStats::default());
    }

    // A first-hit search has no use for speculative refinements past the stop.
    if config.stop_at_exact || config.stop_below.is_some() {
        return search_parallel_with_stats_and_config(gen_config, config);
    }

    // Turbo trades memory for speed, so it tolerates a much larger materialized
    // expression set than the default batch path before falling back to the
    // serial streaming matcher. (The serial/parallel paths stream above ~2M to
    // bound memory; turbo's whole purpose is to use more resources.)
    const MAX_EXPRESSIONS_BEFORE_STREAMING: usize = 64_000_000;
    let context = SearchContext::new(config);

    let gen_start = SearchTimer::start();
    let Some(generated) = generate_bounded_with_rhs_policy(
        gen_config,
        config.target,
        &context.eval,
        MAX_EXPRESSIONS_BEFORE_STREAMING,
        false,
        Some(config),
    ) else {
        return search_streaming_with_config(gen_config, config);
    };
    let gen_time = gen_start.elapsed();

    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs);

    let (matches, mut stats) =
        db.find_matches_turbo_with_stats_and_context(&generated.lhs, &context);
    stats.matcher = Some(MatcherPath::Turbo);
    stats.gen_time = gen_time;
    stats.lhs_count = generated.lhs.len();
    stats.rhs_count = db.rhs_count();

    (matches, stats)
}
