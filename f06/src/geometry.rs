//! This module defines basic geometric concepts to understand Nastran output.

use nalgebra::{Scalar, Vector3};
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::fmt::Display;
use std::str::FromStr;

/// Stupid constant so the code is more readable.
pub const SIXDOF: usize = 6;

/// X-translation DOF for short.
pub const DOF_TX: Dof = Dof {
  dof_type: DofType::Translational,
  axis: Axis::X,
};

/// Y-translation DOF for short.
pub const DOF_TY: Dof = Dof {
  dof_type: DofType::Translational,
  axis: Axis::Y,
};

/// Z-translation DOF for short.
pub const DOF_TZ: Dof = Dof {
  dof_type: DofType::Translational,
  axis: Axis::Z,
};

/// X-translation DOF for short.
pub const DOF_RX: Dof = Dof {
  dof_type: DofType::Rotational,
  axis: Axis::X,
};

/// Y-translation DOF for short.
pub const DOF_RY: Dof = Dof {
  dof_type: DofType::Rotational,
  axis: Axis::Y,
};

/// Z-translation DOF for short.
pub const DOF_RZ: Dof = Dof {
  dof_type: DofType::Rotational,
  axis: Axis::Z,
};

/// The two type of degree of freedom.
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
)]
pub enum DofType {
  /// Translational DOF.
  Translational,
  /// Rotational DOF.
  Rotational,
}

impl From<DofType> for char {
  fn from(value: DofType) -> Self {
    return value.letter();
  }
}

impl TryFrom<char> for DofType {
  type Error = ();

  fn try_from(value: char) -> Result<Self, Self::Error> {
    return Ok(match value {
      'T' | 't' => Self::Translational,
      'R' | 'r' => Self::Rotational,
      _ => return Err(()),
    });
  }
}

impl Display for DofType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", char::from(*self));
  }
}

impl DofType {
  /// Returns this DOF type uppercase letter.
  pub const fn letter(&self) -> char {
    return match self {
      DofType::Translational => 'T',
      DofType::Rotational => 'R',
    };
  }
}

/// The three axes.
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
)]
pub enum Axis {
  /// The X axis.
  X,
  /// The Y axis.
  Y,
  /// The Z axis.
  Z,
}

impl From<Axis> for char {
  fn from(value: Axis) -> Self {
    return value.letter();
  }
}

impl From<Axis> for usize {
  fn from(value: Axis) -> Self {
    return value.number();
  }
}

impl TryFrom<usize> for Axis {
  type Error = ();

  fn try_from(value: usize) -> Result<Self, Self::Error> {
    return Ok(match value {
      1 => Self::X,
      2 => Self::Y,
      3 => Self::Z,
      _ => return Err(()),
    });
  }
}

impl Display for Axis {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.letter());
  }
}

impl Axis {
  /// Returns this axis' lowercase letter.
  pub const fn letter(&self) -> char {
    return match self {
      Axis::X => 'x',
      Axis::Y => 'y',
      Axis::Z => 'z',
    };
  }

  /// Returns this axis' number 1-3.
  pub const fn number(&self) -> usize {
    return match self {
      Axis::X => 1,
      Axis::Y => 2,
      Axis::Z => 3,
    };
  }
}

/// The six degrees of freedom.
#[derive(
  Copy,
  Clone,
  Debug,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  SerializeDisplay,
  DeserializeFromStr,
)]
pub struct Dof {
  /// The type of DOF (translational or rotational).
  pub dof_type: DofType,
  /// The axis of the DOF (X/Y/Z).
  pub axis: Axis,
}

impl AsRef<DofType> for Dof {
  fn as_ref(&self) -> &DofType {
    return &self.dof_type;
  }
}

impl AsRef<Axis> for Dof {
  fn as_ref(&self) -> &Axis {
    return &self.axis;
  }
}

impl From<Dof> for usize {
  fn from(value: Dof) -> Self {
    let added = if value.dof_type == DofType::Rotational {
      3
    } else {
      0
    };
    return usize::from(value.axis) + added;
  }
}

impl TryFrom<usize> for Dof {
  type Error = ();

  fn try_from(value: usize) -> Result<Self, Self::Error> {
    let (dof_type, axis) = match value {
      1 => (DofType::Translational, Axis::X),
      2 => (DofType::Translational, Axis::Y),
      3 => (DofType::Translational, Axis::Z),
      4 => (DofType::Rotational, Axis::X),
      5 => (DofType::Rotational, Axis::Y),
      6 => (DofType::Rotational, Axis::Z),
      _ => return Err(()),
    };
    return Ok(Self { dof_type, axis });
  }
}

impl Display for Dof {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}{}", self.dof_type.letter(), self.axis.letter());
  }
}

impl FromStr for Dof {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    return Ok(match s.to_lowercase().as_str() {
      "tx" => DOF_TX,
      "ty" => DOF_TY,
      "tz" => DOF_TZ,
      "rx" => DOF_RX,
      "ry" => DOF_RY,
      "rz" => DOF_RZ,
      _ => return Err(format!("invalid DoF string \"{s}\"")),
    });
  }
}

impl Dof {
  /// Returns all DOF in order.
  pub const fn all() -> &'static [Self; SIXDOF] {
    return &[
      Self {
        dof_type: DofType::Translational,
        axis: Axis::X,
      },
      Self {
        dof_type: DofType::Translational,
        axis: Axis::Y,
      },
      Self {
        dof_type: DofType::Translational,
        axis: Axis::Z,
      },
      Self {
        dof_type: DofType::Rotational,
        axis: Axis::X,
      },
      Self {
        dof_type: DofType::Rotational,
        axis: Axis::Y,
      },
      Self {
        dof_type: DofType::Rotational,
        axis: Axis::Z,
      },
    ];
  }

  /// Returns a two-character name for the DOF, like Tx or Rz.
  pub const fn name(&self) -> [char; 2] {
    return [self.dof_type.letter(), self.axis.letter()];
  }
}

/// Holds some kind of data for every degree of freedom.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerDof<T: Scalar> {
  /// The data for the three translational degrees of freedom.
  t: Vector3<T>,
  /// The data for the three rotational degrees of freedom.
  r: Vector3<T>,
}

impl<T: Scalar> PerDof<T> {
  /// Accesses a value given a dof.
  fn get(&self, dof: Dof) -> &T {
    let vec = match dof.dof_type {
      DofType::Translational => &self.t,
      DofType::Rotational => &self.r,
    };
    return match dof.axis {
      Axis::X => &vec.x,
      Axis::Y => &vec.y,
      Axis::Z => &vec.z,
    };
  }
}
