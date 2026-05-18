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
}

/// Details of a single flagged datum within a comparison.
#[derive(Copy, Clone, Debug)]
pub(crate) struct FlaggedDetail {
  /// Value read from the reference file.
  pub(crate) ref_val: F06Number,
  /// Value read from the test file.
  pub(crate) test_val: F06Number,
  /// Why the criteria flagged the pair.
  pub(crate) reason: FlagReason,
}

/// The results from a run.
pub(crate) struct ComparisonResult {
  /// Indices checked.
  pub(crate) checked: BTreeSet<DatumIndex>,
  /// Indices flagged, mapped to per-datum detail.
  pub(crate) flagged: BTreeMap<DatumIndex, FlaggedDetail>,
}
