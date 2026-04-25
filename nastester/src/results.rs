//! This submodule defines structures for testing results.

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use f06::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::running::SolverPick;
use crate::suite::*;

/// Enum containing a deck status.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) enum RunState {
  /// Not yet run.
  #[default]
  Ready,
  /// Enqueued.
  Enqueued,
  /// Running
  Running,
  /// Finished, F06 file present.
  Finished(F06File),
  /// Run failed, contains error.
  Error(String),
}

impl<T: ToString> From<Result<F06File, T>> for RunState {
  fn from(value: Result<F06File, T>) -> Self {
    return match value {
      Ok(f) => Self::Finished(f),
      Err(e) => Self::Error(e.to_string()),
    };
  }
}

/// Single-column metrics (such as min, max, mean).
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub(crate) enum SingleColumnMetric {
  /// Minimum of a column.
  Mininum,
  /// Maximum of a column.
  Maximum,
  /// Average value of a column.
  Average,
  /// Standard deviation of a column.
  StandardDeviation,
}

impl SingleColumnMetric {
  /// Returns all the currently-implemented single-column metrics.
  pub(crate) const fn all() -> &'static [Self] {
    return &[
      Self::Mininum,
      Self::Maximum,
      Self::Average,
      Self::StandardDeviation,
    ];
  }

  /// Returns a short name for this metric.
  pub(crate) const fn short_name(&self) -> &'static str {
    return match self {
      Self::Mininum => "min",
      Self::Maximum => "max",
      Self::Average => "avg",
      Self::StandardDeviation => "sd",
    };
  }

  /// Returns a long name for this metric.
  pub(crate) const fn long_name(&self) -> &'static str {
    return match self {
      Self::Mininum => "minimum",
      Self::Maximum => "maximum",
      Self::Average => "average",
      Self::StandardDeviation => "standard deviation",
    };
  }

  /// Computes this metric over a block and columns.
  pub(crate) fn compute(
    &self,
    block: &FinalBlock,
    col: NasIndex,
  ) -> Option<f64> {
    let nums = block
      .row_indices
      .keys()
      .filter_map(|r| block.get(*r, col))
      .map(f64::from);
    match self {
      Self::Mininum => {
        return nums.min_by(|a, b| a.total_cmp(b));
      }
      Self::Maximum => {
        return nums.max_by(|a, b| a.total_cmp(b));
      }
      Self::Average => {
        let mut count: usize = 0;
        let mut total: f64 = 0.0;
        for num in nums {
          count += 1;
          total += num;
        }
        if count > 0 {
          return Some(total / count as f64);
        } else {
          return None;
        }
      }
      Self::StandardDeviation => {
        let avg = Self::Average.compute(block, col)?;
        let mut count: usize = 0;
        let mut total_qm: f64 = 0.0;
        for num in nums {
          count += 1;
          total_qm += (avg - num).powi(2);
        }
        if count > 0 {
          return Some(f64::sqrt(total_qm / count as f64));
        } else {
          return None;
        }
      }
    }
  }
}

/// Column-compare metrics (like the RMSD).
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub(crate) enum ColumnCompareMetric {
  /// Max absolute deviation.
  MaximumAbsoluteDifference,
  /// Average absolute deviation.
  AverageAbsoluteDifference,
  /// Root mean square deviation.
  RootMeanSquareDeviation,
}

impl ColumnCompareMetric {
  /// Returns all the currently-implemented single-column metrics.
  pub(crate) const fn all() -> &'static [Self] {
    return &[
      Self::MaximumAbsoluteDifference,
      Self::AverageAbsoluteDifference,
      Self::RootMeanSquareDeviation,
    ];
  }

  /// Returns a short name for this metric.
  pub(crate) const fn short_name(&self) -> &'static str {
    return match self {
      Self::MaximumAbsoluteDifference => "max-abs-diff",
      Self::AverageAbsoluteDifference => "avg-abs-diff",
      Self::RootMeanSquareDeviation => "rmsd",
    };
  }

  /// Returns a long name for this metric.
  pub(crate) const fn long_name(&self) -> &'static str {
    return match self {
      Self::MaximumAbsoluteDifference => "maximum absolute deviation",
      Self::AverageAbsoluteDifference => "average absolute deviation",
      Self::RootMeanSquareDeviation => "root mean square deviation",
    };
  }

  /// Computes this metric over a block and columns.
  pub(crate) fn compute(
    &self,
    ref_block: &FinalBlock,
    test_block: &FinalBlock,
    col: NasIndex,
  ) -> Option<f64> {
    let nums = ref_block.row_indices.keys().filter_map(|r| {
      if let Some(rval) = ref_block.get(*r, col) {
        if let Some(tval) = test_block.get(*r, col) {
          return Some((f64::from(rval), f64::from(tval)));
        }
      }
      return None;
    });
    match self {
      Self::MaximumAbsoluteDifference => {
        return nums
          .map(|(r, t)| (r - t).abs())
          .max_by(|a, b| a.total_cmp(b));
      }
      Self::AverageAbsoluteDifference => {
        let mut count: usize = 0;
        let mut total: f64 = 0.0;
        for (r, t) in nums {
          count += 1;
          total += (r - t).abs();
        }
        if count > 0 {
          return Some(total / count as f64);
        } else {
          return None;
        }
      }
      Self::RootMeanSquareDeviation => {
        let mut count: usize = 0;
        let mut total: f64 = 0.0;
        for (r, t) in nums {
          count += 1;
          total += (r - t).powi(2);
        }
        if count > 0 {
          return Some(f64::sqrt(total / count as f64));
        } else {
          return None;
        }
      }
    }
  }
}

