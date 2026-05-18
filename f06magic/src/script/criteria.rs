//! This simple submodule implements a numerical comparison criteria data
//! structure.

use f06::prelude::*;
use serde::{Deserialize, Serialize};

/// This struct contains numerical comparison criteria.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SimpleCriteria {
  /// The name for this criteria-set. Must be unique.
  pub(crate) name: String,
  /// Flag if abs(a-b) is above a threshold.
  #[serde(default)]
  pub(crate) max_difference: Option<f64>,
  /// Flag if abs(larger/smaller) is above a threshold.
  #[serde(default)]
  pub(crate) max_ratio: Option<f64>,
  /// Flag if `100*|test/ref - 1|` exceeds this percent tolerance.
  #[serde(default, alias = "percent", alias = "tolerance")]
  pub(crate) percent_tolerance: Option<f64>,
  /// Near-zero floor that gates the percent check. When both `|ref|` and
  /// `|test|` are below this value the percent check passes; when exactly
  /// one is below, the pair is flagged. Ignored unless `percent_tolerance`
  /// is set. Defaults to `0.0` when omitted.
  #[serde(default, alias = "floor")]
  pub(crate) epsilon: Option<f64>,
  /// Flag if signs differ.
  #[serde(default)]
  pub(crate) flag_different_signs: bool,
}

impl From<SimpleCriteria> for Criteria {
  fn from(value: SimpleCriteria) -> Self {
    return Self {
      difference: value.max_difference,
      ratio: value.max_ratio,
      percent: value.percent_tolerance,
      percent_floor: value.epsilon,
      nan: false,
      inf: false,
      sig: value.flag_different_signs,
    };
  }
}
