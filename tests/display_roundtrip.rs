//! Printed equations must evaluate to the value the engine reports.
//!
//! The infix parser in this file is independent of the postfix evaluator. It
//! implements the precedence the printer claims (power tighter than unary
//! minus, unary minus tighter than multiplication and division, addition and
//! subtraction left-associative) so a dropped parenthesis fails the test.

#![cfg(not(target_arch = "wasm32"))]

use ries_rs::eval::{constants, evaluate};
use ries_rs::expr::Expression;
use ries_rs::gen::GenConfig;
use ries_rs::search::{search_with_stats_and_config, SearchConfig};
use ries_rs::symbol::Symbol;
use ries_rs::{solve_for_x_at, MatchSummary};

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    x: f64,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, x: f64) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
            x,
        }
    }

    fn eval(input: &str, x: f64) -> Result<f64, String> {
        let mut parser = Parser::new(input.trim(), x);
        if parser.input.is_empty() {
            return Err("empty expression".to_string());
        }
        let value = parser.parse_add()?;
        parser.skip_ws();
        if parser.pos != parser.input.len() {
            return Err(format!(
                "trailing input {:?} in {input:?}",
                parser.rest_str()
            ));
        }
        if !value.is_finite() {
            return Err(format!("non-finite value {value} for {input:?}"));
        }
        Ok(value)
    }

    fn rest_str(&self) -> String {
        String::from_utf8_lossy(&self.input[self.pos..]).into_owned()
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.input.get(self.pos).copied()
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn starts_with_word(&mut self, word: &str, boundary: bool) -> bool {
        self.skip_ws();
        let rest = &self.input[self.pos..];
        let bytes = word.as_bytes();
        if rest.len() < bytes.len() || &rest[..bytes.len()] != bytes {
            return false;
        }
        if boundary {
            if let Some(next) = rest.get(bytes.len()) {
                if next.is_ascii_alphanumeric() {
                    return false;
                }
            }
        }
        true
    }

    fn consume_word(&mut self, word: &str, boundary: bool) -> bool {
        if self.starts_with_word(word, boundary) {
            self.pos += word.len();
            true
        } else {
            false
        }
    }

    fn parse_add(&mut self) -> Result<f64, String> {
        let mut left = self.parse_mul()?;
        loop {
            if self.consume(b'+') {
                left += self.parse_mul()?;
            } else if self.consume(b'-') {
                left -= self.parse_mul()?;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<f64, String> {
        let mut left = self.parse_power()?;
        loop {
            if self.consume(b'*') {
                left *= self.parse_power()?;
            } else if self.consume(b'/') {
                let right = self.parse_power()?;
                if right.abs() < f64::MIN_POSITIVE {
                    return Err("division by zero".to_string());
                }
                left /= right;
            } else if self.starts_primary() {
                // `2 pi` is multiplication. Only a digit immediately before the
                // space is printed this way; treating any adjacent primary as
                // a product matches that and stays unambiguous.
                left *= self.parse_power()?;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn starts_primary(&mut self) -> bool {
        match self.peek() {
            Some(b'(') | Some(b'0'..=b'9') => true,
            Some(b) if b.is_ascii_alphabetic() => true,
            _ => false,
        }
    }

    fn parse_power(&mut self) -> Result<f64, String> {
        if self.consume(b'-') {
            return Ok(-self.parse_power()?);
        }
        let base = self.parse_primary()?;
        if self.consume(b'"') {
            if !self.consume(b'/') {
                return Err("expected '/' after '\"' in a root".to_string());
            }
            let radicand = self.parse_primary()?;
            return real_root(base, radicand);
        }
        if self.consume(b'^') {
            let exp = self.parse_power()?;
            return real_pow(base, exp);
        }
        Ok(base)
    }

    fn parse_primary(&mut self) -> Result<f64, String> {
        self.skip_ws();
        if self.consume(b'(') {
            let value = self.parse_add()?;
            if !self.consume(b')') {
                return Err("missing ')'".to_string());
            }
            return Ok(value);
        }
        if let Some(b'0'..=b'9') = self.peek() {
            return self.parse_number();
        }

        if self.consume_word("atan2", true) {
            return self.parse_atan2();
        }
        if self.consume_word("sinpi", true) {
            let arg = self.parse_call_arg()?;
            return sin_pi(arg);
        }
        if self.consume_word("cospi", true) {
            let arg = self.parse_call_arg()?;
            return cos_pi(arg);
        }
        if self.consume_word("tanpi", true) {
            let arg = self.parse_call_arg()?;
            return tan_pi(arg);
        }
        if self.consume_word("sqrt", true) {
            let arg = self.parse_call_arg()?;
            if arg < 0.0 {
                return Err("sqrt domain".to_string());
            }
            return Ok(arg.sqrt());
        }
        if self.consume_word("ln", true) {
            let arg = self.parse_call_arg()?;
            if arg <= 0.0 {
                return Err("ln domain".to_string());
            }
            return Ok(arg.ln());
        }
        if self.consume_word("log_", false) {
            return self.parse_log();
        }
        if self.consume_word("plastic", true) {
            return Ok(constants::PLASTIC);
        }
        if self.consume_word("catalan", true) {
            return Ok(constants::CATALAN);
        }
        if self.consume_word("gamma", true) {
            return Ok(constants::GAMMA);
        }
        if self.consume_word("apery", true) {
            return Ok(constants::APERY);
        }
        if self.consume_word("phi", true) {
            return Ok(constants::PHI);
        }
        if self.consume_word("pi", true) {
            return Ok(constants::PI);
        }
        if self.consume_word("W", true) {
            let arg = self.parse_call_arg()?;
            return lambert_w(arg);
        }
        if self.consume_word("e", true) {
            return Ok(constants::E);
        }
        if self.consume_word("x", true) {
            return Ok(self.x);
        }
        Err(format!("expected a value, found {:?}", self.rest_str()))
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.input[start..self.pos]).unwrap();
        text.parse::<f64>()
            .map_err(|_| format!("bad number {text}"))
    }

    fn parse_call_arg(&mut self) -> Result<f64, String> {
        if !self.consume(b'(') {
            return Err("expected '(' after a function name".to_string());
        }
        let value = self.parse_add()?;
        if !self.consume(b')') {
            return Err("missing ')' after a function argument".to_string());
        }
        Ok(value)
    }

    fn parse_atan2(&mut self) -> Result<f64, String> {
        if !self.consume(b'(') {
            return Err("expected '(' after atan2".to_string());
        }
        let y = self.parse_add()?;
        if !self.consume(b',') {
            return Err("expected ',' in atan2".to_string());
        }
        let x = self.parse_add()?;
        if !self.consume(b')') {
            return Err("missing ')' after atan2".to_string());
        }
        Ok(y.atan2(x))
    }

    fn parse_log(&mut self) -> Result<f64, String> {
        let base = if self.peek() == Some(b'(') {
            self.parse_primary()?
        } else {
            self.parse_log_atom()?
        };
        let arg = self.parse_call_arg()?;
        if base <= 0.0 || (base - 1.0).abs() < 1e-15 || arg <= 0.0 {
            return Err("log domain".to_string());
        }
        Ok(arg.ln() / base.ln())
    }

    fn parse_log_atom(&mut self) -> Result<f64, String> {
        // An ungrouped log base is a single constant or number, never a call.
        if let Some(b'0'..=b'9') = self.peek() {
            return self.parse_number();
        }
        if self.consume_word("plastic", true) {
            return Ok(constants::PLASTIC);
        }
        if self.consume_word("catalan", true) {
            return Ok(constants::CATALAN);
        }
        if self.consume_word("gamma", true) {
            return Ok(constants::GAMMA);
        }
        if self.consume_word("apery", true) {
            return Ok(constants::APERY);
        }
        if self.consume_word("phi", true) {
            return Ok(constants::PHI);
        }
        if self.consume_word("pi", true) {
            return Ok(constants::PI);
        }
        if self.consume_word("e", true) {
            return Ok(constants::E);
        }
        if self.consume_word("x", true) {
            return Ok(self.x);
        }
        Err(format!("expected a log base, found {:?}", self.rest_str()))
    }
}

fn snap_integer(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if (value - rounded).abs() <= 1e-12 {
        Some(rounded as i64)
    } else {
        None
    }
}

fn snap_half_index(value: f64) -> Option<i64> {
    let doubled = value * 2.0;
    let rounded = doubled.round();
    if !rounded.is_finite() || (doubled - rounded).abs() > 1e-12 {
        return None;
    }
    let rounded_i = rounded as i64;
    if rounded_i.rem_euclid(2) == 0 {
        return None;
    }
    Some((rounded_i - 1) / 2)
}

fn alternating_sign(n: i64) -> f64 {
    if n.rem_euclid(2) == 0 {
        1.0
    } else {
        -1.0
    }
}

/// Match the engine's exact-integer snaps for `sinpi` / `cospi` / `tanpi`.
fn sin_pi(arg: f64) -> Result<f64, String> {
    if snap_integer(arg).is_some() {
        return Ok(0.0);
    }
    if let Some(n) = snap_half_index(arg) {
        return Ok(alternating_sign(n));
    }
    Ok((constants::PI * arg).sin())
}

fn cos_pi(arg: f64) -> Result<f64, String> {
    if let Some(n) = snap_integer(arg) {
        return Ok(alternating_sign(n));
    }
    if snap_half_index(arg).is_some() {
        return Ok(0.0);
    }
    Ok((constants::PI * arg).cos())
}

fn tan_pi(arg: f64) -> Result<f64, String> {
    if snap_half_index(arg).is_some() {
        return Err("tan pole".to_string());
    }
    if snap_integer(arg).is_some() {
        return Ok(0.0);
    }
    let cosine = (constants::PI * arg).cos();
    if cosine.abs() < 1e-10 {
        return Err("tan pole".to_string());
    }
    Ok((constants::PI * arg).tan())
}

fn real_pow(base: f64, exp: f64) -> Result<f64, String> {
    if !base.is_finite() || !exp.is_finite() {
        return Err("non-finite power".to_string());
    }
    // The engine rejects every non-integer power of a non-positive base,
    // including `0^x` (`ln(1)^x`), before calling `powf`.
    if base <= 0.0 {
        let rounded = exp.round();
        if (exp - rounded).abs() > 1e-8 {
            return Err("non-positive base to a non-integer power".to_string());
        }
    }
    if base < 0.0 {
        let rounded = exp.round();
        let magnitude = (-base).powf(rounded);
        let odd = (rounded as i64) % 2 != 0;
        let value = if odd { -magnitude } else { magnitude };
        if !value.is_finite() {
            return Err("power overflow".to_string());
        }
        return Ok(value);
    }
    let value = base.powf(exp);
    if !value.is_finite() {
        return Err("power overflow".to_string());
    }
    Ok(value)
}

fn real_root(index: f64, radicand: f64) -> Result<f64, String> {
    if index.abs() < f64::MIN_POSITIVE {
        return Err("root index is zero".to_string());
    }
    real_pow(radicand, 1.0 / index)
}

fn lambert_w(x: f64) -> Result<f64, String> {
    let inv_e = -1.0 / constants::E;
    if x < inv_e || !x.is_finite() {
        return Err("lambert W domain".to_string());
    }
    if x == 0.0 {
        return Ok(0.0);
    }
    let mut w = if x < -0.3 {
        let p = (2.0 * (constants::E * x + 1.0)).sqrt();
        -1.0 + p
    } else if x < 1.0 {
        x
    } else {
        x.ln()
    };
    for _ in 0..40 {
        let ew = w.exp();
        let wew = w * ew;
        let residual = wew - x;
        let denom = ew * (w + 1.0) - (w + 2.0) * residual / (2.0 * w + 2.0);
        if denom.abs() < 1e-18 {
            break;
        }
        let delta = residual / denom;
        w -= delta;
        if delta.abs() < 1e-14 {
            break;
        }
    }
    if !w.is_finite() {
        return Err("lambert W did not converge".to_string());
    }
    Ok(w)
}

fn assert_infix_round_trip(postfix: &str, x: f64) {
    let expr = Expression::parse(postfix).unwrap_or_else(|| panic!("parse {postfix}"));
    let infix = expr.to_infix();
    let posted = evaluate(&expr, x);
    let printed = Parser::eval(&infix, x);
    match (posted, printed) {
        (Ok(result), Ok(value)) => {
            let scale = result.value.abs().max(value.abs()).max(1.0);
            let tol = 1e-8 * scale;
            assert!(
                (result.value - value).abs() <= tol,
                "infix {infix:?} from {postfix:?} evaluated to {value}, postfix is {} (x={x})",
                result.value
            );
        }
        (Err(_), Err(_)) => {}
        (Ok(result), Err(err)) => panic!(
            "infix {infix:?} from {postfix:?} failed to parse ({err}); postfix value is {}",
            result.value
        ),
        (Err(_), Ok(value)) if !value.is_finite() || value.abs() > 1e12 => {
            // Poles such as tanpi(0.5) overflow in the engine and are huge in f64.
        }
        (Err(err), Ok(value)) => panic!(
            "infix {infix:?} from {postfix:?} evaluated to {value}, but postfix evaluation failed: {err}"
        ),
    }
}

fn eval_solved_infix(solved: &Expression) -> f64 {
    let infix = solved.to_infix();
    Parser::eval(&infix, 0.0).unwrap_or_else(|err| {
        panic!(
            "printed solution {infix:?} (postfix {}) failed: {err}",
            solved.to_postfix()
        )
    })
}

#[test]
fn reported_targets_print_the_branch_they_claim() {
    let sqrt_two_pi = 2.506_628_274_631;
    let phi = 1.618_033_988_7;

    // sin(πx) = 1. The principal inverse is 0.5; the root near √(2π) is 2.5.
    let sin_solved = solve_for_x_at(
        &Expression::parse("xS").unwrap(),
        &Expression::parse("1").unwrap(),
        sqrt_two_pi,
    )
    .expect("sin equation is solvable");
    let sin_value = eval_solved_infix(&sin_solved);
    assert!(
        (sin_value - 2.5).abs() < 1e-9,
        "printed {} evaluates to {sin_value}, expected 2.5",
        sin_solved.to_infix()
    );

    // cos(πx) = ln(1) = 0. The principal inverse is 0.5; 2 + acos(0)/π = 2.5.
    let cos_solved = solve_for_x_at(
        &Expression::parse("xC").unwrap(),
        &Expression::parse("1l").unwrap(),
        sqrt_two_pi,
    )
    .expect("cos equation is solvable");
    let cos_value = eval_solved_infix(&cos_solved);
    assert!(
        (cos_value - 2.5).abs() < 1e-9,
        "printed {} evaluates to {cos_value}, expected 2.5",
        cos_solved.to_infix()
    );

    // -(1/(1/(-φ))) simplifies to φ. The printed grouping has to survive parsing.
    assert_infix_round_trip("fnrrn", phi);
    let grouped = Expression::parse("fnrrn").unwrap().to_infix();
    assert_eq!(grouped, "-(1/(1/(-phi)))");
    let grouped_value = Parser::eval(&grouped, 0.0).unwrap();
    assert!(
        (grouped_value - constants::PHI).abs() < 1e-12,
        "grouped form {grouped} evaluated to {grouped_value}"
    );
}

#[test]
fn infix_printing_round_trips_operator_combinations() {
    let probes = [
        "x", "1", "p", "e", "f", "xn", "xr", "xs", "xq", "xl", "xE", "xS", "xC", "xT", "fnrrn",
        "xrr", "xrs", "xsr", "xsn", "xns", "xss", "12-3-", "123--", "123+-", "x23^^", "x2^3^",
        "2p*", "2p*q", "1l", "x2^", "2x^", "2x/", "x2/", "xx-", "12+3*", "12-3*", "x1+n", "x1-r",
        "2xv", "x2L", "1xA", "en", "fnr", "1fnr-", "p2^",
    ];
    for postfix in probes {
        for x in [0.5, 1.618_033_988_7, 2.506_628_274_631, -1.5, 3.25] {
            assert_infix_round_trip(postfix, x);
        }
    }

    // Bounded enumeration: every well-formed expression of a few symbols, so
    // nesting that the hand-written probes miss still has to round-trip.
    let atoms = ["1", "2", "x", "p", "e", "f"];
    let unaries = ["n", "r", "q", "s", "l", "E", "S", "C"];
    let binaries = ["+", "-", "*", "/", "^"];
    let mut depth1: Vec<String> = atoms.iter().map(|s| (*s).to_string()).collect();
    for atom in &atoms {
        for op in &unaries {
            depth1.push(format!("{atom}{op}"));
        }
    }
    let mut samples = depth1.clone();
    for left in &depth1 {
        for op in &unaries {
            samples.push(format!("{left}{op}"));
        }
    }
    for left in &depth1 {
        for right in &depth1 {
            for op in &binaries {
                samples.push(format!("{left}{right}{op}"));
            }
        }
    }
    let xs = [0.7, -1.3, 2.5];
    for postfix in samples {
        for x in xs {
            assert_infix_round_trip(&postfix, x);
        }
    }
}

fn search_targets() -> Vec<f64> {
    vec![
        2.506_628_274_631,
        1.618_033_988_7,
        std::f64::consts::PI,
        std::f64::consts::E,
        0.5,
        -0.5,
        -2.0,
        1.414_213_562_37,
        -1.414_213_562_37,
        0.123_456_789,
    ]
}

fn focused_gen_config() -> GenConfig {
    // Wide enough for sin(πx), cos(πx), ln, and √(2π), narrow enough to
    // enumerate. x + sinpi is weight 28; √(2π) is weight 40.
    GenConfig {
        max_lhs_complexity: 36,
        max_rhs_complexity: 42,
        max_length: 10,
        constants: vec![
            Symbol::X,
            Symbol::One,
            Symbol::Two,
            Symbol::Pi,
            Symbol::E,
            Symbol::Phi,
        ],
        unary_ops: vec![
            Symbol::Neg,
            Symbol::Recip,
            Symbol::Sqrt,
            Symbol::Square,
            Symbol::Ln,
            Symbol::Exp,
            Symbol::SinPi,
            Symbol::CosPi,
            Symbol::TanPi,
        ],
        binary_ops: vec![
            Symbol::Add,
            Symbol::Sub,
            Symbol::Mul,
            Symbol::Div,
            Symbol::Pow,
        ],
        ..GenConfig::default()
    }
}

#[test]
fn solved_expressions_match_reported_roots_across_targets() {
    let gen = focused_gen_config();
    let mut saw_nonprincipal = false;

    for target in search_targets() {
        let config = SearchConfig {
            target,
            max_matches: 24,
            max_error: (target.abs() * 0.02).max(1e-6),
            ..SearchConfig::default()
        };
        let (matches, _) = search_with_stats_and_config(&gen, &config);
        assert!(
            !matches.is_empty(),
            "expected at least one match for target {target}"
        );

        for m in &matches {
            assert_infix_round_trip(&m.lhs.expr.to_postfix(), target);
            assert_infix_round_trip(&m.rhs.expr.to_postfix(), target);

            let summary = MatchSummary::from_match(m);
            let Some(solved_text) = summary.solve_for_x else {
                continue;
            };
            let printed = solved_text
                .strip_prefix("x = ")
                .unwrap_or(solved_text.as_str());
            let value = Parser::eval(printed, m.x_value).unwrap_or_else(|err| {
                panic!(
                    "target {target}: printed solution {printed:?} (postfix {:?}) failed: {err}",
                    summary.solve_for_x_postfix
                )
            });
            let scale = m.x_value.abs().max(1.0);
            // The printed closed form has to land on the reported root, within
            // the error the match itself claims (with a floor for exact hits).
            let allowed = m.error.abs().max(1e-8 * scale);
            assert!(
                (value - m.x_value).abs() <= allowed,
                "target {target}: {printed} evaluates to {value}, but the match reports x = {} (error {})",
                m.x_value,
                m.error
            );
            // Principal trig inverses land in [-1, 1]. A solved value outside
            // that interval means a period was added.
            let lhs = m.lhs.expr.to_postfix();
            let periodic = lhs.contains('S') || lhs.contains('C') || lhs.contains('T');
            if periodic && value.abs() > 1.0 {
                saw_nonprincipal = true;
            }
        }
    }

    assert!(
        saw_nonprincipal,
        "the target spread never produced a non-principal solved expression"
    );
}
