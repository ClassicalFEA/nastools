//! Named bags of "debug expressions" attached to checks/comparisons.
//!
//! A `[[printout]]` table holds a `name` plus an arbitrary set of
//! `<label> = "<expression>"` pairs. The expressions reuse the
//! [`crate::script::predicate`] grammar. Checks and comparisons reference
//! one or more printouts via the `printout` / `printouts` field; when a
//! datum is flagged, the printout's expressions are evaluated against
//! that datum and appended (verbose mode only) to the failure line as
//! `printouts: label=value label=value ...`.
//!
//! Expressions are parsed once per usage site under that site's
//! [`crate::script::predicate::Scope`], so the same printout may be
//! reused across checks and comparisons (a reference-only variable
//! like `y`/`r` inside a printout attached to a check is rejected at
//! prepare time with a clear error).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::script::predicate::Predicate;

/// One `[[printout]]` table as read from TOML.
///
/// `name` identifies the printout; every other key/value pair in the
/// table is a `label = "expression"` debug entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SimplePrintout {
  /// The name used to reference this printout from checks/comparisons.
  pub(crate) name: String,
  /// `label -> raw expression`. Captured via `#[serde(flatten)]` so the
  /// labels are arbitrary user-chosen identifiers.
  #[serde(flatten)]
  pub(crate) vars: BTreeMap<String, String>,
}

/// A printout whose expressions have been parsed against the using
/// site's scope. The label order is the (sorted) `BTreeMap` order of
/// the source TOML.
#[derive(Debug)]
pub(crate) struct ResolvedPrintout {
  /// The source printout's name (useful for diagnostics).
  pub(crate) name: String,
  /// `(label, parsed expression)`, kept in the same order they were
  /// resolved.
  pub(crate) vars: Vec<(String, Predicate)>,
}

/// One evaluated printout value, ready to be rendered on the verbose
/// failure line.
#[derive(Clone, Debug)]
pub(crate) enum PrintoutValue {
  /// fasteval2 returned a numeric result.
  Number(f64),
  /// fasteval2 returned an error.
  Error(String),
}

impl PrintoutValue {
  /// Renders the value for the verbose `label=value` sub-line.
  ///
  /// Numbers use a fixed-decimal layout for magnitudes in `[1e-3, 1e6)`
  /// and scientific notation outside that range; `0`, `NaN`, and `\u{00b1}inf`
  /// are passed through verbatim. Error messages are truncated to keep
  /// the line scannable.
  pub(crate) fn render(&self) -> String {
    return match self {
      Self::Number(v) => render_number(*v),
      Self::Error(msg) => format!("<err:{}>", truncate(msg, 60)),
    };
  }
}

/// Renders an `f64` in a debug-friendly way (fixed-decimal for
/// "normal" magnitudes, scientific for extremes).
fn render_number(v: f64) -> String {
  if v.is_nan() {
    return "NaN".to_owned();
  }
  if v.is_infinite() {
    return if v.is_sign_positive() {
      "inf".to_owned()
    } else {
      "-inf".to_owned()
    };
  }
  if v == 0.0 {
    return "0".to_owned();
  }
  let mag = v.abs();
  if (1e-3..1e6).contains(&mag) {
    return format!("{v:.6}");
  }
  return format!("{v:.6e}");
}

/// Truncates a string to at most `max` characters, appending an
/// ellipsis when truncation happened.
fn truncate(s: &str, max: usize) -> String {
  if s.chars().count() <= max {
    return s.to_owned();
  }
  let head: String = s.chars().take(max).collect();
  return format!("{head}...");
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_simple_printout_toml() {
    let src = r#"
      name = "debug-bar"
      mozzarella = "x + 1"
      gorgonzola = "y * 2"
    "#;
    let p: SimplePrintout = toml::from_str(src).expect("parse");
    assert_eq!(p.name, "debug-bar");
    assert_eq!(p.vars.len(), 2);
    assert_eq!(p.vars.get("mozzarella").map(String::as_str), Some("x + 1"));
    assert_eq!(p.vars.get("gorgonzola").map(String::as_str), Some("y * 2"));
  }

  #[test]
  fn render_number_uses_fixed_for_normal_magnitudes() {
    assert_eq!(render_number(1.5), "1.500000");
    assert_eq!(render_number(-12.345), "-12.345000");
  }

  #[test]
  fn render_number_uses_scientific_for_extremes() {
    let small = render_number(1.234e-6);
    assert!(small.contains('e'), "expected scientific, got {small}");
    let big = render_number(2.5e9);
    assert!(big.contains('e'), "expected scientific, got {big}");
  }

  #[test]
  fn render_number_handles_specials() {
    assert_eq!(render_number(0.0), "0");
    assert_eq!(render_number(f64::NAN), "NaN");
    assert_eq!(render_number(f64::INFINITY), "inf");
    assert_eq!(render_number(f64::NEG_INFINITY), "-inf");
  }

  #[test]
  fn printout_value_render_error_is_truncated() {
    let long = "x".repeat(200);
    let pv = PrintoutValue::Error(long);
    let r = pv.render();
    assert!(r.starts_with("<err:"));
    assert!(r.ends_with("...>"));
    assert!(r.len() < 80);
  }
}
