//! This module implements comparison of blocks and the data within. The
//! `f06diff` tool is an example of this module's capabilities.

use std::collections::BTreeSet;
use std::fmt::Display;
use std::str::FromStr;

use clap::{Args, ValueEnum};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// This enumeration holds a reason why two blocks cannot be compared.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum IncompatibilityReason {
  /// The blocks aren't even of the same type.
  DifferentType,
  /// The blocks aren't the same subcase.
  DifferentSubcase,
  /// The blocks don't have the same column indexes.
  DifferentColumns,
  /// The blocks have no row indexes in common.
  NoCommonRows,
}

impl Display for IncompatibilityReason {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(
      f,
      "{}",
      match self {
        Self::DifferentType => "block types differ",
        Self::DifferentSubcase => "subcases differ",
        Self::DifferentColumns => "column sets differ",
        Self::NoCommonRows => "no rows in common",
      }
    );
  }
}

/// This enumeration is a "shallow" comparison of blocks -- the data isn't
/// compared, it's just to see what the blocks have in common, structurally
/// speaking.
#[derive(Clone, Debug, Serialize, Deserialize, derive_more::From)]
pub enum BlockCompatibility {
  /// The blocks are not compatible.
  Incompatible(IncompatibilityReason),
  /// The blocks are compatible for data comparison.
  Compatible {
    /// The rows the blocks have in common.
    common_rows: BTreeSet<NasIndex>,
    /// The rows one block has but the other one doesn't.
    disjunction: BTreeSet<NasIndex>,
  },
}

impl From<(&FinalBlock, &FinalBlock)> for BlockCompatibility {
  fn from((a, b): (&FinalBlock, &FinalBlock)) -> Self {
    if a.block_type != b.block_type {
      return IncompatibilityReason::DifferentType.into();
    }
    if a.subcase != b.subcase {
      return IncompatibilityReason::DifferentSubcase.into();
    }
    let aci = a.col_indices.keys().copied().collect::<BTreeSet<_>>();
    let bci = b.col_indices.keys().copied().collect::<BTreeSet<_>>();
    if aci != bci {
      return IncompatibilityReason::DifferentColumns.into();
    }
    let ari = a.row_indices.keys().copied().collect::<BTreeSet<_>>();
    let bri = b.row_indices.keys().copied().collect::<BTreeSet<_>>();
    let ixn = &ari & &bri;
    let dxn = &ari ^ &bri;
    if ixn.is_empty() {
      return IncompatibilityReason::NoCommonRows.into();
    }
    return Self::Compatible {
      common_rows: ixn,
      disjunction: dxn,
    };
  }
}

/// What to do when there's a row disunction (i.e. there are some rows that
/// appear in one block but not another.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DisjunctionBehaviour {
  /// Skip the disjunct row, do not include them in the comparison.
  Skip,
  /// Assume an all-zero row where it's missing.
  AssumeZeroes,
  /// Flag the row and column.
  Flag,
}

impl Display for DisjunctionBehaviour {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(
      f,
      "{}",
      match self {
        DisjunctionBehaviour::Skip => "skip",
        DisjunctionBehaviour::AssumeZeroes => "assume zeros",
        DisjunctionBehaviour::Flag => "flag",
      }
    );
  }
}

impl Default for DisjunctionBehaviour {
  fn default() -> Self {
    return Self::AssumeZeroes;
  }
}

impl FromStr for DisjunctionBehaviour {
  type Err = ();

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    return Self::all()
      .iter()
      .copied()
      .find(|v| s.eq_ignore_ascii_case(v.small_lc_name()))
      .ok_or(());
  }
}

impl ValueEnum for DisjunctionBehaviour {
  fn value_variants<'a>() -> &'a [Self] {
    return Self::all();
  }

  fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
    return Some(self.small_lc_name().into());
  }
}

impl DisjunctionBehaviour {
  /// Returns all variants.
  pub const fn all() -> &'static [Self] {
    return &[Self::Skip, Self::AssumeZeroes, Self::Flag];
  }

  /// Returns a small name for the variant (lower-case).
  pub const fn small_lc_name(&self) -> &'static str {
    return match self {
      DisjunctionBehaviour::Skip => "skip",
      DisjunctionBehaviour::AssumeZeroes => "zero",
      DisjunctionBehaviour::Flag => "flag",
    };
  }
}

