//! This module implements the general structure of an F06 file as we interpret
//! it, and its submodules are responsible for specific parsing subroutines.

pub mod diff;
pub mod extraction;

use std::collections::{BTreeMap, BTreeSet};

use log::debug;
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::util::*;

/// This type stores a reference to a specific subcase and type (generally
/// used to refer to a specific block).
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  derive_more::From,
)]
pub struct BlockRef {
  /// The subcase. A value of 1 is pre-set for when the output file doesn't
  /// ever mention subcases.
  pub subcase: usize,
  /// The type of the block (or blocks).
  pub block_type: BlockType,
}

/// This is the output of an F06 parser.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct F06File {
  /// Original name of the file, if known and string-able.
  pub filename: Option<String>,
  /// The flavour of file.
  pub flavour: Flavour,
  /// The detected blocks.
  pub blocks: BTreeMap<BlockRef, Vec<FinalBlock>>,
  /// The line numbers for warning messages.
  pub warnings: BTreeMap<usize, String>,
  /// The line numbers for fatal error messages.
  pub fatal_errors: BTreeMap<usize, String>,
  /// Lines with potential, unknown headers, and their line ranges.
  pub potential_headers: BTreeSet<PotentialHeader>,
}

impl Default for F06File {
  fn default() -> Self {
    return Self::new();
  }
}

impl F06File {
  /// Instantiates a new F06 file struct with nothing inside.
  pub fn new() -> Self {
    return Self {
      filename: None,
      flavour: Flavour::default(),
      blocks: BTreeMap::new(),
      warnings: BTreeMap::new(),
      fatal_errors: BTreeMap::new(),
      potential_headers: BTreeSet::new(),
    };
  }

  /// Inserts a new block into the file.
  pub fn insert_block(&mut self, block: FinalBlock) {
    let br = block.block_ref();
    if let Some(ref mut vec) = self.blocks.get_mut(&br) {
      vec.push(block);
    } else {
      self.blocks.insert(br, vec![block]);
    }
  }

  /// Returns an iterator over all blocks, optionally only the unique ones
  /// (only one of their type in their subcase).
  pub fn all_blocks(&self, unique: bool) -> impl Iterator<Item = &FinalBlock> {
    return self
      .blocks
      .values()
      .filter(move |v| v.len() == 1 || !unique)
      .flatten();
  }

  /// Returns an iterator over mutable references of all blocks, optionally
  /// only the unique ones (only one fo their type in their subcase).
  pub fn all_blocks_mut(
    &mut self,
    unique: bool,
  ) -> impl Iterator<Item = &mut FinalBlock> {
    return self
      .blocks
      .values_mut()
      .filter(move |v| v.len() == 1 || !unique)
      .flatten();
  }

  /// Merges a vector of blocks having only a mutable reference to that vector.
  fn merge_block_vec(vec: &mut Vec<FinalBlock>, clean: bool) -> usize {
    let mut num_merges = 0;
    let mut new_vec: Vec<FinalBlock> = Vec::new();
    while let Some(primary) = vec.pop() {
      // look for merge candidates
      let sio: Option<usize> = vec
        .iter()
        .enumerate()
        .find(|(_, s)| {
          let can_merge = primary.can_merge(s);
          let conflicts = primary.row_conflicts(s);
          let full_ok = can_merge.is_ok() && (conflicts.is_empty() || !clean);
          if !full_ok {
            debug!("a merge failed! check this out:");
            debug!("{:?}", can_merge);
            debug!("{:?}", conflicts);
          }
          return full_ok;
        })
        .map(|t| t.0);
      if let Some(si) = sio {
        // at least one to merge
        let secondary = vec.remove(si);
        let res = primary.try_merge(secondary);
        let merged = match res {
          Ok(MergeResult::Success { merged }) => merged,
          Ok(MergeResult::Partial { .. }) => {
            panic!("partial merge not implemented yet!")
          }
          Err(x) => panic!("pre-merge check failed: {:#?}", x),
        };
        num_merges += 1;
        // put it back since it could have other potential merges
        vec.push(merged);
      } else {
        // unmergeable, put it in the new ones
        new_vec.push(primary);
      }
    }
    std::mem::swap(&mut new_vec, vec);
    return num_merges;
  }

  /// Locates blocks that can be merged and merges them. Returns the number of
  /// done merges. Clean merges mean no row conflicts.
  pub fn merge_blocks(&mut self, clean: bool) -> usize {
    return self
      .blocks
      .values_mut()
      .map(|v| Self::merge_block_vec(v, clean))
      .sum();
  }

  /// Merges the potential headers. Returns the number of merges.
  pub fn merge_potential_headers(&mut self) -> usize {
    let mut new_phs: BTreeSet<PotentialHeader> = BTreeSet::new();
    let mut num_merges: usize = 0;
    while !self.potential_headers.is_empty() {
      // take one
      let first = self.potential_headers.pop_first().unwrap();
      // take another
      if let Some(second) = self.potential_headers.pop_first() {
        // is the next one compatible?
        match first.try_merge(second) {
          Ok(merged) => {
            // merged, put it back, continue.
            self.potential_headers.insert(merged);
            num_merges += 1;
          }
          Err((first, second)) => {
            // couldn't merge. put the first one in the new set, put the second
            // one back, and try again.
            new_phs.insert(first);
            self.potential_headers.insert(second);
          }
        }
      } else {
        // there is no second. put the final one in the new set, and we're done
        new_phs.insert(first);
      }
    }
    std::mem::swap(&mut new_phs, &mut self.potential_headers);
    return num_merges;
  }

  /// Sorts the rows and columns of all blocks.
  pub fn sort_all_blocks(&mut self) {
    for block in self.all_blocks_mut(false) {
      block.sort_columns();
      block.sort_rows();
    }
  }

  /// Returns all the subcases.
  pub fn subcases(&self) -> impl Iterator<Item = usize> {
    return self
      .blocks
      .keys()
      .map(|k| k.subcase)
      .collect::<BTreeSet<usize>>()
      .into_iter();
  }

  /// Returns all the block types.
  pub fn block_types(&self) -> impl Iterator<Item = BlockType> {
    return self
      .blocks
      .keys()
      .map(|k| k.block_type)
      .collect::<BTreeSet<BlockType>>()
      .into_iter();
  }

  /// Searches blocks filtering by subcase and/or type.
  pub fn block_search(
    &self,
    type_filter: Option<BlockType>,
    subcase_filter: Option<usize>,
    unique: bool,
  ) -> impl Iterator<Item = &'_ FinalBlock> {
    return self
      .all_blocks(unique)
      .filter(move |b| type_filter.map(|t| b.block_type == t).unwrap_or(true))
      .filter(move |b| subcase_filter.map(|s| b.subcase == s).unwrap_or(true));
  }
}
