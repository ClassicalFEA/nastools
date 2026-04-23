//! This module contains types and subroutines to produce CSV-ready data from
//! parsed F06 files.

use std::collections::BTreeMap;
use std::fmt::Display;

use f06::prelude::*;
use log::error;
use serde::{Deserialize, Serialize};

use crate::layout::*;
use crate::prelude::index_fns::*;

pub mod index_fns;
pub mod templates;

/// Functions used to convert NasIndexes into CSV fields.
pub type IndexFn = fn(NasIndex) -> Result<CsvField, ConversionError>;

/// Contains ten generators, to make a CSV row's worth of values.
pub type RowGenerator = [ColumnGenerator; 10];

/// Blank value for row headers.
pub(crate) const HBLANK: &str = "<UNUSED>";

/// A conversion error.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ConversionError {
  /// The wrong block type was passed.
  WrongBlockType {
    /// The type of the passed block.
    got: BlockType,
    /// The type of block the converter expected.
    expected: BlockType,
  },
  /// A value was missing from the block data.
  MissingDatum {
    /// The row we tried to access.
    row: NasIndex,
    /// The column we tried to access.
    col: NasIndex,
  },
  /// A row index has the wrong type (contains the index).
  BadRowIndexType(NasIndex),
  /// A column index has the wrong type (contains the index).
  BadColIndexType(NasIndex),
}

impl Display for ConversionError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::WrongBlockType { got, expected } => {
        write!(f, "wrong block type (got {}, expected {})", got, expected)
      }
      Self::MissingDatum { row, col } => {
        write!(f, "missing datum at ({}, {})", row, col)
      }
      Self::BadRowIndexType(ni) => {
        write!(f, "row index {} is of wrong/unexpected type", ni)
      }
      Self::BadColIndexType(ni) => {
        write!(f, "col index {} is of wrong/unexpected type", ni)
      }
    };
  }
}