/// Value testing/comparison criteria.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Args)]
pub struct Criteria {
  /// Test an absolute value difference?
  #[arg(long, short = 'd')]
  pub difference: Option<f64>,
  /// Test a big-to-small ratio?
  #[arg(long, short = 'r')]
  pub ratio: Option<f64>,
  /// Test a relative-percent error `100*|test/ref - 1|` against this
  /// tolerance? `a` (first argument to [`Criteria::check`]) is the reference.
  #[arg(long, short = 'p')]
  pub percent: Option<f64>,
  /// Optional near-zero floor that gates the percent check. When both
  /// `|ref|` and `|test|` are below this value the percent check passes;
  /// when exactly one is below, the pair is flagged as a floor asymmetry.
  /// Ignored unless `percent` is also set. `None` is treated as `0.0`.
  #[arg(long)]
  pub percent_floor: Option<f64>,
  /// Check for NaNs?
  #[arg(long)]
  pub nan: bool,
  /// Check for infinities?
  #[arg(long)]
  pub inf: bool,
  /// Check for differing signs?
  #[arg(long)]
  pub sig: bool,
}

impl Default for Criteria {
  fn default() -> Self {
    return Self {
      difference: None,
      ratio: None,
      percent: None,
      percent_floor: None,
      nan: true,
      inf: true,
      sig: false,
    };
  }
}

impl Criteria {
  /// Checks a pair of values against this set of criteria.
  pub fn check(&self, a: f64, b: f64) -> Option<FlagReason> {
    // check for NaNs
    if self.nan && (a.is_nan() || b.is_nan()) {
      return Some(FlagReason::NaN);
    }
    // check for infinities
    if self.inf && (a.is_infinite() || b.is_infinite()) {
      return Some(FlagReason::Infinity);
    }
    // check signs
    if self.sig && (a.signum() != b.signum()) {
      return Some(FlagReason::Signs);
    }
    // check difference
    if let Some(eps) = self.difference {
      let diff = (a - b).abs();
      if diff > eps {
        return Some(FlagReason::Difference {
          abs_difference: diff,
          max_epsilon: eps,
        });
      }
    }
    // check ratio
    if let Some(max_ratio) = self.ratio {
      let (big, small) = if a >= b { (a, b) } else { (b, a) };
      let rat = (big / small).abs();
      if rat > max_ratio {
        return Some(FlagReason::Ratio {
          big_to_small: rat,
          max_ratio,
        });
      }
    }
    // check percent (relative error). `a` is the reference, `b` is the
    // test value. The optional floor declares a noise band: when both
    // sides are inside it the pair is treated as agreeing; when exactly
    // one side is inside it the pair is flagged as a floor asymmetry;
    // when both are outside it the percent formula is applied normally.
    if let Some(max_percent) = self.percent {
      let floor = self.percent_floor.unwrap_or(0.0);
      let ar = a.abs();
      let br = b.abs();
      let a_small = ar < floor;
      let b_small = br < floor;
      if a_small && b_small {
        // both in the noise band: pass.
      } else if a_small != b_small {
        return Some(FlagReason::FloorAsymmetry {
          ref_val: a,
          test_val: b,
          floor,
        });
      } else {
        // both above floor (or floor unset): apply the formula. If `a`
        // is exactly zero the relative error is undefined; treat it as
        // `+inf` so the pair fails unless `b` is also zero.
        let pct = if a == 0.0 {
          if b == 0.0 {
            0.0
          } else {
            f64::INFINITY
          }
        } else {
          100.0 * (b / a - 1.0).abs()
        };
        if pct > max_percent {
          return Some(FlagReason::Percent {
            percent: pct,
            max_percent,
          });
        }
      }
    }
    // nothing? no flag
    return None;
  }
}

/// Holds a found value in two data blocks.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FoundValues {
  /// The row index.
  pub row: NasIndex,
  /// The column index.
  pub col: NasIndex,
  /// The value in block A.
  pub val_a: F06Number,
  /// The value in block B.
  pub val_b: F06Number,
}