/// Index to get a single-column metric.
pub(crate) type SingleColumnMetricIndex =
  (SolverPick, BlockRef, NasIndex, SingleColumnMetric);

/// Index to get a column-compare metric.
pub(crate) type ColumnCompareMetricIndex =
  (BlockRef, NasIndex, ColumnCompareMetric);

/// This structure holds extraction results: blocks and flagged indexes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ExtractionResults {
  /// The extraction number this is for.
  pub(crate) extraction_num: usize,
  /// The resulting sub-blocks gotten from the reference solver.
  pub(crate) blocks_ref: Vec<FinalBlock>,
  /// The resulting sub-blocks gotten from the solver under test.
  pub(crate) blocks_test: Vec<FinalBlock>,
  /// The flagged indices.
  pub(crate) flagged: Option<BTreeSet<DatumIndex>>,
  /// The extracted indices.
  pub(crate) extracted: BTreeSet<DatumIndex>,
  /// Single-column metrics.
  pub(crate) col_metrics: BTreeMap<SingleColumnMetricIndex, Option<f64>>,
  /// Column-compare metrics.
  pub(crate) col_compares: BTreeMap<ColumnCompareMetricIndex, Option<f64>>,
}

impl ExtractionResults {
  /// Returns the extracted blocks for a solver pick.
  pub(crate) fn blocks_of(&self, pick: SolverPick) -> &Vec<FinalBlock> {
    return match pick {
      SolverPick::Reference => &self.blocks_ref,
      SolverPick::Testing => &self.blocks_test,
    };
  }

  /// Returns the (reference, testing) block pair for a given block ref.
  pub(crate) fn block_pair(
    &self,
    block_ref: BlockRef,
  ) -> (Option<&FinalBlock>, Option<&FinalBlock>) {
    let r = self.blocks_ref.iter().find(|b| b.block_ref() == block_ref);
    let t = self.blocks_test.iter().find(|b| b.block_ref() == block_ref);
    return (r, t);
  }

  /// Returns an iterator over all block references present in either file.
  pub(crate) fn block_refs(&self) -> impl Iterator<Item = BlockRef> + '_ {
    return SolverPick::all()
      .iter()
      .flat_map(|p| self.blocks_of(*p).iter().map(|b| b.block_ref()));
  }

  /// Updates the single-column metrics.
  pub(crate) fn update_single_col_metrics(&mut self) {
    let indices = SolverPick::all()
      .iter()
      .flat_map(|p| {
        self
          .blocks_of(*p)
          .iter()
          .flat_map(move |b| b.col_indices.keys().map(move |ci| (*p, b, *ci)))
      })
      .flat_map(|(p, b, c)| {
        SingleColumnMetric::all()
          .iter()
          .map(move |scm| (p, b, c, *scm))
      });
    let mut new_scm: BTreeMap<_, Option<f64>> = BTreeMap::new();
    for (pick, block, col, metric) in indices {
      let true_index = (pick, block.block_ref(), col, metric);
      let value = metric.compute(block, col);
      new_scm.insert(true_index, value);
    }
    mem::swap(&mut self.col_metrics, &mut new_scm);
  }

  /// Updates the column-compare metrics.
  pub(crate) fn update_col_compare_metrics(&mut self) {
    let brs: BTreeSet<_> = self.block_refs().collect();
    let mut new_ccm: BTreeMap<_, Option<f64>> = BTreeMap::new();
    for block_ref in brs {
      if let (Some(r), Some(t)) = self.block_pair(block_ref) {
        for col in r.col_indices.keys() {
          for metric in ColumnCompareMetric::all() {
            let true_index = (block_ref, *col, *metric);
            let value = metric.compute(r, t, *col);
            new_ccm.insert(true_index, value);
          }
        }
      }
    }
    mem::swap(&mut self.col_compares, &mut new_ccm);
  }
}

