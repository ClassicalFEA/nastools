//! Implements checks: applies simple numerical rules for the values within
//! an extraction.

use std::collections::{BTreeMap, BTreeSet};

use f06::prelude::*;
use serde::{Deserialize, Serialize};

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
}

/// The results from a check run for one F06/extraction.
#[derive(Clone, Default, Debug)]
pub(crate) struct PartialCheckResult {
  /// Indices checked.
  pub(crate) checked: BTreeSet<DatumIndex>,
  /// Indices flagged.
  pub(crate) flagged: BTreeSet<DatumIndex>,
}

/// The full results from a check.
#[derive(Clone, Default, Debug)]
pub(crate) struct CheckResult {
  /// Links (file, extraction) to a PartialCheckResult.
  pub(crate) per_pair: BTreeMap<(String, String), PartialCheckResult>,
}

impl Check {
  /// Runs this check for a set of numbers.
  pub(crate) fn run_for<I>(&self, numbers: I) -> PartialCheckResult
  where
    I: IntoIterator<Item = (DatumIndex, F06Number)>,
  {
    let mut results = PartialCheckResult::default();
    for (i, (di, x_tmp)) in numbers.into_iter().enumerate() {
      let x: f64 = x_tmp.into();
      results.checked.insert(di);
      if self.all_equal.is_some_and(|y| x != y) {
        results.flagged.insert(di);
        continue;
      }
      if self.all_in_range.is_some_and(|(a, b)| (x < a || x > b)) {
        results.flagged.insert(di);
        continue;
      }
      if self
        .exact_values
        .as_ref()
        .is_some_and(|v| v.get(i).is_some_and(|y| x != *y))
      {
        results.flagged.insert(di);
        continue;
      }
      if self
        .ranges
        .as_ref()
        .is_some_and(|v| v.get(i).is_some_and(|(a, b)| (x < *a || x > *b)))
      {
        results.flagged.insert(di);
        continue;
      }
    }
    return results;
  }
}
