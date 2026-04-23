//! This submodule implements several indexing types used to acces values in an
//! output block.

use std::collections::BTreeMap;
use std::fmt::{Debug as DebugTrait, Display};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// Generates a NasIndex type from pure enum fields. Saves some time.
macro_rules! from_enum {
  (
    $desc:literal,
    $tname:ident,
    [
      $(
        ($varname:ident, $varstr:literal),
      )+
    ]
  ) => {
    #[derive(
      Copy, Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq,
      Eq, derive_more::From
    )]
    #[doc = $desc]
    #[allow(missing_docs)]
    pub enum $tname {
      $($varname,)+
    }

    impl $tname {
      /// Returns a short, uppercase name for this index variant.
      pub const fn name(&self) -> &'static str {
        return match self {
          $(Self::$varname => $varstr,)+
        };
      }

      /// Returns all the variants of this index, in canonical order.
      pub const fn all() -> &'static [Self] {
        return &[$(Self::$varname,)+];
      }

      /// Returns a map with this index in canonical order for ease of use when
      /// booting up a decoder.
      pub fn canonical_cols() -> BTreeMap<Self, usize> {
        return Self::all()
          .iter()
          .copied()
          .enumerate()
          .map(|(a, b)| (b, a))
          .collect();
      }
    }

    impl Display for $tname {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(f, "{}", self.name());
      }
    }
  };
}

/// Generates an index that merely contains another.
macro_rules! gen_with_inner(
  (
    $desc:literal,
    $name:literal,
    $outer_type:ident,
    $inner_type:ident
  ) => {
    #[doc = $desc]
    #[derive(
      Copy, Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
      derive_more::From
    )]
    #[allow(missing_docs)] // nah
    pub struct $outer_type(pub $inner_type);

    impl Display for $outer_type {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return Display::fmt(&self.0, f);
      }
    }

    impl IndexType for $outer_type {
      const INDEX_NAME: &'static str = $name;
    }
  }
);

/// Generates the NasIndex struct that encapsulates all indexing types.
macro_rules! gen_nasindex {
  (
    $($tn:ident,)*
  ) => {
    /// This enum encapsulates all index types, taken generally.
    #[derive(
      Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq,
      PartialOrd, Ord
    )]
    #[allow(missing_docs)] // I refuse.
    pub enum NasIndex {
      $($tn($tn),)*
    }

    $(
      impl From<$tn> for NasIndex {
        fn from(value: $tn) -> Self {
          return Self::$tn(value);
        }
      }
    )*

    impl Display for NasIndex {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return match self {
          $(Self::$tn(x) => <$tn as Display>::fmt(x, f),)*
        };
      }
    }

    impl NasIndex {
      /// Returns the name of the type of this index.
      pub fn type_name(&self) -> &'static str {
        return match self {
          $(Self::$tn(_) => <$tn as IndexType>::INDEX_NAME,)*
        };
      }
    }
  };
}

impl NasIndex {
  /// Returns the grid point associated with this index, if it has one.
  pub fn grid_point_id(&self) -> Option<GridPointRef> {
    return Some(match self {
      NasIndex::GridPointRef(g) => *g,
      NasIndex::PointInElement(pie) => match pie.point {
        ElementPoint::Corner(g) => g,
        ElementPoint::Midpoint(g) => g,
        _ => return None,
      },
      NasIndex::GridPointForceOrigin(gpfo) => gpfo.grid_point,
      NasIndex::ElementSidedPoint(esp) => match esp.point {
        ElementPoint::Corner(g) => g,
        ElementPoint::Midpoint(g) => g,
        _ => return None,
      },
      NasIndex::GridPointCsys(g) => g.gid,
      _ => return None,
    });
  }

  /// Returns the element associated with this index, if it has one.
  pub fn element_id(&self) -> Option<ElementRef> {
    return Some(match self {
      NasIndex::ElementRef(e) => *e,
      NasIndex::PointInElement(pie) => pie.element,
      NasIndex::GridPointForceOrigin(gpfo) => match gpfo.force_origin {
        ForceOrigin::Element { elem } => elem,
        _ => return None,
      },
      NasIndex::ElementSidedPoint(esp) => esp.element,
      _ => return None,
    });
  }

