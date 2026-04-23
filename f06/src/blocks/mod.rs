//! This submodule defines the blocks that make up an F06 file.

pub mod compare;
pub(crate) mod decoders;
pub mod indexing;
pub mod types;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::mem::discriminant;

use log::warn;
use nalgebra::{Const, DMatrix, Dyn, Matrix, Scalar, VecStorage};
use num::Zero;
use serde::{Deserialize, Serialize};

use crate::blocks::types::BlockType;
use crate::flavour::Flavour;
use crate::prelude::BlockRef;
use indexing::{IndexType, NasIndex};

/// This trait encapsulates the necessary properties for a scalar that can exist
/// in the data matrices.
pub trait NasScalar: Copy + Scalar + Zero {}

impl NasScalar for f64 {}
impl NasScalar for isize {}
impl NasScalar for usize {}

/// This type encapsulates a dynamic matrix of scalar type S and width W.
pub type DynMatx<S, const W: usize> =
  Matrix<S, Dyn, Const<W>, VecStorage<S, Dyn, Const<W>>>;

/// Full-dynamic matrix used in finalised blocks.
#[derive(Clone, Debug, Serialize, Deserialize, derive_more::From)]
pub enum FinalDMat {
  /// Matrix with real values.
  Reals(DMatrix<f64>),
  /// Matrix with integer values.
  Integers(DMatrix<isize>),
  /// Matrix with natural values.
  Naturals(DMatrix<usize>),
}

impl FinalDMat {
  /// Swaps two rows.
  pub fn swap_rows(&mut self, a: usize, b: usize) {
    match self {
      FinalDMat::Reals(m) => m.swap_rows(a, b),
      FinalDMat::Integers(m) => m.swap_rows(a, b),
      FinalDMat::Naturals(m) => m.swap_rows(a, b),
    };
  }

  /// Swaps two columns.
  pub fn swap_columns(&mut self, a: usize, b: usize) {
    match self {
      FinalDMat::Reals(m) => m.swap_columns(a, b),
      FinalDMat::Integers(m) => m.swap_columns(a, b),
      FinalDMat::Naturals(m) => m.swap_columns(a, b),
    };
  }

  /// Number of rows.
  pub fn nrows(&self) -> usize {
    return match self {
      FinalDMat::Reals(m) => m.nrows(),
      FinalDMat::Integers(m) => m.nrows(),
      FinalDMat::Naturals(m) => m.nrows(),
    };
  }

  /// Number of columns.
  pub fn ncols(&self) -> usize {
    return match self {
      FinalDMat::Reals(m) => m.ncols(),
      FinalDMat::Integers(m) => m.ncols(),
      FinalDMat::Naturals(m) => m.ncols(),
    };
  }
}

/// Value inside a FinalDMat.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialEq,
  PartialOrd,
  derive_more::From,
)]
pub enum F06Number {
  /// Real value.
  Real(f64),
  /// Integer value.
  Integer(isize),
  /// Natural value.
  Natural(usize),
}

impl From<F06Number> for f64 {
  fn from(value: F06Number) -> Self {
    return match value {
      F06Number::Real(x) => x,
      F06Number::Integer(i) => i as f64,
      F06Number::Natural(n) => n as f64,
    };
  }
}

impl Display for F06Number {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      F06Number::Real(x) => x.fmt(f),
      F06Number::Integer(i) => i.fmt(f),
      F06Number::Natural(n) => n.fmt(f),
    };
  }
}

/// A block that contains an indexing type, some details, and a data matrix.
/// The number of columns is fixed -- F06 data don't grow horizontally. Types:
///   - S: the scalar type for the data within.
///   - R: the type of abstract index for the rows.
///   - C: the type of abstract index for the columns.
///   - W: the width of the matrix, a constant.
#[derive(Clone, Debug)]
pub(crate) struct RowBlock<
  S: NasScalar,
  R: IndexType,
  C: IndexType,
  const W: usize,
> {
  /// The row indexes.
  row_indexes: BTreeMap<R, usize>,
  /// The column indexes.
  col_indexes: BTreeMap<C, usize>,
  /// The data within.
  data: Option<DynMatx<S, W>>,
}

