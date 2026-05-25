//! This module implements tools and structures to use when comparing F06 files
//! (especially meant for the `f06diff` tool).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;

#[cfg(feature = "clap")]
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// This enum encodes reasons why blocks within a file were not compared.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NonCompareReason {
  /// There were no counterparts to this blockref in one of the files.
  /// Contains the filename (if provided).
  NoCounterpart(Option<String>),
  /// That blockref was not unique in one of the files.
  NotUniqueInOne(Option<String>),
  /// That blockref was not unique in either file.
  NotUniqueInBoth,
  /// The blockref was unique in both files, but the blocks were not
  /// compatible.
  NotCompatible(IncompatibilityReason),
}

impl Display for NonCompareReason {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      NonCompareReason::NoCounterpart(Some(file)) => {
        write!(f, "no counterpart in {file}")
      }
      NonCompareReason::NoCounterpart(None) => {
        write!(f, "no counterpart in one of the files")
      }
      NonCompareReason::NotUniqueInOne(Some(file)) => {
        write!(f, "not unique in {file}")
      }
      NonCompareReason::NotUniqueInOne(None) => {
        write!(f, "not unique in one of the files")
      }
      NonCompareReason::NotUniqueInBoth => {
        write!(f, "not unique in either file")
      }
      NonCompareReason::NotCompatible(reason) => {
        write!(f, "incompatibility: {reason}")
      }
    };
  }
}

/// This contains the settings for when you need to compare two files.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "clap", derive(Args))]
pub struct DiffSettings {
  /// The criteria for comparing numbers.
  #[cfg_attr(feature = "clap", command(flatten))]
  pub criteria: Criteria,
  /// What to do with line disjunctions?
  #[cfg_attr(feature = "clap", arg(short = 'x'))]
  #[cfg_attr(feature = "clap", clap(default_value = "zero"))]
  pub dxn_behaviour: Option<DisjunctionBehaviour>,
  /// Limit for the number of flagged values per block (0 for no limit)
  #[cfg_attr(feature = "clap", clap(default_value = "0"))]
  #[cfg_attr(feature = "clap", arg(short = 'F'))]
  pub max_flags: Option<usize>,
}

impl From<DiffSettings> for DataDiffer {
  fn from(value: DiffSettings) -> Self {
    return Self {
      criteria: value.criteria,
      dxn_behaviour: value.dxn_behaviour.unwrap_or_default(),
    };
  }
}

/// This structure holds the differences found between two F06Files.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct F06Diff {
  /// Blocks that were compared and the positions that were flagged.
  pub compared: BTreeMap<BlockRef, Vec<FlaggedPosition>>,
  /// Blocks that were not compared due to their being incompatible.
  pub not_compared: BTreeMap<BlockRef, NonCompareReason>,
}

impl F06Diff {
  /// Diffs two `F06File`s.
  pub fn compare(settings: &DiffSettings, a: &F06File, b: &F06File) -> Self {
    // init inners
    let mut compared: BTreeMap<BlockRef, Vec<FlaggedPosition>>;
    let mut not_compared: BTreeMap<BlockRef, NonCompareReason>;
    compared = BTreeMap::new();
    not_compared = BTreeMap::new();
    let differ: DataDiffer = (*settings).into();
    let brs = a
      .blocks
      .keys()
      .chain(b.blocks.keys())
      .collect::<BTreeSet<_>>();
    for br in brs {
      let ta: Vec<FinalBlock> = Vec::new();
      let tb: Vec<FinalBlock> = Vec::new();
      let va = a.blocks.get(br).unwrap_or(&ta);
      let vb = b.blocks.get(br).unwrap_or(&tb);
      let afn = a.filename.clone();
      let bfn = b.filename.clone();
      match (va.len(), vb.len()) {
        (0, 0) => panic!("block type missing in both files?!"),
        (0, 1) => {
          not_compared.insert(*br, NonCompareReason::NoCounterpart(afn));
        }
        (1, 0) => {
          not_compared.insert(*br, NonCompareReason::NoCounterpart(bfn));
        }
        (1, 1) => {
          let block_a = va.first().unwrap();
          let block_b = vb.first().unwrap();
          if let Ok(flags) = differ.compare(block_a, block_b) {
            let mf = settings.max_flags.unwrap_or(0);
            if mf == 0 {
              compared.insert(*br, flags.collect());
            } else {
              compared.insert(*br, flags.take(mf).collect());
            }
          }
        }
        (_, 1) => {
          not_compared.insert(*br, NonCompareReason::NotUniqueInOne(afn));
        }
        (1, _) => {
          not_compared.insert(*br, NonCompareReason::NotUniqueInOne(bfn));
        }
        (_, _) => {
          not_compared.insert(*br, NonCompareReason::NotUniqueInBoth);
        }
      };
    }
    return Self {
      compared,
      not_compared,
    };
  }
}
