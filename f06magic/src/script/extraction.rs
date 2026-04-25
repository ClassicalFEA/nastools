//! This simple sub-module implements the idea of an extraction.

use crate::utils::{AnyAmount, NumListRange};
use f06::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents a procedure for extracting values from an F06. Converts into a
/// real libf06 Extraction.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SimpleExtraction {
  /// Extraction name -- must be unique.
  pub(crate) name: String,
  /// Block types.
  #[serde(default)]
  #[serde(alias = "block")]
  pub(crate) blocks: AnyAmount<BlockType>,
  /// Subcase numbers.
  #[serde(default)]
  #[serde(alias = "subcase")]
  pub(crate) subcases: NumListRange<usize>,
  /// Grid point IDs.
  #[serde(default)]
  #[serde(alias = "node")]
  pub(crate) nodes: NumListRange<usize>,
  /// Element IDs.
  #[serde(default)]
  #[serde(alias = "element")]
  pub(crate) elements: NumListRange<usize>,
  /// Element types.
  #[serde(default)]
  #[serde(alias = "element_type")]
  pub(crate) element_types: AnyAmount<ElementType>,
  /// Degrees of freedom.
  #[serde(default)]
  #[serde(alias = "dofs")]
  pub(crate) dof: AnyAmount<Dof>,
  /// Raw column indices. Use with caution.
  #[serde(default)]
  #[serde(alias = "column", alias = "col", alias = "cols")]
  pub(crate) columns: AnyAmount<usize>,
  /// Raw row indices. Use with caution.
  #[serde(default)]
  #[serde(alias = "row")]
  pub(crate) rows: AnyAmount<usize>,
}

impl From<SimpleExtraction> for Extraction {
  fn from(value: SimpleExtraction) -> Self {
    return Extraction {
      subcases: value.subcases.into(),
      block_types: value.blocks.into(),
      grid_points: value.nodes.into_iter().map(GridPointRef::from).collect(),
      elements: value.elements.into_iter().map(ElementRef::from).collect(),
      rows: Specifier::All,
      cols: value.dof.into_iter().map(NasIndex::Dof).collect(),
      raw_cols: value.columns.into(),
      raw_rows: value.rows.into(),
      dxn: DisjunctionBehaviour::AssumeZeroes,
    };
  }
}