impl<S, R, C, const W: usize> RowBlock<S, R, C, W>
where
  S: NasScalar,
  R: IndexType,
  C: IndexType,
{
  /// Creates a new RowBlock with a set width and a pre-allocated size.
  pub(crate) fn new(col_indexes: BTreeMap<C, usize>) -> Self {
    let row_indexes: BTreeMap<R, usize> = BTreeMap::new();
    return Self {
      row_indexes,
      col_indexes,
      data: None,
    };
  }

  /// Inserts a line raw into the data matrix, without fixing indexes. Returns
  /// the row within the underlying matrixes this was put in.
  pub(crate) fn insert_raw(&mut self, row_index: R, row: &[S; W]) -> usize {
    if self.row_indexes.contains_key(&row_index) {
      warn!(
        "tried to insert the same line twice! index: {:?}",
        row_index
      );
    }
    let irow: usize;
    if let Some(mut mat) = self.data.take() {
      if let Some(fnd) = self.row_indexes.get(&row_index) {
        irow = *fnd;
      } else {
        irow = mat.nrows();
        mat = mat.insert_row(irow, S::zero());
      }
      mat.row_mut(irow).copy_from_slice(row);
      self.data = Some(mat);
    } else {
      irow = 0;
      let mat = DynMatx::<S, W>::from_row_slice(row);
      self.data = Some(mat);
    }
    self.row_indexes.insert(row_index, irow);
    return irow;
  }

  /// Returns the column indexes as known by the RowBlock. This way, you can
  /// set up your slices adequately and pass them directly into insert_row_raw,
  /// which is much, much faster.
  pub(crate) fn col_indexes(&self) -> &BTreeMap<C, usize> {
    return &self.col_indexes;
  }

  /// Returns the row indexes.
  pub(crate) fn row_indexes(&self) -> &BTreeMap<R, usize> {
    return &self.row_indexes;
  }

  /// Inserts a line into the data matrix, but passing column indexes because
  /// you might not know the underlying column order.
  pub(crate) fn insert_row(
    &mut self,
    row_index: R,
    data: &BTreeMap<C, S>,
  ) -> usize {
    let mut raw_data = [S::zero(); W];
    data.iter().for_each(|(c, s)| {
      let ri = self.col_indexes.get(c).expect("bad col index");
      raw_data[*ri] = *s;
    });
    return self.insert_raw(row_index, &raw_data);
  }
}

impl<S, R, C, const W: usize> RowBlock<S, R, C, W>
where
  S: NasScalar,
  R: IndexType,
  C: IndexType,
  FinalDMat: From<DMatrix<S>>,
{
  /// Consumes this data structure and unwraps the matrix within. This is to
  /// avoid inconsistent data ever residing within this block.
  pub(crate) fn finalise(
    self,
    block_type: BlockType,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    let row_indexes: BTreeMap<NasIndex, usize> = self
      .row_indexes
      .into_iter()
      .map(|(k, v)| (k.into(), v))
      .collect();
    let col_indexes: BTreeMap<NasIndex, usize> = self
      .col_indexes
      .into_iter()
      .map(|(k, v)| (k.into(), v))
      .collect();
    let data: Option<FinalDMat> = self.data.map(|m| {
      let nr = m.nrows();
      let nc = m.ncols();
      return FinalDMat::from(m.reshape_generic(Dyn(nr), Dyn(nc)));
    });
    return FinalBlock {
      block_type,
      line_range,
      subcase,
      row_indexes,
      col_indexes,
      data,
    };
  }
}

/// Contains the result of an attempt to merge two blocks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MergeResult {
  /// Merge was successful.
  Success {
    /// The block with the merged data.
    merged: FinalBlock,
  },
  /// Found conflicting rows -- non-conflicting were put in the the primary.
  Partial {
    /// The block with the merged data.
    merged: FinalBlock,
    /// The block with the remaining data.
    residue: FinalBlock,
    /// The rows skipped due to their already being in the primary.
    skipped: BTreeSet<NasIndex>,
  },
}

/// The incompatibilities that can happen when attempting to merge two
/// FinalBlocks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeIncompatible {
  /// Merge found conflicting columns and did nothing.
  ColumnConflict {
    /// Columns missing in the primary.
    missing_in_primary: BTreeSet<NasIndex>,
    /// Columns missing in the secondary.
    missing_in_secondary: BTreeSet<NasIndex>,
  },
  /// Blocks were not the same type.
  BlockTypeMismatch,
  /// Matrices did not have the same type of scalar.
  ScalarMismatch,
  /// Subcases don't match.
  SubcaseMismatch,
}