  /// Returns the degree of freedom associated with this index, if it has one.
  pub fn dof(&self) -> Option<Dof> {
    if let Self::Dof(d) = self {
      return Some(*d);
    }
    return None;
  }
}

gen_nasindex!(
  Dof,
  GridPointRef,
  ElementRef,
  PointInElement,
  GridPointForceOrigin,
  ElementSidedPoint,
  SingleForce,
  SingleStress,
  SingleStrain,
  BarForceField,
  BarStressField,
  BarStrainField,
  RodForceField,
  RodStressField,
  RodStrainField,
  PlateForceField,
  PlateStressField,
  PlateStrainField,
  GridPointCsys,
  RealEigenvalueField,
  EigenSolutionMode,
);

/// All field indexing types must implement this trait.
pub trait IndexType:
  Copy + Ord + Eq + Into<NasIndex> + Display + DebugTrait
{
  /// The name of this type of index, all caps.
  const INDEX_NAME: &'static str;
}

impl IndexType for Dof {
  const INDEX_NAME: &'static str = "DOF";
}

/// The possible origins for a force.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub enum ForceOrigin {
  /// The force was applied by a load.
  Load,
  /// The force was applied by another element.
  Element {
    /// A reference to the element.
    elem: ElementRef,
  },
  /// The force was applied by a single-point constraint.
  SinglePointConstraint,
  /// The force was applied by a multi-point constraint.
  MultiPointConstraint,
}

impl Display for ForceOrigin {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::Load => write!(f, "APPLIED LOAD"),
      Self::Element { elem } => write!(f, "{elem}"),
      Self::SinglePointConstraint => write!(f, "SINGLE-POINT CONSTRAINT"),
      Self::MultiPointConstraint => write!(f, "MULTI-POINT CONSTRAINT"),
    };
  }
}

/// A grid point, referenced by its ID.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
  derive_more::FromStr,
)]
pub struct GridPointRef {
  /// The ID of the grid point.
  pub gid: usize,
}

impl Display for GridPointRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "GRID {}", self.gid);
  }
}

impl IndexType for GridPointRef {
  const INDEX_NAME: &'static str = "GRID POINT ID";
}

/// An element, referenced by its ID.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub struct ElementRef {
  /// The ID of the element.
  pub eid: usize,
  /// The type of element, if known.
  pub etype: Option<ElementType>,
}

impl From<usize> for ElementRef {
  fn from(value: usize) -> Self {
    return Self {
      eid: value,
      etype: None,
    };
  }
}

impl FromStr for ElementRef {
  type Err = <usize as FromStr>::Err;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    return usize::from_str(s).map(|eid| Self { eid, etype: None });
  }
}

impl Display for ElementRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self.etype {
      Some(et) => write!(f, "ELEMENT {} ({})", self.eid, et.name()),
      None => write!(f, "ELEMENT {}", self.eid),
    };
  }
}

impl IndexType for ElementRef {
  const INDEX_NAME: &'static str = "ELEMENT ID";
}

/// A coordinate system, referenced by its ID.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub struct CsysRef {
  /// The ID of the coordinate system.
  pub cid: usize,
}

impl Display for CsysRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "COORD SYS {}", self.cid);
  }
}

/// A combination of a grid point reference and a force origin.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub struct GridPointForceOrigin {
  /// A reference to the grid point.
  pub grid_point: GridPointRef,
  /// The origin of the force.
  pub force_origin: ForceOrigin,
}

impl Display for GridPointForceOrigin {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{} FORCE AT {}", self.force_origin, self.grid_point);
  }
}

impl IndexType for GridPointForceOrigin {
  const INDEX_NAME: &'static str = "GRID POINT FORCE ORIGIN";
}

