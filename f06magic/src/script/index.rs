//! User-facing wrapper that lets a script write a row/column filter as a
//! plain string. The string is parsed lazily into a [`NasIndex`] when an
//! extraction is realised, so block context can be used to narrow ambiguous
//! values.

use std::fmt::Display;

use f06::prelude::*;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// A user-supplied row/column index. Holds the raw script string until it can
/// be resolved against a block context.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct LenientNasIndex {
  /// The raw string from the script.
  pub(crate) raw: String,
}

impl<'de> Deserialize<'de> for LenientNasIndex {
  fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
    let s = String::deserialize(d)?;
    if s.trim().is_empty() {
      return Err(de::Error::custom("empty index string"));
    }
    return Ok(Self { raw: s });
  }
}

impl Display for LenientNasIndex {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.raw);
  }
}

/// Resolves a [`LenientNasIndex`] into a real [`NasIndex`].
///
/// `narrow_to_block_types` lists the block types whose row/col index types
/// should be considered (used to disambiguate). If empty, all known index
/// types are tried.
///
/// `axis` selects whether the value is being interpreted as a row index, a
/// column index, or either (used when a single field accepts both).
pub(crate) fn resolve_index(
  lni: &LenientNasIndex,
  narrow_to_block_types: &[BlockType],
  axis: IndexAxis,
) -> Result<NasIndex, ParseLenientError> {
  let allow: Vec<&'static str> = narrow_to_block_types
    .iter()
    .flat_map(|bt| match axis {
      IndexAxis::Row => vec![bt.row_index_kind()],
      IndexAxis::Col => vec![bt.col_index_kind()],
      IndexAxis::Either => vec![bt.row_index_kind(), bt.col_index_kind()],
    })
    .collect();
  let narrow: Option<&[&str]> =
    if allow.is_empty() { None } else { Some(&allow) };
  return NasIndex::parse_lenient(&lni.raw, narrow);
}

/// Selects which set of index types a [`LenientNasIndex`] is being matched
/// against.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum IndexAxis {
  /// Row indices only.
  Row,
  /// Column indices only.
  Col,
  /// Either rows or columns.
  Either,
}