/// A "column generator" -- a conversion template has ten of them.
/// They're called with a block and a row index, and also the file flavour.
#[derive(Copy, Clone, Debug)]
pub enum ColumnGenerator {
  /// Generate a blank column.
  Blank,
  /// Output the value from a specific column. Errs if absent.
  ColumnValue(NasIndex),
  /// Output a constant field.
  ConstantField(&'static CsvField),
  /// Outputs the grid point ID of the row, errs if absent.
  GridId,
  /// Outputs the element ID of the row, errs if absent.
  ElementId,
  /// Outputs the element type of the row, errs if absent.
  ElementType,
  /// Outputs some other function of the row index.
  RowIndexFn(&'static IndexFn),
  /// Output the block short name.
  BlockShortName,
  /// Output the block long name.
  BlockLongName,
  /// Output the solution type number (0 if absent/unknown).
  SolTypeNumber,
  /// Output the solution type name ("?" if absent/unknown).
  SolTypeName,
  /// Output the solver name ("Unknown" if absent/unknown).
  SolverName,
  /// Output the subcase.
  Subcase,
  // /// Output the Eigen Mode.
  // EigenMode,
  /// Output a constant number.
  ConstantNumber(F06Number),
  /// Output a constant string.
  ConstantString(&'static str),
  /// Runs another generator, with a default for errors.
  WithDefault(&'static ColumnGenerator, &'static CsvField),
}

impl ColumnGenerator {
  /// Calls the generator to produce a CSV field, or an error.
  pub fn convert(
    &self,
    block: &FinalBlock,
    flavour: Flavour,
    row: NasIndex,
  ) -> Result<CsvField, ConversionError> {
    return Ok(match self {
      Self::Blank => ().into(),
      Self::ColumnValue(col) => match block.get(row, *col) {
        Some(x) => x.into(),
        None => return Err(ConversionError::MissingDatum { row, col: *col }),
      },
      Self::ConstantField(cf) => (*cf).clone(),
      Self::GridId => return ixfn_gid(row),
      Self::ElementId => return ixfn_eid(row),
      Self::ElementType => return ixfn_etype(row),
      Self::RowIndexFn(f) => return f(row),
      Self::BlockShortName => block.block_type.short_name().to_owned().into(),
      Self::BlockLongName => block.block_type.to_string().into(),
      Self::SolTypeNumber => match flavour.soltype {
        Some(sol) => usize::from(sol).into(),
        None => "?".to_owned().into(),
      },
      Self::SolTypeName => match flavour.soltype {
        Some(sol) => sol.to_string(),
        None => "?".to_owned(),
      }
      .into(),
      Self::SolverName => match flavour.solver {
        Some(solver) => solver.to_string(),
        None => "Unknown".to_string(),
      }
      .into(),
      Self::Subcase => block.subcase.into(),
      Self::ConstantNumber(x) => (*x).into(),
      Self::ConstantString(s) => s.to_string().into(),
      Self::WithDefault(g, d) => {
        g.convert(block, flavour, row).unwrap_or((*d).clone())
      }
    });
  }
}

/// A template to convert an F06 block into a series of CSV records.
#[derive(Copy, Clone, Debug)]
pub struct BlockConverter {
  /// The block type this is meant for.
  pub input_block_type: BlockType,
  /// The type of CSV block this produces.
  pub output_block_id: CsvBlockId,
  /// Contains row generators, because a single data block row might produce
  /// more than one CSV row.
  pub generators: &'static [RowGenerator],
  /// The headers for the row this produces.
  pub headers: &'static [RowHeader],
}

impl BlockConverter {
  /// Begins conversion of a block into an iterator of CSV records. Need to
  /// know the file flavour though. Fields that cause an error when converting
  /// will issue an error log and turn into "<ERROR>" fields.
  pub fn convert_block<'a>(
    &'a self,
    block: &'a FinalBlock,
    flavour: &'a Flavour,
  ) -> Result<impl Iterator<Item = CsvRecord> + 'a, ConversionError> {
    if block.block_type != self.input_block_type {
      return Err(ConversionError::WrongBlockType {
        got: block.block_type,
        expected: self.input_block_type,
      });
    }
    return Ok(block.row_indexes.keys().flat_map(|row| {
      self.generators.iter().enumerate().map(|(irow, gens)| {
        let headers = &self.headers[irow];
        let mut fields: [CsvField; NAS_CSV_COLS - 1] = [
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
          CsvField::Blank,
        ];
        let mut gid: Option<usize> = None;
        let mut eid: Option<usize> = None;
        let mut etype: Option<ElementType> = None;
        let mut subcase: Option<usize> = None;
        for (i, cgen) in gens.iter().enumerate() {
          let fld = cgen.convert(block, *flavour, *row);
          if let Err(cverr) = fld {
            error!(
              concat!(
                "Error found when doing value #{} for csv-row #{} for {} in",
                "the {} block (subcase {}). Found error: {}. Attempted ",
                "conversion: {:?}."
              ),
              i + 2,
              irow + 1,
              *row,
              block.block_type.short_name(),
              block.subcase,
              cverr,
              cgen
            );
          }
          let flderr = fld.unwrap_or("<ERROR>".to_owned().into());
          let fld_nat: Option<_> = if let CsvField::Natural(n) = flderr {
            Some(n)
          } else {
            None
          };
          let fld_et: Option<_> = if let CsvField::ElementType(et) = flderr {
            Some(et)
          } else {
            None
          };
          if matches!(cgen, ColumnGenerator::GridId) && gid.is_none() {
            gid = fld_nat;
          }
          if matches!(cgen, ColumnGenerator::ElementId) && eid.is_none() {
            eid = fld_nat;
          }
          if matches!(cgen, ColumnGenerator::ElementType) && etype.is_none() {
            etype = fld_et;
          }
          if matches!(cgen, ColumnGenerator::Subcase) && subcase.is_none() {
            subcase = fld_nat;
          }
          fields[i] = flderr;
        }
        etype = etype.or(self.input_block_type.elem_type());
        return CsvRecord {
          block_id: self.output_block_id,
          block_type: Some(block.block_type),
          gid,
          eid,
          etype,
          subcase,
          fields,
          headers,
        };
      })
    }));
  }
}

/// Generates the 0-block for a file.
pub fn zeroth_block(file: &F06File) -> impl Iterator<Item = CsvRecord> + '_ {
  /// Name for unknown values
  const U: &str = "Unknown";
  /// Shorthand for ToString::to_string.
  fn ts<T: ToString>(t: T) -> String {
    return t.to_string();
  }
  // produce the key-value pairs
  let vvk: Vec<(&'static str, Option<String>)> = vec![
    ("Solver", file.flavour.solver.map(ts)),
    ("Solution", file.flavour.soltype.map(ts)),
    ("Filename", file.filename.clone()),
    ("#Subcases", Some(file.subcases().count().to_string())),
    ("#Warnings", Some(file.warnings.len().to_string())),
    ("#Fatals", Some(file.fatal_errors.len().to_string())),
    ("f06csv version", option_env!("CARGO_PKG_VERSION").map(ts)),
    ("f06csv authors", option_env!("CARGO_PKG_AUTHORS").map(ts)),
    ("Part of", Some("the MYSTRAN project".to_owned())),
  ];
  // make it into fields
  return vvk.into_iter().map(|(k, v)| CsvRecord {
    block_id: CsvBlockId::Metadata,
    block_type: None,
    gid: None,
    eid: None,
    etype: None,
    subcase: None,
    fields: [
      CsvField::String(k.to_owned()),
      CsvField::String(v.unwrap_or(U.to_owned())),
      CsvField::Blank,
      CsvField::Blank,
      CsvField::Blank,
      CsvField::Blank,
      CsvField::Blank,
      CsvField::Blank,
      CsvField::Blank,
      CsvField::Blank,
    ],
    headers: &[
      "Key", "Value", HBLANK, HBLANK, HBLANK, HBLANK, HBLANK, HBLANK, HBLANK,
      HBLANK,
    ],
  });
}

/// Generates all CSV records for a file.
pub fn to_records<'s>(
  file: &'s F06File,
  converters: &'s BTreeMap<BlockType, BlockConverter>,
) -> impl Iterator<Item = CsvRecord> + 's {
  // zeroth block
  let zeroth = zeroth_block(file);
  // sort the block refs by the output csv block id
  let mut block_refs = file.blocks.keys().collect::<Vec<_>>();
  block_refs.sort_by_key(|br| {
    converters
      .get(&br.block_type)
      .map(|c| usize::from(c.output_block_id))
      .unwrap_or(0)
  });
  // get the blocks in the correct order
  let blocks = block_refs
    .into_iter()
    .flat_map(|br| file.blocks.get(br).unwrap())
    .filter_map(|b| {
      converters
        .get(&b.block_type)
        .map(|c| c.convert_block(b, &file.flavour))
    })
    .flatten();
  return zeroth.chain(blocks.flatten());
}