/// A point within an element.
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
)]
pub enum ElementPoint {
  /// The element's center.
  Centroid,
  /// A corner point.
  Corner(GridPointRef),
  /// A midpoint.
  Midpoint(GridPointRef),
  /// Anywhere in the element.
  Anywhere,
}

impl Display for ElementPoint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::Centroid => write!(f, "CENTROID"),
      Self::Corner(GridPointRef { gid }) => {
        write!(f, "CORNER AT GRID {gid}")
      }
      Self::Midpoint(GridPointRef { gid }) => {
        write!(f, "MIDPOINT AT GRID {gid}")
      }
      Self::Anywhere => write!(f, "ANYWHERE IN THE ELEMENT"),
    };
  }
}

/// An element side.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub enum ElementSide {
  /// The bottom (Z1) side of the element.
  Bottom,
  /// The top (Z2) side of the element.
  Top,
}

impl Display for ElementSide {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(
      f,
      "{} SIDE",
      match self {
        Self::Bottom => "BOTTOM",
        Self::Top => "TOP",
      }
    );
  }
}

impl ElementSide {
  /// Returns the opposite side.
  pub const fn opposite(&self) -> Self {
    return match self {
      Self::Bottom => Self::Top,
      Self::Top => Self::Bottom,
    };
  }
}

/// An element and a point within it.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub struct PointInElement {
  /// A reference to the element.
  pub element: ElementRef,
  /// The point within the element.
  pub point: ElementPoint,
}

impl Display for PointInElement {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}, {}", self.element, self.point);
  }
}

impl IndexType for PointInElement {
  const INDEX_NAME: &'static str = "POINT IN ELEMENT";
}

/// An element and a point within it, plus a side.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub struct ElementSidedPoint {
  /// A reference to the element.
  pub element: ElementRef,
  /// The point within the element.
  pub point: ElementPoint,
  /// The side.
  pub side: ElementSide,
}

impl Display for ElementSidedPoint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}, {}, {}", self.element, self.point, self.side);
  }
}

impl IndexType for ElementSidedPoint {
  const INDEX_NAME: &'static str = "ELEMENT, POINT AND SIDE";
}

impl ElementSidedPoint {
  /// Flips the side of this element point.
  pub fn flip_side(&mut self) {
    self.side = self.side.opposite();
  }
}

from_enum!(
  "The columns for the stresses table for plate elements.",
  PlateStressField,
  [
    (FibreDistance, "FIBRE DISTANCE"),
    (NormalX, "NORMAL-X"),
    (NormalY, "NORMAL-Y"),
    (ShearXY, "SHEAR-XY"),
    (Angle, "ANGLE"),
    (Major, "MAJOR"),
    (Minor, "MINOR"),
    (VonMises, "VON MISES"),
  ]
);

impl IndexType for PlateStressField {
  const INDEX_NAME: &'static str = "PLATE STRESS FIELD";
}

gen_with_inner!(
  "The columns for the strains table for plate elements.",
  "PLATE STRAIN FIELD",
  PlateStrainField,
  PlateStressField
);

from_enum!(
  "The columns for the engineering forces table for a quadrilateral element.",
  PlateForceField,
  [
    (NormalX, "Nx"),
    (NormalY, "Ny"),
    (NormalXY, "Nxy"),
    (MomentX, "Mx"),
    (MomentY, "My"),
    (MomentXY, "Mxy"),
    (TransverseShearX, "Qx"),
    (TransverseShearY, "Qy"),
  ]
);

impl IndexType for PlateForceField {
  const INDEX_NAME: &'static str = "2D ELEM FORCE FIELD";
}

from_enum!(
  "Engineering forces for ROD elements.",
  RodForceField,
  [(AxialForce, "AXIAL FORCE"), (Torque, "TORQUE"),]
);

impl IndexType for RodForceField {
  const INDEX_NAME: &'static str = "ROD FORCE FIELD";
}

from_enum!(
  "An end of a BAR element.",
  BarEnd,
  [(EndA, "END-A"), (EndB, "END-B"),]
);

