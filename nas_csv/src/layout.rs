//! This submodule implements the general layout of CSV files.

#![allow(clippy::needless_return)] // i'll never forgive rust for this

use std::fmt::Display;

#[cfg(feature = "clap")]
use clap::builder::PossibleValue;
#[cfg(feature = "clap")]
use clap::ValueEnum;
use f06::prelude::*;
use f06::util::fmt_f64;
use serde::{Deserialize, Serialize};

/// Number of fields in a fixed-form CSV record.
pub const NAS_CSV_COLS: usize = 11;

/// Type that holds the headers for a row.
pub type RowHeader = [&'static str; NAS_CSV_COLS - 1];

/// CSV block IDs based on their content.]
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum CsvBlockId {
  /// The 0-block: general solution info; subcase IDs, solution types, etc.
  Metadata,
  /// The 1-block: displacements.
  Displacements,
  /// The 2-block: stresses.
  Stresses,
  /// The 3-block: strains.
  Strains,
  /// The 4-block: element engineering forces.
  EngForces,
  /// The 5-block: grid point force balance.
  GridPointForces,
  /// The 6-block: applied forces.
  AppliedForces,
  /// The 7-block: forces of single-point constraint.
  SpcForces,
  /// The 8-block: eigenvectors.
  Eigenvectors,
  /// The 9-block: real eigenvalues.
  Eigenvalues,
}

// this impl allow numerical shorthands
#[cfg(feature = "clap")]
impl ValueEnum for CsvBlockId {
  fn value_variants<'a>() -> &'a [Self] {
    return Self::all();
  }

  fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
    let mut pv: PossibleValue = self.shorthand().into();

    pv = pv.aliases(self.aliases());

    let help = self.help_string();
    let pad_len = CsvBlockId::longest_help_len();
    pv = pv.help(format!("{help:pad_len$} aliases: {:?}", self.aliases()));

    return Some(pv);
  }
}

impl CsvBlockId {
  /// Returns all known block IDs.
  pub const fn all() -> &'static [Self] {
    return &[
      Self::Metadata,
      Self::Displacements,
      Self::Stresses,
      Self::Strains,
      Self::EngForces,
      Self::GridPointForces,
      Self::AppliedForces,
      Self::SpcForces,
      Self::Eigenvectors,
      Self::Eigenvalues,
    ];
  }

  /// Returns the maximum length in bytes of all help strings.
  #[cfg(feature = "clap")]
  pub(crate) const fn longest_help_len() -> usize {
    let mut max = 0;
    let mut i = 0;
    while i < Self::all().len() {
      let help_len = Self::all()[i].help_string().len();
      if help_len > max {
        max = help_len
      }
      i += 1;
    }
    max
  }

  /// Returns a short help string for the specified case
  pub const fn help_string(&self) -> &'static str {
    match self {
      CsvBlockId::Metadata => {
        "general solution info; subcase IDs, solution types, etc."
      }
      CsvBlockId::Displacements => "displacements.",
      CsvBlockId::Stresses => "stresses.",
      CsvBlockId::Strains => "strains.",
      CsvBlockId::EngForces => "element engineering forces.",
      CsvBlockId::GridPointForces => "grid point force balance.",
      CsvBlockId::AppliedForces => "applied forces.",
      CsvBlockId::SpcForces => "forces of single-point constraint.",
      CsvBlockId::Eigenvectors => "eigenvectors.",
      CsvBlockId::Eigenvalues => "real eigenvalues.",
    }
  }

  /// Returns a constant name for this block ID.
  pub const fn name(&self) -> &'static str {
    return match self {
      Self::Metadata => "Metadata",
      Self::Displacements => "Displacements",
      Self::Stresses => "Stresses",
      Self::Strains => "Strains",
      Self::EngForces => "EngForces",
      Self::GridPointForces => "GridPointForces",
      Self::AppliedForces => "AppliedForces",
      Self::SpcForces => "SpcForces",
      Self::Eigenvectors => "Eigenvectors",
      Self::Eigenvalues => "Eigenvalues",
    };
  }

  /// Returns a human-friendly, properly spaced/capitalised name for this
  /// block ID, suitable for use in UI labels (e.g. `"SPC Forces"` rather
  /// than `"SpcForces"`).
  pub const fn display_name(&self) -> &'static str {
    return match self {
      Self::Metadata => "Metadata",
      Self::Displacements => "Displacements",
      Self::Stresses => "Stresses",
      Self::Strains => "Strains",
      Self::EngForces => "Engineering Forces",
      Self::GridPointForces => "Grid Point Forces",
      Self::AppliedForces => "Applied Forces",
      Self::SpcForces => "SPC Forces",
      Self::Eigenvectors => "Eigenvectors",
      Self::Eigenvalues => "Eigenvalues",
    };
  }

  /// Returns the shorthand for the block ID.
  pub const fn shorthand(&self) -> &'static str {
    return match self {
      Self::Metadata => "meta",
      Self::Displacements => "disp",
      Self::Stresses => "stress",
      Self::Strains => "strain",
      Self::EngForces => "engfor",
      Self::GridPointForces => "gpforce",
      Self::AppliedForces => "load",
      Self::SpcForces => "spcfor",
      Self::Eigenvectors => "eigenvec",
      Self::Eigenvalues => "eigenval",
    };
  }

  /// Returns the hidden aliases for each block ID.
  pub const fn aliases(&self) -> &'static [&'static str] {
    return match self {
      Self::Metadata => &["0", "sol", "sol_info", "info"],
      Self::Displacements => &["1", "disp", "displs", "displacements"],
      Self::Stresses => &["2", "stresses"],
      Self::Strains => &["3", "strains"],
      Self::EngForces => &["4", "engforces", "eng_forces"],
      Self::GridPointForces => &[
        "5",
        "gpfb",
        "gpfor",
        "gpforces",
        "grid_point_forces",
        "grid_point_force_balance",
      ],
      Self::AppliedForces => &["6", "applied"],
      Self::SpcForces => &["7", "spcf", "spcforces"],
      Self::Eigenvectors => &["8", "eigenvectors"],
      Self::Eigenvalues => &["9", "eigenvalues"],
    };
  }
}

