//! Implements checks: applies simple numerical rules for the values within
//! an extraction.

use std::collections::{BTreeMap, BTreeSet};

use f06::prelude::*;
use serde::{Deserialize, Serialize};

use crate::script::equation::{Equation, EvalOutcome, Stats};
use crate::utils::OneOrMany;

/// A check takes a single F06 file and verifies if all values within an
/// extraction follow some numerical rule.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Check {
  /// The name for this check.
  pub(crate) name: String,
  /// Files to pull.
  #[serde(alias = "file")]
  pub(crate) files: OneOrMany<String>,
  /// Data extractions to pull.
  #[serde(alias = "extraction")]
  pub(crate) extractions: OneOrMany<String>,
  /// All must equal a value.
  #[serde(default)]
  pub(crate) all_equal: Option<f64>,
  /// All must be within range.
  #[serde(default)]
  pub(crate) all_in_range: Option<(f64, f64)>,
  /// Values (sorted) must equal these exact values.
  #[serde(default)]
  pub(crate) exact_values: Option<Vec<f64>>,
  /// Values (sorted) must all fall within these ranges.
  #[serde(default)]
  pub(crate) ranges: Option<Vec<(f64, f64)>>,
  /// User-supplied boolean equation; PASS iff the equation evaluates to
  /// a non-zero, non-NaN value. See `script::equation` for the grammar.
  #[serde(default, alias = "formula", alias = "predicate")]
  pub(crate) equation: Option<String>,
}

/// Which check rule caused a value to be flagged, plus the relevant
/// expected/limit values for human-readable reporting.
#[derive(Clone, Debug)]
pub(crate) enum CheckRule {
  /// `all_equal` rule: every value must equal `expected`.
  AllEqual {
    /// The expected value.
    expected: f64,
  },
  /// `all_in_range` rule: every value must lie within `[lo, hi]`.
  AllInRange {
    /// Inclusive lower bound.
    lo: f64,
    /// Inclusive upper bound.
    hi: f64,
  },
  /// Positional `exact_values[idx]`: value at this position must equal
  /// `expected`.
  ExactValues {
    /// 0-based position in the extraction sequence.
    idx: usize,
    /// The expected value at this position.
    expected: f64,
  },
  /// Positional `ranges[idx]`: value at this position must lie within
  /// `[lo, hi]`.
  Ranges {
    /// 0-based position in the extraction sequence.
    idx: usize,
    /// Inclusive lower bound.
    lo: f64,
    /// Inclusive upper bound.
    hi: f64,
  },
  /// `equation` rule: the user-supplied predicate returned a fail value
  /// (`0.0` or `NaN`), or its evaluation produced an error.
  Equation {
    /// Original equation source.
    raw: String,
    /// Numeric result returned by the equation (`f64::NAN` when evaluation
    /// itself failed).
    value: f64,
    /// Optional error message when fasteval2 itself returned an error.
    error: Option<String>,
  },
}

/// Detail of a single flagged datum within a check.
#[derive(Clone, Debug)]
pub(crate) struct CheckFailure {
  /// The value read from the F06.
  pub(crate) value: F06Number,
  /// The rule that flagged the value.
  pub(crate) rule: CheckRule,
}

/// The results from a check run for one F06/extraction.
#[derive(Clone, Default, Debug)]
pub(crate) struct PartialCheckResult {
  /// Indices checked.
  pub(crate) checked: BTreeSet<DatumIndex>,
  /// Indices flagged, mapped to per-datum detail.
  pub(crate) flagged: BTreeMap<DatumIndex, CheckFailure>,
}

/// The full results from a check.
#[derive(Clone, Default, Debug)]
pub(crate) struct CheckResult {
  /// Links (file, extraction) to a PartialCheckResult.
  pub(crate) per_pair: BTreeMap<(String, String), PartialCheckResult>,
}

impl Check {
  /// Runs this check for a set of numbers, optionally evaluating a parsed
  /// `equation` against the precomputed `stats` (both must be `Some` or
  /// both `None`; the caller decides).
  pub(crate) fn run_for<I>(
    &self,
    numbers: I,
    equation: Option<&Equation>,
    stats: Option<&Stats>,
  ) -> PartialCheckResult
  where
    I: IntoIterator<Item = (DatumIndex, F06Number)>,
  {
    let mut results = PartialCheckResult::default();
    for (i, (di, x_tmp)) in numbers.into_iter().enumerate() {
      let x: f64 = x_tmp.into();
      results.checked.insert(di);
      if let Some(y) = self.all_equal
        && x != y
      {
        results.flagged.insert(
          di,
          CheckFailure {
            value: x_tmp,
            rule: CheckRule::AllEqual { expected: y },
          },
        );
        continue;
      }
      if let Some((a, b)) = self.all_in_range
        && (x < a || x > b)
      {
        results.flagged.insert(
          di,
          CheckFailure {
            value: x_tmp,
            rule: CheckRule::AllInRange { lo: a, hi: b },
          },
        );
        continue;
      }
      if let Some(v) = self.exact_values.as_ref()
        && let Some(y) = v.get(i)
        && x != *y
      {
        results.flagged.insert(
          di,
          CheckFailure {
            value: x_tmp,
            rule: CheckRule::ExactValues {
              idx: i,
              expected: *y,
            },
          },
        );
        continue;
      }
      if let Some(v) = self.ranges.as_ref()
        && let Some((a, b)) = v.get(i)
        && (x < *a || x > *b)
      {
        results.flagged.insert(
          di,
          CheckFailure {
            value: x_tmp,
            rule: CheckRule::Ranges {
              idx: i,
              lo: *a,
              hi: *b,
            },
          },
        );
        continue;
      }
      if let (Some(eq), Some(st)) = (equation, stats) {
        match eq.evaluate(x, None, st, None) {
          EvalOutcome::Pass { .. } => {}
          EvalOutcome::Fail { value } => {
            results.flagged.insert(
              di,
              CheckFailure {
                value: x_tmp,
                rule: CheckRule::Equation {
                  raw: eq.raw().to_owned(),
                  value,
                  error: None,
                },
              },
            );
            continue;
          }
          EvalOutcome::Error { message } => {
            results.flagged.insert(
              di,
              CheckFailure {
                value: x_tmp,
                rule: CheckRule::Equation {
                  raw: eq.raw().to_owned(),
                  value: f64::NAN,
                  error: Some(message),
                },
              },
            );
            continue;
          }
        }
      }
    }
    return results;
  }
}
