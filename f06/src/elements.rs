//! This module defines the different kinds of elements that can be found in
//! Nastran output so that output fields can be taken generically over elements
//! and so the code is easier to expand.

use core::str::FromStr;
use std::fmt::Display;

#[cfg(feature = "clap")]
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

/// Broadly-defined element categories.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ElementCategory {
  /// Rigid-body elements, like RBE2.
  RigidBody,
  /// Scalar mass elements, like MASS1.
  ScalarMass,
  /// Scalar spring elements, like ELAS1.
  ScalarSpring,
  /// Bushing elements, like BUSH.
  Bushing,
  /// One-dimensional elastic elements, like ROD.
  OneDimensionalElastic,
  /// Two-dimensional elastic elements, like QUAD4.
  TwoDimensionalElastic,
  /// Three-dimensional elastic elements, like HEXA.
  ThreeDimensionalElastic,
}

/// Generates the ElementType enum.
macro_rules! gen_elems {
  (
    $(($vn:ident, $nm:literal, $cat:ident),)*
  ) => {
    /// Known element types.
    #[derive(
      Copy, Clone, Debug, PartialEq, Eq, SerializeDisplay,
      DeserializeFromStr,
    )]
    #[cfg_attr(feature = "clap", derive(ValueEnum))]
    #[cfg_attr(feature = "clap", clap(rename_all = "UPPER"))]
    #[allow(missing_docs)]
    #[non_exhaustive]
    pub enum ElementType {
      $($vn,)*
    }

    impl ElementType {
      /// Returns the all-caps name of the element type.
      pub const fn name(&self) -> &'static str {
        return match self {
          $(Self::$vn => $nm,)*
        };
      }

      /// Returns the category of the element type.
      pub const fn category(&self) -> ElementCategory {
        return match self {
          $(Self::$vn => ElementCategory::$cat,)*
        };
      }

      /// Returns a static slice with all known element types.
      pub const fn all() -> &'static [Self] {
        return &[
          $(Self::$vn,)*
        ];
      }
    }

    impl FromStr for ElementType {
      type Err = String;

      fn from_str(s: &str) -> Result<Self, Self::Err> {
        return match s {
          $(
            $nm => Ok(Self::$vn),
          )*
          _ => return Err(format!("invalid element type \"{}\"", s))
        };
      }
    }
  };
}

gen_elems!(
  // rigid-body
  (Rbe2, "RBE2", RigidBody),
  (Rbe3, "RBE3", RigidBody),
  (Rspline, "RSPLINE", RigidBody),
  // scalar mass
  (Mass1, "MASS1", ScalarMass),
  (Mass2, "MASS2", ScalarMass),
  (Mass3, "MASS3", ScalarMass),
  (Mass4, "MASS4", ScalarMass),
  // scalar spring
  (Elas1, "ELAS1", ScalarSpring),
  (Elas2, "ELAS2", ScalarSpring),
  (Elas3, "ELAS3", ScalarSpring),
  (Elas4, "ELAS4", ScalarSpring),
  // bushing
  (Bush, "BUSH", Bushing),
  // 1D elastic
  (Bar, "BAR", OneDimensionalElastic),
  (Rod, "ROD", OneDimensionalElastic),
  (Beam, "BEAM", OneDimensionalElastic),
  // 2D elastic
  (Quad4, "QUAD4", TwoDimensionalElastic),
  (Quad4k, "QUAD4K", TwoDimensionalElastic),
  (Quad6, "QUAD6", TwoDimensionalElastic),
  (Quad8, "QUAD8", TwoDimensionalElastic),
  (Quadr, "QUADR", TwoDimensionalElastic),
  (Tria3, "TRIA3", TwoDimensionalElastic),
  (Tria3k, "TRIA3K", TwoDimensionalElastic),
  (Tria6, "TRIA6", TwoDimensionalElastic),
  (Triar, "TRIAR", TwoDimensionalElastic),
  (Shear, "SHEAR", TwoDimensionalElastic),
  // 3D elastic
  (Tetra, "TETRA", ThreeDimensionalElastic),
  (Penta, "PENTA", ThreeDimensionalElastic),
  (Hexa, "HEXA", ThreeDimensionalElastic),
);

impl PartialOrd for ElementType {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for ElementType {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    return self.name().cmp(other.name());
  }
}

impl Display for ElementType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.name());
  }
}