impl Display for CsvBlockId {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.name());
  }
}

impl From<CsvBlockId> for usize {
  fn from(value: CsvBlockId) -> Self {
    return match value {
      CsvBlockId::Metadata => 0,
      CsvBlockId::Displacements => 1,
      CsvBlockId::Stresses => 2,
      CsvBlockId::Strains => 3,
      CsvBlockId::EngForces => 4,
      CsvBlockId::GridPointForces => 5,
      CsvBlockId::AppliedForces => 6,
      CsvBlockId::SpcForces => 7,
      CsvBlockId::Eigenvectors => 8,
      CsvBlockId::Eigenvalues => 9,
    };
  }
}

impl From<CsvBlockId> for CsvField {
  fn from(value: CsvBlockId) -> Self {
    return Self::Natural(value.into());
  }
}

impl TryFrom<usize> for CsvBlockId {
  type Error = ();

  fn try_from(value: usize) -> Result<Self, Self::Error> {
    return Ok(match value {
      0 => CsvBlockId::Metadata,
      1 => CsvBlockId::Displacements,
      2 => CsvBlockId::Stresses,
      3 => CsvBlockId::Strains,
      4 => CsvBlockId::EngForces,
      5 => CsvBlockId::GridPointForces,
      6 => CsvBlockId::AppliedForces,
      7 => CsvBlockId::SpcForces,
      8 => CsvBlockId::Eigenvectors,
      9 => CsvBlockId::Eigenvalues,
      _ => return Err(()),
    });
  }
}

/// The kinds of CSV records we can find in our format.
#[derive(
  Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd, derive_more::From,
)]
pub enum CsvField {
  /// A blank record.
  Blank,
  /// An integer.
  Integer(isize),
  /// A natural number.
  Natural(usize),
  /// A real number.
  Real(f64),
  /// An alloc'd string.
  String(String),
  /// An element type.
  ElementType(ElementType),
}

impl From<F06Number> for CsvField {
  fn from(value: F06Number) -> Self {
    return match value {
      F06Number::Real(x) => Self::Real(x),
      F06Number::Integer(i) => Self::Integer(i),
      F06Number::Natural(n) => Self::Natural(n),
    };
  }
}

impl Display for CsvField {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::Blank => write!(f, ""),
      Self::Integer(i) => i.fmt(f),
      Self::Natural(n) => n.fmt(f),
      Self::Real(x) => fmt_f64(f, *x, 0, 6, 3, true, false),
      Self::String(s) => s.fmt(f),
      Self::ElementType(et) => et.fmt(f),
    };
  }
}

/// A non-header line in a CSV file.
#[derive(Clone, Debug, Serialize)]
pub struct CsvRecord {
  /// The CSV block type.
  pub block_id: CsvBlockId,
  /// Block type that originated this record. If none, it's the 0-block.
  pub block_type: Option<BlockType>,
  /// If this record relates to a grid point, its ID.
  pub gid: Option<usize>,
  /// If this record relates to an element, the element ID.
  pub eid: Option<usize>,
  /// If this record relates to an element, its type.
  pub etype: Option<ElementType>,
  /// If this record relates to a subcase, its ID.
  pub subcase: Option<usize>,
  /// The remaining ten fields.
  pub fields: [CsvField; NAS_CSV_COLS - 1],
  /// The headers for the ten fields.
  pub headers: &'static RowHeader,
}

impl CsvRecord {
  /// Returns this as eleven strings.
  pub fn to_fields(self) -> impl Iterator<Item = CsvField> {
    return [CsvField::from(self.block_id)]
      .into_iter()
      .chain(self.fields);
  }

  /// Returns this block's headers as eleven strings.
  pub fn header_as_iter(&self) -> impl Iterator<Item = &str> {
    return [self.block_id.name()]
      .into_iter()
      .chain(self.headers.iter().copied());
  }
}