/// The reason a value was flagged.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum FlagReason {
  /// Flagged due to an absolute difference.
  Difference {
    /// The absolute-value difference between the numbers.
    abs_difference: f64,
    /// The exceeded epsilon value.
    max_epsilon: f64,
  },
  /// Flagged due to an exceeded ratio.
  Ratio {
    /// The ratio between the larger and the smaller number.
    big_to_small: f64,
    /// The max ratio exceeded.
    max_ratio: f64,
  },
  /// Flagged due to an exceeded relative-percent error.
  Percent {
    /// The computed percent error `100*|test/ref - 1|`.
    percent: f64,
    /// The exceeded percent tolerance.
    max_percent: f64,
  },
  /// Flagged because exactly one of the two values was below the
  /// percent-check floor (i.e. one side is in the noise band, the other
  /// isn't).
  FloorAsymmetry {
    /// The reference value.
    ref_val: f64,
    /// The test value.
    test_val: f64,
    /// The floor (epsilon) that gated the check.
    floor: f64,
  },
  /// Flagged due to being a NaN.
  NaN,
  /// Flagged due to there being an infinity.
  Infinity,
  /// Signs differ!
  Signs,
  /// Row is misisng in one of the blocks.
  Disjunction,
}

impl Display for FlagReason {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(
      f,
      "{}",
      match self {
        FlagReason::Difference { .. } => "maximum difference exceeded",
        FlagReason::Ratio { .. } => "maximum ratio exceeded",
        FlagReason::Percent { .. } => "maximum percent error exceeded",
        FlagReason::FloorAsymmetry { .. } =>
          "one value inside near-zero floor, the other outside",
        FlagReason::NaN => "NaN detected",
        FlagReason::Infinity => "infinity detected",
        FlagReason::Signs => "signs differ",
        FlagReason::Disjunction => "value absent in one of the files",
      }
    );
  }
}

/// This structure holds a flagged difference in data.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct FlaggedPosition {
  /// The flagged values and their positions.
  pub values: FoundValues,
  /// The reason for flagging.
  pub reason: FlagReason,
}

/// This structure holds the necessary data to diff data blocks. It could be
/// made parallel, but there's been no need to make this parallel... for now.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct DataDiffer {
  /// The value-flagging criteria.
  pub criteria: Criteria,
  /// What to do when doing disjunct lines?
  pub dxn_behaviour: DisjunctionBehaviour,
}

impl DataDiffer {
  /// Instantiate a new DataDiffer with the given settings.
  pub fn new(criteria: Criteria, dxn_behaviour: DisjunctionBehaviour) -> Self {
    return Self {
      criteria,
      dxn_behaviour,
    };
  }

  /// Diff two data blocks and return flagged positions.
  pub fn compare<'a>(
    &'a self,
    a: &'a FinalBlock,
    b: &'a FinalBlock,
  ) -> Result<impl Iterator<Item = FlaggedPosition> + 'a, IncompatibilityReason>
  {
    let comp = BlockCompatibility::from((a, b));
    if let BlockCompatibility::Incompatible(reason) = comp {
      return Err(reason);
    }
    let get = |s: &FinalBlock,
               r: &NasIndex,
               c: &NasIndex|
     -> Result<Option<f64>, FlagReason> {
      if s.row_indices.contains_key(r) {
        return Ok(Some(s.get(*r, *c).unwrap().into()));
      } else {
        match self.dxn_behaviour {
          DisjunctionBehaviour::Skip => return Ok(None),
          DisjunctionBehaviour::AssumeZeroes => return Ok(Some(0.0)),
          DisjunctionBehaviour::Flag => return Err(FlagReason::Disjunction),
        }
      }
    };
    let row_indices = a
      .row_indices
      .keys()
      .chain(b.row_indices.keys())
      .copied()
      .collect::<BTreeSet<_>>();
    let col_indices = a.col_indices.keys().copied();
    return Ok(
      row_indices
        .into_iter()
        .cartesian_product(col_indices)
        .filter_map(move |(r, c)| {
          let mut fv = FoundValues {
            row: r,
            col: c,
            val_a: 0.0.into(),
            val_b: 0.0.into(),
          };
          match (get(a, &r, &c), get(b, &r, &c)) {
            // got both values
            (Ok(Some(x)), Ok(Some(y))) => {
              fv.val_a = x.into();
              fv.val_b = y.into();
              return self.criteria.check(x, y).map(|fr| FlaggedPosition {
                values: fv,
                reason: fr,
              });
            }
            (Ok(_), Ok(None)) | (Ok(None), Ok(_)) => {
              // got both values but at least one skip
              return None;
            }
            (_, Err(fr)) | (Err(fr), _) => {
              // at least one disjunction
              return Some(FlaggedPosition {
                values: fv,
                reason: fr,
              });
            }
          }
        }),
    );
  }
}