impl BarEnd {
  /// Returns the opposite end.
  pub const fn opposite(&self) -> Self {
    return match self {
      Self::EndA => Self::EndB,
      Self::EndB => Self::EndA,
    };
  }
}

from_enum!(
  "A plane of a BAR element.",
  BarPlane,
  [(Plane1, "PLANE 1"), (Plane2, "PLANE 2"),]
);

/// A column of a BAR engineering force table.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub enum BarForceField {
  /// Bend moments.
  BendMoment {
    /// The end of the bar.
    end: BarEnd,
    /// The plane.
    plane: BarPlane,
  },
  /// Shear forces.
  Shear {
    /// The plane.
    plane: BarPlane,
  },
  /// Axial force.
  AxialForce,
  /// Torque.
  Torque,
}

impl Display for BarForceField {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      BarForceField::BendMoment { end, plane } => {
        write!(f, "BEND-MOMENT {end}, {plane}")
      }
      BarForceField::Shear { plane } => write!(f, "SHEAR {plane}"),
      BarForceField::AxialForce => write!(f, "AXIAL FORCE"),
      BarForceField::Torque => write!(f, "TORQUE"),
    };
  }
}

impl IndexType for BarForceField {
  const INDEX_NAME: &'static str = "BAR FORCE FIELD";
}

impl BarForceField {
  /// Returns the fields in the most commonly seen order.
  pub const fn all() -> &'static [Self] {
    return &[
      Self::BendMoment {
        end: BarEnd::EndA,
        plane: BarPlane::Plane1,
      },
      Self::BendMoment {
        end: BarEnd::EndA,
        plane: BarPlane::Plane2,
      },
      Self::BendMoment {
        end: BarEnd::EndB,
        plane: BarPlane::Plane1,
      },
      Self::BendMoment {
        end: BarEnd::EndB,
        plane: BarPlane::Plane2,
      },
      Self::Shear {
        plane: BarPlane::Plane1,
      },
      Self::Shear {
        plane: BarPlane::Plane2,
      },
      Self::AxialForce,
      Self::Torque,
    ];
  }

  /// Returns a col index map for ease of use in decoders.
  pub fn canonical_cols() -> BTreeMap<Self, usize> {
    return Self::all()
      .iter()
      .copied()
      .enumerate()
      .map(|(a, b)| (b, a))
      .collect();
  }
}

from_enum!(
  "Generic single-force field.",
  SingleForce,
  [(Force, "FORCE"),]
);

from_enum!(
  "Generic single-stress field.",
  SingleStress,
  [(Stress, "STRESS"),]
);

from_enum!(
  "Generic single-strain field.",
  SingleStrain,
  [(Strain, "STRAIN"),]
);

impl IndexType for SingleForce {
  const INDEX_NAME: &'static str = "FORCE";
}

impl IndexType for SingleStress {
  const INDEX_NAME: &'static str = "STRESS";
}

impl IndexType for SingleStrain {
  const INDEX_NAME: &'static str = "STRAIN";
}

impl From<SingleStress> for SingleStrain {
  fn from(_value: SingleStress) -> Self {
    return Self::Strain;
  }
}

from_enum!(
  "Rod element stress field.",
  RodStressField,
  [
    (Axial, "AXIAL"),
    (AxialSafetyMargin, "AXIAL SAFETY MARGIN"),
    (Torsional, "TORSIONAL"),
    (TorsionalSafetyMargin, "TORSIONAL SAFETY MARGIN"),
  ]
);

impl IndexType for RodStressField {
  const INDEX_NAME: &'static str = "ROD STRESS FIELD";
}

gen_with_inner!(
  "The columns for the strains table for rod elements.",
  "ROD STRAIN FIELD",
  RodStrainField,
  RodStressField
);

/// Type of normal stress.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub enum NormalStressDirection {
  /// Tension stress.
  Tension,
  /// Compression stress.
  Compression,
}

impl Display for NormalStressDirection {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(
      f,
      "{}",
      match self {
        Self::Tension => "TENSION",
        Self::Compression => "COMPRESSION",
      }
    );
  }
}

