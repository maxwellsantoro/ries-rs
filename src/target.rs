//! Validation of decimal target precision at the f64 search boundary.
//!
//! Decimal integer arithmetic here is bounded by the literal length and the
//! finite f64 exponent range. It does not use a rounded f64 subtraction to
//! decide whether parsing discarded more than half the last typed place.

use std::cmp::Ordering;

fn multiply(digits: &mut Vec<u8>, factor: u8) {
    let mut carry = 0;
    for digit in digits.iter_mut().rev() {
        let n = *digit * factor + carry;
        *digit = n % 10;
        carry = n / 10;
    }
    if carry != 0 {
        digits.insert(0, carry);
    }
}

fn compare(a: &[u8], ae: i64, b: &[u8], be: i64) -> Ordering {
    let a = &a[a.iter().position(|d| *d != 0).unwrap_or(a.len())..];
    let b = &b[b.iter().position(|d| *d != 0).unwrap_or(b.len())..];
    if a.is_empty() || b.is_empty() {
        return (!a.is_empty()).cmp(&!b.is_empty());
    }
    (a.len() as i128 + i128::from(ae))
        .cmp(&(b.len() as i128 + i128::from(be)))
        .then_with(|| {
            (0..a.len().max(b.len()))
                .map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        })
}

/// Check an original decimal against the finite f64 used by the search.
///
/// Nonzero literals that underflow to zero are always rejected. With
/// `match_all_digits`, the exact binary value must lie within half the last
/// typed decimal place of the exact decimal literal. This does not make an
/// accepted search a proof of equality to that decimal or an intended constant.
/// Literal-free numeric APIs already supply f64 values and need no such check.
pub fn validate_decimal_target(
    target: f64,
    text: &str,
    match_all_digits: bool,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 4096 {
        return Err("Target decimal literal must contain 1..4096 characters".into());
    }
    if !target.is_finite() || text.parse::<f64>().ok() != Some(target) {
        return Err(
            "Target text must be a finite decimal literal matching the numeric target".into(),
        );
    }
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let exponent = exponent.parse::<i64>().unwrap_or_else(|_| {
        if exponent.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    });
    let places = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let mut digits: Vec<u8> = mantissa
        .bytes()
        .filter(|b| *b != b'.')
        .map(|b| b - b'0')
        .collect();
    let nonzero = digits.iter().any(|d| *d != 0);
    if target == 0.0 && nonzero {
        return Err(
            "Nonzero decimal target underflows to zero in f64; use a representable target".into(),
        );
    }
    if !match_all_digits || !nonzero {
        return Ok(());
    }
    let quantum = exponent.saturating_sub(places as i64);
    // Compare 2*f64 with [2*decimal - one last-place unit,
    //                      2*decimal + one last-place unit].
    multiply(&mut digits, 2);
    let mut lower = digits.clone();
    for digit in lower.iter_mut().rev() {
        if *digit > 0 {
            *digit -= 1;
            break;
        }
        *digit = 9;
    }
    let mut upper = digits;
    let mut carry = true;
    for digit in upper.iter_mut().rev() {
        if *digit < 9 {
            *digit += 1;
            carry = false;
            break;
        }
        *digit = 0;
    }
    if carry {
        upper.insert(0, 1);
    }

    let bits = target.abs().to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let significand = (bits & ((1_u64 << 52) - 1)) | if biased == 0 { 0 } else { 1_u64 << 52 };
    let power = if biased == 0 {
        -1074
    } else {
        biased - 1023 - 52
    } + 1;
    let mut exact: Vec<u8> = significand.to_string().bytes().map(|b| b - b'0').collect();
    let decimal_exponent = if power >= 0 {
        for _ in 0..power {
            multiply(&mut exact, 2);
        }
        0
    } else {
        for _ in 0..-power {
            multiply(&mut exact, 5);
        }
        i64::from(power)
    };
    if compare(&exact, decimal_exponent, &lower, quantum).is_lt()
        || compare(&exact, decimal_exponent, &upper, quantum).is_gt()
    {
        return Err("Decimal target loses more than the requested last-digit tolerance when parsed as f64; reduce the requested digits or omit --match-all-digits".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_decimal_rounding_without_f64_subtraction() {
        for text in [
            "1.000000000000000000000000000001",
            "-1.000000000000000000000000000001",
            "0.10000000000000000000",
        ] {
            assert!(
                validate_decimal_target(text.parse().unwrap(), text, true).is_err(),
                "{text}"
            );
        }
        for text in [
            "1.000000000000000000000000000000",
            "0.1",
            "1.0000000000000002",
            "-0.1",
            "3.141592653589793",
            "5e-324",
            "1.0000000000000002220446049250313080847263336181640625",
            "0e-999999999999999999999999999",
        ] {
            assert!(
                validate_decimal_target(text.parse().unwrap(), text, true).is_ok(),
                "{text}"
            );
        }
        for text in ["1e-400", "-1e-999999999999999999999999999"] {
            assert!(validate_decimal_target(0.0, text, false).is_err());
        }
        assert!(validate_decimal_target(1.0, "1.00000000000000000001", false).is_ok());
    }
}
