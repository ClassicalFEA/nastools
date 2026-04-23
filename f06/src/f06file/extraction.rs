//! This module implements data structures to specify ways to extract data
//! subsets from F06 files.

use std::error::Error;
use std::fmt::Display;
use std::mem::discriminant;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// This specifies a value or sets thereof.
#[derive(
  Debug, Clone, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq, Default,
)]
pub enum Specifier<A> {
  /// Use all in the file.
  #[default]
  All,
  /// Use a list.
  List(Vec<A>),
  /// Use an exclusion list.
  AllExcept(Vec<A>),
}

impl<A> FromIterator<A> for Specifier<A> {
  fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
    let v: Vec<A> = iter.into_iter().collect();
    if v.is_empty() {
      return Self::All;
    }
    return Self::List(v);
  }
}

/// This is a specifier type.
#[derive(
  Debug, Copy, Clone, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
)]
pub enum SpecifierType {
  /// Use all in the file.
  All,
  /// Use a list.
  List,
  /// Use an exclusion list.
  AllExcept,
}

impl SpecifierType {
  /// Returns a short name for this type of specifier.
  pub fn name(&self) -> &'static str {
    return match self {
      Self::All => "all",
      Self::List => "only",
      Self::AllExcept => "except",
    };
  }
}

impl Display for SpecifierType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.name());
  }
}

impl<A> Specifier<A> {
  /// Returns the type of this specifier.
  pub fn get_type(&self) -> SpecifierType {
    return match self {
      Specifier::All => SpecifierType::All,
      Specifier::List(_) => SpecifierType::List,
      Specifier::AllExcept(_) => SpecifierType::AllExcept,
    };
  }

  /// Tries to convert this to another type, preserving as much information
  /// as possible.
  pub fn set_type(&mut self, to: SpecifierType) {
    let vec = match self {
      Specifier::All => Vec::new(),
      Specifier::List(v) => std::mem::take(v),
      Specifier::AllExcept(v) => std::mem::take(v),
    };
    match to {
      SpecifierType::All => *self = Specifier::All,
      SpecifierType::List => *self = Specifier::List(vec),
      SpecifierType::AllExcept => *self = Specifier::AllExcept(vec),
    };
  }

  /// Returns a reference into the inner vector if there is one.
  pub fn inner_vec(&self) -> Option<&Vec<A>> {
    return match self {
      Specifier::All => None,
      Specifier::List(ref v) => Some(v),
      Specifier::AllExcept(ref v) => Some(v),
    };
  }

  /// Returns a mutable reference into the inner vector if there is one.
  pub fn inner_vec_mut(&mut self) -> Option<&mut Vec<A>> {
    return match self {
      Specifier::All => None,
      Specifier::List(ref mut v) => Some(v),
      Specifier::AllExcept(ref mut v) => Some(v),
    };
  }
}

impl<A: Clone> Specifier<A> {
  /// Returns a clone with another type.
  pub fn with_type(&self, to: SpecifierType) -> Self {
    let mut clone = self.clone();
    clone.set_type(to);
    return clone;
  }
}

impl<A: PartialEq> Specifier<A> {
  /// Use this as a filter for an iterator.
  pub fn filter_fn(&self, item: &A) -> bool {
    return match self {
      Self::All => true,
      Self::List(l) => l.is_empty() || l.contains(item),
      Self::AllExcept(l) => l.is_empty() || !l.contains(item),
    };
  }

  /// Use this as a lax filter (None means fail but All means All).
  pub fn lax_filter(&self, item: &Option<A>) -> bool {
    match self {
      Specifier::All => return true,
      Specifier::List(v) | Self::AllExcept(v) => {
        if v.is_empty() {
          return true;
        }
      }
    };
    return match item {
      Some(v) => self.filter_fn(v),
      None => false,
    };
  }

  /// Use this as a strict filter (None means fail).
  pub fn strict_filter(&self, item: &Option<A>) -> bool {
    return match item {
      Some(v) => self.filter_fn(v),
      None => false,
    };
  }
}

/// This is a "full index", it refers to a single datum in an F06 file.
#[derive(
  Debug, Copy, Clone, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
)]
pub struct DatumIndex {
  /// The block reference.
  pub block_ref: BlockRef,
  /// The row.
  pub row: NasIndex,
  /// The column.
  pub col: NasIndex,
}

impl DatumIndex {
  /// Attempts to get the value at this index from a data block.
  pub fn get_from(&self, file: &F06File) -> Result<F06Number, ExtractionError> {
    let block = file
      .block_search(
        Some(self.block_ref.block_type),
        Some(self.block_ref.subcase),
        true,
      )
      .nth(0)
      .ok_or(ExtractionError::NoSuchBlock(self.block_ref))?;
    let ri_ex = block
      .row_indexes
      .keys()
      .nth(0)
      .ok_or(ExtractionError::BlockIsEmpty)?;
    let ci_ex = block
      .col_indexes
      .keys()
      .nth(0)
      .ok_or(ExtractionError::BlockIsEmpty)?;
    if discriminant(&self.row) != discriminant(ri_ex) {
      return Err(ExtractionError::RowTypeMismatch {
        tried: self.row,
        against: *ri_ex,
      });
    }
    if discriminant(&self.col) != discriminant(ci_ex) {
      return Err(ExtractionError::ColumnTypeMismatch {
        tried: self.col,
        against: *ci_ex,
      });
    }
    if !block.row_indexes.contains_key(&self.row) {
      return Err(ExtractionError::MissingRow(self.row));
    }
    if !block.col_indexes.contains_key(&self.col) {
      return Err(ExtractionError::MissingColumn(self.col));
    }
    return Ok(
      block
        .get(self.row, self.col)
        .expect("row & col check failed!"),
    );
  }
}