/// The columns of a bar stress/strain table are indexed by this type.
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
)]
pub enum BarStressField {
  /// Stress calculated at a specific recovery point.
  AtRecoveryPoint {
    /// The bar end where the stress was calculated.
    end: BarEnd,
    /// The recovery point where the stress was recovered. It's 1-4 for BARs.
    point: u8,
  },
  /// Axial stress.
  Axial,
  /// Maximum stress at one end.
  MaxAt(BarEnd),
  /// Minimum stress at one end.
  MinAt(BarEnd),
  /// Margin of safety.
  SafetyMargin(NormalStressDirection),
}

impl Display for BarStressField {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::AtRecoveryPoint { end, point } => {
        write!(f, "{end}, RECOVERY POINT {point}")
      }
      Self::Axial => write!(f, "AXIAL"),
      Self::MaxAt(end) => write!(f, "MAX AT {end}"),
      Self::MinAt(end) => write!(f, "MIN AT {end}"),
      Self::SafetyMargin(dir) => write!(f, "MARGIN OF SAFETY FOR {dir}"),
    };
  }
}

impl IndexType for BarStressField {
  const INDEX_NAME: &'static str = "BAR STRESS FIELD";
}

impl BarStressField {
  /// Returns all variants.
  pub const fn all() -> &'static [Self] {
    return &[
      Self::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 1,
      },
      Self::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 2,
      },
      Self::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 3,
      },
      Self::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 4,
      },
      Self::MaxAt(BarEnd::EndA),
      Self::MinAt(BarEnd::EndA),
      Self::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 1,
      },
      Self::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 2,
      },
      Self::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 3,
      },
      Self::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 4,
      },
      Self::MaxAt(BarEnd::EndB),
      Self::MinAt(BarEnd::EndB),
      Self::Axial,
      Self::SafetyMargin(NormalStressDirection::Tension),
      Self::SafetyMargin(NormalStressDirection::Compression),
    ];
  }

  /// Returns a map with all variants in the canonical order, useful for making
  /// column indexes in RowBlocks.
  pub fn canonical_cols() -> BTreeMap<Self, usize> {
    return Self::all()
      .iter()
      .copied()
      .enumerate()
      .map(|(a, b)| (b, a))
      .collect();
  }
}

gen_with_inner!(
  "The columns for the strains table for bar elements.",
  "BAR STRAIN FIELD",
  BarStrainField,
  BarStressField
);

/// A combination of a grid point reference and a coordinate system
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub struct GridPointCsys {
  /// A reference to the grid point.
  pub gid: GridPointRef,
  /// The coordinate system.
  pub cid: CsysRef,
}

impl IndexType for GridPointCsys {
  const INDEX_NAME: &'static str = "GRID POINT COORD SYS";
}

impl Display for GridPointCsys {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{} ON {}", self.gid, self.cid)
  }
}

impl From<(usize, usize)> for GridPointCsys {
  fn from((gid, cid): (usize, usize)) -> Self {
    Self {
      gid: gid.into(),
      cid: cid.into(),
    }
  }
}

/// Vibration mode of eigen solution
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialOrd,
  Ord,
  PartialEq,
  Eq,
  derive_more::From,
)]
pub struct EigenSolutionMode(pub i32);

impl IndexType for EigenSolutionMode {
  const INDEX_NAME: &'static str = "EIGEN SOLUTION MODE";
}

impl Display for EigenSolutionMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("MODE")
  }
}

from_enum!(
  "Field Values for Real Eigenvalues",
  RealEigenvalueField,
  [
    (Eigenvalue, "EIGENVALUE"),
    (Radians, "RADIANS"),
    (Cycles, "CYCLES"),
    (GeneralizedMass, "GENERALIZED MASS"),
    (GeneralizedStiffness, "GENERALIZED STIFFNESS"),
  ]
);

impl IndexType for RealEigenvalueField {
  const INDEX_NAME: &'static str = "EIGENVALUE FIELDS";
}