/// These are the results for a single deck.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DeckResults {
  /// The F06 file from the reference solver.
  pub(crate) ref_f06: RunState,
  /// The F06 file from the solver under test.
  pub(crate) test_f06: RunState,
  /// Contains the results for each extraction.
  pub(crate) extractions: Vec<ExtractionResults>,
  /// Contains all flagged indices.
  pub(crate) flagged: BTreeSet<DatumIndex>,
  /// Contains all extracted indices.
  pub(crate) extracted: BTreeSet<DatumIndex>,
}

impl DeckResults {
  /// Gets a reference to a run state.
  pub(crate) fn get(&self, solver: SolverPick) -> &RunState {
    return match solver {
      SolverPick::Reference => &self.ref_f06,
      SolverPick::Testing => &self.test_f06,
    };
  }

  /// Gets a mutable reference to a run state.
  pub(crate) fn get_mut(&mut self, solver: SolverPick) -> &mut RunState {
    return match solver {
      SolverPick::Reference => &mut self.ref_f06,
      SolverPick::Testing => &mut self.test_f06,
    };
  }

  /// Clears all flagged values.
  pub(crate) fn clear_flags(&mut self) {
    for res in self.extractions.iter_mut() {
      res.flagged = None;
    }
    self.flagged.clear();
  }

  /// Clears a run's results.
  pub(crate) fn clear_of(&mut self, pick: SolverPick) {
    *self.get_mut(pick) = RunState::Ready;
    for res in self.extractions.iter_mut() {
      res.flagged = None;
      match pick {
        SolverPick::Reference => &mut res.blocks_ref,
        SolverPick::Testing => &mut res.blocks_test,
      }
      .clear();
      res.col_compares.clear();
      res.col_metrics.retain(|k, _| k.0 != pick);
    }
  }

  /// Recomputes the extraction results (sub-blocks and flagged values).
  pub(crate) fn recompute_extractions(
    &mut self,
    deck: &Deck,
    crit_sets: &BTreeMap<Uuid, NamedCriteria>,
  ) {
    self.extractions.clear();
    let pair = (&self.ref_f06, &self.test_f06);
    if let (RunState::Finished(r), RunState::Finished(t)) = pair {
      for (i, (exn, crit_uuid)) in deck.extractions.iter().enumerate() {
        let mut res = ExtractionResults {
          extraction_num: i,
          blocks_ref: exn.blockify(r),
          blocks_test: exn.blockify(t),
          flagged: None,
          extracted: BTreeSet::new(),
          col_metrics: BTreeMap::new(),
          col_compares: BTreeMap::new(),
        };
        // get extracted indices
        res.extracted.extend(exn.lookup(r));
        res.extracted.extend(exn.lookup(t));
        // recompute metrics
        res.update_single_col_metrics();
        res.update_col_compare_metrics();
        if let Some(critset) = crit_uuid.and_then(|u| crit_sets.get(&u)) {
          let in_ref = exn.lookup(r).collect::<BTreeSet<_>>();
          let in_test = exn.lookup(t).collect::<BTreeSet<_>>();
          let in_either =
            in_ref.union(&in_test).copied().collect::<BTreeSet<_>>();
          let dxn = in_ref
            .symmetric_difference(&in_test)
            .copied()
            .collect::<BTreeSet<_>>();
          let mut flagged: BTreeSet<DatumIndex> = BTreeSet::new();
          if exn.dxn == DisjunctionBehaviour::Flag {
            flagged.extend(dxn);
          }
          let get = |f: &F06File, ix: &DatumIndex| -> Option<F06Number> {
            let v = ix.get_from(f);
            if v.is_err() && exn.dxn == DisjunctionBehaviour::AssumeZeroes {
              return Some(0.0.into());
            } else {
              return Some(v.unwrap());
            }
          };
          for ix in in_either {
            let val_ref = get(r, &ix);
            let val_test = get(t, &ix);
            if let (Some(rv), Some(tv)) = (val_ref, val_test) {
              if critset.criteria.check(rv.into(), tv.into()).is_some() {
                flagged.insert(ix);
              }
            }
          }
          self.flagged.extend(flagged.iter().copied());
          res.flagged = Some(flagged);
        }
        self.extracted.extend(res.extracted.iter().copied());
        self.extractions.push(res);
      }
    }
  }

  /// Returns all block refs in the results set.
  pub(crate) fn all_block_refs(&self) -> Vec<BlockRef> {
    let mut v: Vec<BlockRef> = Vec::new();
    for pick in SolverPick::all() {
      if let RunState::Finished(f) = self.get(*pick) {
        v.extend(f.all_blocks(true).map(|b| b.block_ref()));
      }
    }
    v.sort();
    v.dedup();
    return v;
  }

  /// Returns the total number of flagged values.
  pub(crate) fn num_flagged(&self) -> usize {
    return self
      .extractions
      .iter()
      .filter_map(|er| er.flagged.as_ref().map(|v| v.len()))
      .sum();
  }
}
