//! This simple sub-module implements the idea of an extraction.

use crate::script::errors::ScriptValidationError;
use crate::script::index::{resolve_index, IndexAxis, LenientNasIndex};
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
  /// Column index filter (block-specific). Each entry is a string parsed
  /// against the block(s)'s column index type. Accepts an optional
  /// `type:value` prefix to force a specific subtype.
  #[serde(default)]
  #[serde(alias = "col", alias = "column", alias = "dofs", alias = "dof")]
  pub(crate) cols: AnyAmount<LenientNasIndex>,
  /// Row index filter (block-specific). Same syntax as `cols`.
  #[serde(default)]
  #[serde(alias = "row")]
  pub(crate) rows: AnyAmount<LenientNasIndex>,
  /// Raw column indices. Use only when `cols` cannot express the filter.
  #[serde(default)]
  pub(crate) raw_cols: AnyAmount<usize>,
  /// Raw row indices. Use only when `rows` cannot express the filter.
  #[serde(default)]
  pub(crate) raw_rows: AnyAmount<usize>,
}

impl SimpleExtraction {
  /// Resolves this simple extraction into a real [`Extraction`], reporting a
  /// helpful error if any row/column index cannot be parsed or does not match
  /// the row/column index type of the configured block(s).
  pub(crate) fn resolve(self) -> Result<Extraction, ScriptValidationError> {
    let block_types: Vec<BlockType> =
      (&self.blocks).into_iter().copied().collect();
    let resolve_axis =
      |entries: &AnyAmount<LenientNasIndex>,
       axis: IndexAxis|
       -> Result<Vec<NasIndex>, ScriptValidationError> {
        let mut out: Vec<NasIndex> = Vec::new();
        for lni in entries.into_iter() {
          let parsed = resolve_index(lni, &block_types, axis).map_err(|e| {
            ScriptValidationError::IndexParse {
              extraction: self.name.clone(),
              axis,
              raw: lni.raw.clone(),
              cause: e,
            }
          })?;
          if !block_types.is_empty() {
            let kind_name = parsed.type_name();
            let mut allowed_for_axis: Vec<&'static str> = block_types
              .iter()
              .map(|bt| match axis {
                IndexAxis::Row => bt.row_index_kind(),
                IndexAxis::Col => bt.col_index_kind(),
                IndexAxis::Either => bt.row_index_kind(),
              })
              .collect();
            if axis == IndexAxis::Either {
              allowed_for_axis
                .extend(block_types.iter().map(|bt| bt.col_index_kind()));
            }
            if !allowed_for_axis.contains(&kind_name) {
              return Err(ScriptValidationError::IndexKindMismatch {
                extraction: self.name.clone(),
                axis,
                raw: lni.raw.clone(),
                got: kind_name,
                expected: allowed_for_axis
                  .into_iter()
                  .map(|s| s.to_owned())
                  .collect(),
              });
            }
          }
          out.push(parsed);
        }
        return Ok(out);
      };
    let cols = resolve_axis(&self.cols, IndexAxis::Col)?;
    let rows = resolve_axis(&self.rows, IndexAxis::Row)?;
    return Ok(Extraction {
      subcases: self.subcases.into(),
      block_types: self.blocks.into(),
      grid_points: self.nodes.into_iter().map(GridPointRef::from).collect(),
      elements: self.elements.into_iter().map(ElementRef::from).collect(),
      rows: rows.into_iter().collect(),
      cols: cols.into_iter().collect(),
      raw_cols: self.raw_cols.into(),
      raw_rows: self.raw_rows.into(),
      dxn: DisjunctionBehaviour::AssumeZeroes,
    });
  }
}