/// Immutable view into a result block once it's finalised.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinalBlock {
  /// The line range, if at all known.
  pub line_range: Option<(usize, usize)>,
  /// The block type that originated the data.
  pub block_type: BlockType,
  /// The subcase where this block appears.
  pub subcase: usize,
  /// The row indexes.
  pub row_indexes: BTreeMap<NasIndex, usize>,
  /// The column indexes.
  pub col_indexes: BTreeMap<NasIndex, usize>,
  /// The data within.
  pub data: Option<FinalDMat>,
}

impl FinalBlock {
  /// Returns the data at a certain location.
  pub fn get<R: Into<NasIndex>, C: Into<NasIndex>>(
    &self,
    row: R,
    col: C,
  ) -> Option<F06Number> {
    let ri = self.row_indexes.get(&row.into())?;
    let ci = self.col_indexes.get(&col.into())?;
    return Some(match self.data {
      Some(FinalDMat::Reals(ref m)) => F06Number::Real(*m.get((*ri, *ci))?),
      Some(FinalDMat::Integers(ref m)) => {
        F06Number::Integer(*m.get((*ri, *ci))?)
      }
      Some(FinalDMat::Naturals(ref m)) => {
        F06Number::Natural(*m.get((*ri, *ci))?)
      }
      None => return None,
    });
  }

  /// Returns this's blocks BlockRef for adding into files.
  pub fn block_ref(&self) -> BlockRef {
    return BlockRef {
      subcase: self.subcase,
      block_type: self.block_type,
    };
  }

  /// Swaps two columns and updates the column indexes array.
  pub fn swap_columns(&mut self, a: NasIndex, b: NasIndex) {
    let aio = self.col_indexes.get(&a).copied();
    let bio = self.col_indexes.get(&b).copied();
    match (&mut self.data, aio, bio) {
      (Some(ref mut fdm), Some(ai), Some(bi)) => {
        fdm.swap_columns(ai, bi);
        self.col_indexes.insert(a, bi);
        self.col_indexes.insert(b, ai);
      }
      _ => return,
    };
  }

  /// Swaps two rows and updates the row indexes array.
  pub fn swap_rows(&mut self, a: NasIndex, b: NasIndex) {
    let aio = self.row_indexes.get(&a).copied();
    let bio = self.row_indexes.get(&b).copied();
    match (&mut self.data, aio, bio) {
      (Some(ref mut fdm), Some(ai), Some(bi)) => {
        fdm.swap_rows(ai, bi);
        self.row_indexes.insert(a, bi);
        self.row_indexes.insert(b, ai);
      }
      _ => return,
    };
  }

  /// Swaps columns in the underlying matrix (updating the column indexes map
  /// accordingly) so that the real row indexes grow monotonically with the
  /// high-level indexes.
  pub fn sort_columns(&mut self) {
    let nixes: Vec<NasIndex> = self.col_indexes.keys().copied().collect();
    let mut ns: Vec<usize> = self.col_indexes.values().copied().collect();
    ns.sort();
    for (nix, i) in nixes.into_iter().zip(ns.into_iter()) {
      let nswap = self
        .col_indexes
        .iter()
        .find(|p| p.1 == &i)
        .map(|p| *(p.0))
        .expect("couldn't reverse index when sorting columns");
      self.swap_columns(nix, nswap);
    }
  }

  /// Swaps rows in the underlying matrix (updating the row indexes map
  /// accordingly) so that the real row indexes grow monotonically with the
  /// high-level indexes.
  pub fn sort_rows(&mut self) {
    let nixes: Vec<NasIndex> = self.row_indexes.keys().copied().collect();
    let mut ns: Vec<usize> = self.row_indexes.values().copied().collect();
    ns.sort();
    for (nix, i) in nixes.into_iter().zip(ns.into_iter()) {
      let nswap = self
        .row_indexes
        .iter()
        .find(|p| p.1 == &i)
        .map(|p| *(p.0))
        .expect("couldn't reverse index when sorting rows");
      self.swap_rows(nix, nswap);
    }
  }