/// This is the kind of error that can be returned when extracting a datum.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum ExtractionError {
  /// The F06 file has no block matching a subcase and type.
  NoSuchBlock(BlockRef),
  /// The block was found but there was an index type mismatch for the rows.
  RowTypeMismatch {
    /// A row index we tried to use.
    tried: NasIndex,
    /// An example of row index from the block.
    against: NasIndex,
  },
  /// The block was found but there was an index type mismatch for the columns.
  ColumnTypeMismatch {
    /// A column index we tried to use.
    tried: NasIndex,
    /// An example of column index from the block.
    against: NasIndex,
  },
  /// A row index is the correct type but is not part of the matrix.
  MissingRow(NasIndex),
  /// A column index is the correct type but is not part of the matrix.
  MissingColumn(NasIndex),
  /// The block has no data.
  BlockIsEmpty,
}

impl Display for ExtractionError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::NoSuchBlock(bref) => write!(
        f,
        "no such block ({}, subcase {})",
        bref.block_type.short_name(),
        bref.subcase
      ),
      Self::RowTypeMismatch { tried, against } => write!(
        f,
        "wrong row type (tried a {}, block uses {})",
        tried.type_name(),
        against.type_name()
      ),
      Self::ColumnTypeMismatch { tried, against } => write!(
        f,
        "wrong column type (tried a {}, block uses {})",
        tried.type_name(),
        against.type_name()
      ),
      Self::MissingRow(ri) => write!(f, "no such row ({ri})"),
      Self::MissingColumn(ci) => write!(f, "no such column ({ci})"),
      Self::BlockIsEmpty => write!(f, "block is empty"),
    };
  }
}

impl Error for ExtractionError {}

/// This structure represents a way to extract a subset of the data from an F06
/// so one can apply comparison criteria to it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Extraction {
  /// Subcases to get data from.
  pub subcases: Specifier<usize>,
  /// Block types to get data from.
  pub block_types: Specifier<BlockType>,
  /// Grid point filter (filters out grid points if present).
  pub grid_points: Specifier<GridPointRef>,
  /// Element filter (filters out element IDs if present).
  pub elements: Specifier<ElementRef>,
  /// Row filter (for when you want very specific data).
  pub rows: Specifier<NasIndex>,
  /// Column filter (for when you want very specific data).
  pub cols: Specifier<NasIndex>,
  /// Raw column filter (for ease of separation).
  pub raw_cols: Specifier<usize>,
  /// What to do in case of disjunctions.
  pub dxn: DisjunctionBehaviour,
}

impl Extraction {
  /// Produces an iterator over the indices resulting from applying an
  /// extraction to a file. This assumes the file has already had its blocks
  /// sorted and merged.
  pub fn lookup<'f>(
    &'f self,
    file: &'f F06File,
  ) -> impl Iterator<Item = DatumIndex> + 'f {
    return file
      .all_blocks(true)
      .filter(|b| self.subcases.filter_fn(&b.subcase))
      .filter(|b| self.block_types.filter_fn(&b.block_type))
      .flat_map(|b| {
        let rows = b
          .row_indexes
          .keys()
          .filter(|ri| self.rows.filter_fn(ri))
          .filter(|ri| self.grid_points.lax_filter(&ri.grid_point_id()))
          .filter(|ri| self.elements.lax_filter(&ri.element_id()));
        let cols = b
          .col_indexes
          .keys()
          .filter(|ci| self.cols.filter_fn(ci))
          .filter(|ci| self.grid_points.lax_filter(&ci.grid_point_id()))
          .filter(|ci| self.elements.lax_filter(&ci.element_id()))
          .filter(|ci| self.raw_cols.filter_fn(b.col_indexes.get(ci).unwrap()));
        return rows.cartesian_product(cols).map(|(ri, ci)| DatumIndex {
          block_ref: BlockRef {
            subcase: b.subcase,
            block_type: b.block_type,
          },
          row: *ri,
          col: *ci,
        });
      });
  }

  /// Produces a series of `FinalBlock`s from extracting data from a file.
  pub fn blockify(&self, file: &F06File) -> Vec<FinalBlock> {
    let mut subs: Vec<FinalBlock> = Vec::new();
    let compatible_blocks = file
      .all_blocks(true)
      .filter(|b| self.subcases.filter_fn(&b.subcase))
      .filter(|b| self.block_types.filter_fn(&b.block_type));
    for block in compatible_blocks {
      let mut clone = block.clone();
      let rows: Vec<NasIndex> = clone
        .row_indexes
        .keys()
        .filter(|ri| self.rows.filter_fn(ri))
        .filter(|ri| self.grid_points.lax_filter(&ri.grid_point_id()))
        .filter(|ri| self.elements.lax_filter(&ri.element_id()))
        .copied()
        .collect();
      let cols: Vec<NasIndex> = clone
        .col_indexes
        .keys()
        .filter(|ci| self.cols.filter_fn(ci))
        .filter(|ci| self.grid_points.lax_filter(&ci.grid_point_id()))
        .filter(|ci| self.elements.lax_filter(&ci.element_id()))
        .filter(|ci| {
          self.raw_cols.filter_fn(clone.col_indexes.get(ci).unwrap())
        })
        .copied()
        .collect();
      clone.row_indexes.retain(|ri, _| rows.contains(ri));
      clone.col_indexes.retain(|ci, _| cols.contains(ci));
      subs.push(clone);
    }
    return subs;
  }
}
