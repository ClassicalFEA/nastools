//! This simple submodule implements cross-F06 comparison.

use std::collections::{BTreeMap, BTreeSet};

use f06::prelude::{DatumIndex, F06Number, FlagReason};
use serde::{Deserialize, Serialize};

use crate::utils::OneOrMany;

/// A comparison takes two or more F06 files, extractions, and a criteria set.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Comparison {
  /// The name of this comparison.
  pub(crate) name: String,
  /// The name of the reference F06 file.
  pub(crate) reference_f06: String,
  /// The name of the test F06 file.
  pub(crate) test_f06: String,
  /// Data extractions to pull.
  #[serde(alias = "extraction")]
  pub(crate) extractions: OneOrMany<String>,
  /// Comparison criteria to apply.
  #[serde(alias = "criterion")]
  pub(crate) criteria: String,
  /// User-supplied boolean equation; flags the datum when the equation
  /// evaluates to `0.0` or `NaN`. See `script::equation` for the grammar.
  #[serde(default, alias = "formula", alias = "predicate")]
  pub(crate) equation: Option<String>,
}

/// Why a datum was flagged within a comparison. Wraps libf06's
/// [`FlagReason`] and adds an [`FlagReason2::Equation`] variant for
/// f06magic's user-supplied predicates.
#[derive(Clone, Debug)]
pub(crate) enum FlagReason2 {
  /// Flagged by the comparison criteria.
  Criteria(FlagReason),
  /// Flagged by a user-supplied equation.
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

/// Details of a single flagged datum within a comparison.
#[derive(Clone, Debug)]
pub(crate) struct FlaggedDetail {
  /// Value read from the reference file.
  pub(crate) ref_val: F06Number,
  /// Value read from the test file.
  pub(crate) test_val: F06Number,
  /// Why the criteria flagged the pair.
  pub(crate) reason: FlagReason2,
}

/// Which side of a comparison triggered an `allow_*_empty` violation for a
/// given extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmptySide {
  /// The reference file matched zero datums (and the extraction had
  /// `allow_reference_empty = false`).
  Reference,
  /// The test file matched zero datums (and the extraction had
  /// `allow_test_empty = false`).
  Test,
  /// Both files matched zero datums (and the extraction had
  /// `allow_empty = false`).
  Both,
}

impl EmptySide {
  /// Short human-readable label used in CLI output.
  pub(crate) fn label(self) -> &'static str {
    return match self {
      Self::Reference => "reference",
      Self::Test => "test",
      Self::Both => "both",
    };
  }
}

/// The results from a run.
pub(crate) struct ComparisonResult {
  /// Indices checked.
  pub(crate) checked: BTreeSet<DatumIndex>,
  /// Indices flagged, mapped to per-datum detail.
  pub(crate) flagged: BTreeMap<DatumIndex, FlaggedDetail>,
  /// Extractions referenced by this comparison that hit an `allow_*_empty`
  /// violation, paired with the side that fired. A single extraction may
  /// appear multiple times when more than one flag fires (e.g. both
  /// `allow_reference_empty = false` and `allow_test_empty = false` on a
  /// totally-empty extraction). Each entry counts as one comparison-level
  /// failure on top of `flagged`.
  pub(crate) empty_extractions: Vec<(String, EmptySide)>,
}