  /// Checks for merge compatibility.
  pub fn can_merge(&self, other: &Self) -> Result<(), MergeIncompatible> {
    // check for same type
    if self.block_type != other.block_type {
      return Err(MergeIncompatible::BlockTypeMismatch);
    }
    // check for same subcase
    if self.subcase != other.subcase {
      return Err(MergeIncompatible::SubcaseMismatch);
    }
    // check for same columns
    let primary_col_set: BTreeSet<NasIndex> =
      self.col_indexes.keys().copied().collect();
    let secondary_col_set: BTreeSet<NasIndex> =
      other.col_indexes.keys().copied().collect();
    let missing_in_primary = &secondary_col_set - &primary_col_set;
    let missing_in_secondary = &primary_col_set - &secondary_col_set;
    if !missing_in_primary.is_empty() || !missing_in_secondary.is_empty() {
      return Err(MergeIncompatible::ColumnConflict {
        missing_in_primary,
        missing_in_secondary,
      });
    }
    match (&self.data, &other.data) {
      (Some(ms), Some(mo)) if discriminant(ms) != discriminant(mo) => {
        return Err(MergeIncompatible::ScalarMismatch);
      }
      _ => return Ok(()),
    };
  }

  /// Returns the row indexes this has in common with another.
  pub fn row_conflicts(&self, other: &Self) -> BTreeSet<NasIndex> {
    let primary_row_set: BTreeSet<&NasIndex> =
      self.row_indexes.keys().collect();
    let secondary_row_set: BTreeSet<&NasIndex> =
      other.row_indexes.keys().collect();
    return primary_row_set
      .intersection(&secondary_row_set)
      .copied()
      .copied()
      .collect();
  }

  /// Copies lines from another block into this one.
  pub fn try_merge(
    mut self,
    mut other: FinalBlock,
  ) -> Result<MergeResult, MergeIncompatible> {
    // check for compatibility
    self.can_merge(&other)?;
    // sort columns in both so we can just move stuff
    self.sort_columns();
    other.sort_columns();
    /// Copies rows from one matrix to another
    fn row_copy<S: NasScalar>(
      mut p: DMatrix<S>,
      s: &DMatrix<S>,
      si: usize,
    ) -> DMatrix<S> {
      let pi = p.nrows();
      p = p.insert_row(pi, S::zero());
      p.set_row(pi, &s.row(si));
      return p;
    }
    match (self.data, other.data) {
      (None, None) => {
        // both empty. return whichever (primary)
        self.data = None;
        return Ok(MergeResult::Success { merged: self });
      }
      (None, Some(od)) => {
        // only secondary is nonempty. return secondary.
        other.data = Some(od);
        return Ok(MergeResult::Success { merged: other });
      }
      (Some(sd), None) => {
        // only primary is nonempty. return primary.
        self.data = Some(sd);
        return Ok(MergeResult::Success { merged: self });
      }
      (Some(dp), Some(ds)) => {
        // both nonempty. copy data from primary to secondary.
        // check for which indexes we're gonna copy
        let primary_row_set: BTreeSet<NasIndex> =
          self.row_indexes.keys().copied().collect();
        let secondary_row_set: BTreeSet<NasIndex> =
          other.row_indexes.keys().copied().collect();
        let copied = &secondary_row_set - &primary_row_set;
        let skipped = &secondary_row_set - &copied;
        let to_copy =
          copied.iter().map(|ci| other.row_indexes.get(ci).unwrap());
        // copy data
        let (ndp, nds) = match (dp, ds) {
          (FinalDMat::Reals(mut p), FinalDMat::Reals(s)) => {
            for (si, ci) in to_copy.zip(copied.iter()) {
              self.row_indexes.insert(*ci, p.nrows());
              p = row_copy(p, &s, *si)
            }
            (FinalDMat::Reals(p), FinalDMat::Reals(s))
          }
          (FinalDMat::Integers(mut p), FinalDMat::Integers(s)) => {
            for (si, ci) in to_copy.zip(copied.iter()) {
              self.row_indexes.insert(*ci, p.nrows());
              p = row_copy(p, &s, *si)
            }
            (FinalDMat::Integers(p), FinalDMat::Integers(s))
          }
          (FinalDMat::Naturals(mut p), FinalDMat::Naturals(s)) => {
            for (si, ci) in to_copy.zip(copied.iter()) {
              self.row_indexes.insert(*ci, p.nrows());
              p = row_copy(p, &s, *si)
            }
            (FinalDMat::Naturals(p), FinalDMat::Naturals(s))
          }
          _ => return Err(MergeIncompatible::ScalarMismatch),
        };
        // un-move stuff (this is stupid)
        self.data = Some(ndp);
        other.data = Some(nds);
        // delete line range
        self.line_range = None;
        // return accordingly
        if skipped.is_empty() {
          return Ok(MergeResult::Success { merged: self });
        } else {
          return Ok(MergeResult::Partial {
            merged: self,
            residue: other,
            skipped,
          });
        }
      }
    }
  }
}

