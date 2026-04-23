//! This module implements differences between flavours of text output between
//! solvers and their varied configurtions/solution types.

use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::prelude::BlockType;

/// The different supported solvers.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum Solver {
  /// The MYSTRAN solver originally developed by Dr. Bill Case.
  Mystran,
  /// The Simcenter Nastran solver, formerly known as NX Nastran.
  Simcenter,
  /// The MSC Nastran solver, developed by Hexagon.
  Msc,
}

impl Display for Solver {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.name());
  }
}

impl Solver {
  /// Returns all known solvers.
  pub const fn all() -> &'static [Self] {
    return &[Self::Mystran, Self::Simcenter, Self::Msc];
  }

  /// Returns a constant display name for the solver.
  pub const fn name(&self) -> &'static str {
    return match self {
      Solver::Mystran => "MYSTRAN",
      Solver::Simcenter => "Simcenter Nastran",
      Solver::Msc => "MSC Nastran",
    };
  }

  /// Returns an array of "block ending" strings tht we should test for.
  pub const fn block_enders(&self) -> &'static [&'static str] {
    return match self {
      Solver::Mystran => &["-------------", "------------"],
      Solver::Simcenter => &["SIMCENTER NASTRAN"],
      Solver::Msc => &["MSC Nastran"],
    };
  }

  /// Returns the exceptions to block enders.
  pub const fn ender_exceptions(&self) -> &'static [BlockType] {
    return match self {
      Solver::Mystran => &[BlockType::GridPointForceBalance],
      Solver::Simcenter => &[],
      Solver::Msc => &[],
    };
  }
}

/// The known solution types.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum SolType {
  /// Linear static analysis, also known as SOL STATIC or SOL 101.
  LinearStatic,
  /// Eigenvalue/modes analyssis, also known as SOL MODES or SOL 103.
  Eigenvalue,
  /// Linear static solutions with differential stiffness.
  LinearStaticDiffStiff,
  /// Linear buckling analysis, also known as SOL BUCKLING or SOL 105.
  LinearBuckling,
  /// Nonlinear static analysis, also known as SOL NLSTATIC or SOL 106.
  NonLinearStatic,
}

impl From<SolType> for usize {
  fn from(value: SolType) -> Self {
    return match value {
      SolType::LinearStatic => 101,
      SolType::Eigenvalue => 103,
      SolType::LinearStaticDiffStiff => 104,
      SolType::LinearBuckling => 105,
      SolType::NonLinearStatic => 106,
    };
  }
}

impl TryFrom<usize> for SolType {
  type Error = ();
  fn try_from(sol: usize) -> Result<SolType, ()> {
    return Ok(match sol {
      1 | 101 => Self::LinearStatic,
      3 | 103 => Self::Eigenvalue,
      4 | 104 => Self::LinearStaticDiffStiff,
      5 | 105 => Self::LinearBuckling,
      106 => Self::NonLinearStatic,
      _ => return Err(()),
    });
  }
}

impl Display for SolType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.name());
  }
}

impl SolType {
  /// Returns a user-friendly display name for the solution.
  pub const fn name(&self) -> &'static str {
    return match self {
      SolType::LinearStatic => "Linear static",
      SolType::Eigenvalue => "Eigenvalue",
      SolType::LinearStaticDiffStiff => {
        "Linear static with differential stiffness"
      }
      SolType::LinearBuckling => "Linear buckling",
      SolType::NonLinearStatic => "Non-linear static",
    };
  }
}

/// This structure encapsulates what we currently take to be the "flavour" of
/// F06 file.
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default,
)]
pub struct Flavour {
  /// The solver that produced the file, if known.
  pub solver: Option<Solver>,
  /// The solution type that resulted in the file, if known.
  pub soltype: Option<SolType>,
}