/// Response of a block parser upon receiving a line.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum LineResponse {
  /// The supplied line contained no useful information.
  Useless,
  /// The supplied line contained useful metadata, but was not a data line.
  Metadata,
  /// The supplied line contained useful data I've put into the matrix.
  Data,
  /// The supplied line makes me think this block is finished.
  Done,
  /// The flavour is incompatible (e.g. unknown solver)
  BadFlavour,
  /// The supplied line looks like it contains data but I lack the necessary
  /// metadata to decode it.
  MissingMetadata,
  /// The supplied line makes me think I'm the wrong decoder but right solver.
  WrongDecoder,
  /// The supplied line makes me think you got the solver wrong.
  WrongSolver,
  /// Unsupported data format.
  Unsupported,
  /// Something's done terribly wrong.
  Abort,
}

impl LineResponse {
  /// Returns true if the response was abnormal.
  pub const fn abnormal(&self) -> bool {
    return !matches!(
      self,
      Self::Useless | Self::Metadata | Self::Data | Self::Done
    );
  }
}

/// This trait is implemented by all known output block decoders. It aids with
/// code uniformity.
pub(crate) trait BlockDecoder {
  /// The type of scalar in the data matrix.
  type MatScalar: NasScalar;
  /// The type of row index.
  type RowIndex: IndexType;
  /// The type of column index.
  type ColumnIndex: IndexType;
  /// The width of the actual data matrix -- doesn't count indexes.
  const MATWIDTH: usize;
  /// The block type this decoder is for.
  const BLOCK_TYPE: BlockType;

  /// Initializes the decoder.
  fn new(flavour: Flavour) -> Self;

  /// Unwraps the underlying data.
  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock;

  /// Called to hint to you what the header is, maybe it'll be useful.
  /// It can also tell you to drop it immediately by returning false.
  fn good_header(&mut self, _header: &str) -> bool {
    return true;
  }

  /// Called to hint about the last used index. Useful for catching paging.
  fn hint_last(&mut self, _last: NasIndex) {}

  /// Returns the last inserted index. Useful for stateful decoders.
  fn last_row_index(&self) -> Option<NasIndex> {
    return None;
  }

  /// Consumes a line into the underlying data.
  fn consume(&mut self, line: &str) -> LineResponse;
}

/// This trait is used to hide implementation details of a block decoder.
pub trait OpaqueDecoder {
  /// Returns the block type this decoder is for.
  fn block_type(&self) -> BlockType;

  /// This function takes in a line and loads it into the decoder.
  fn consume(&mut self, line: &str) -> LineResponse;

  /// Called to hint to you what the header is, maybe it'll be useful.
  fn good_header(&mut self, header: &str) -> bool;

  /// Called to hint about the last used index. Useful for catching paging.
  fn hint_last(&mut self, last: NasIndex);

  /// Returns the last inserted index. Useful for stateful decoders.
  fn last_index(&self) -> Option<NasIndex>;

  /// Extracts the data within.
  fn finalise(
    self: Box<Self>,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock;
}

impl<T> OpaqueDecoder for T
where
  T: BlockDecoder,
  FinalDMat: From<DMatrix<T::MatScalar>>,
{
  fn block_type(&self) -> BlockType {
    return Self::BLOCK_TYPE;
  }

  fn hint_last(&mut self, last: NasIndex) {
    return BlockDecoder::hint_last(self, last);
  }

  fn last_index(&self) -> Option<NasIndex> {
    return BlockDecoder::last_row_index(self);
  }

  fn finalise(
    self: Box<Self>,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.unwrap(subcase, line_range);
  }

  fn good_header(&mut self, header: &str) -> bool {
    return BlockDecoder::good_header(self, header);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    return BlockDecoder::consume(self, line);
  }
}
