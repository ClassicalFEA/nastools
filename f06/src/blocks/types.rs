//! This module implements a list of known data blocks and information related
//! to them, such as names for detection and decoder instantiation subroutines.

use std::fmt::Display;
use std::str::FromStr;

use convert_case::{Case, Casing};
use serde_with::{DeserializeFromStr, SerializeDisplay};

use crate::blocks::decoders::*;
use crate::prelude::*;

/// Generates the BlockType enum and calls the init functions for them.
macro_rules! gen_block_types {
  (
    $(
      {
        $desc:literal,
        $bname:ident,
        $dec:ty,
        $etype:expr,
        $spaceds:expr
      },
    )*
  ) => {
    /// This contains all the known data blocks.
    #[derive(
      Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerializeDisplay,
      DeserializeFromStr
    )]
    #[non_exhaustive]
    pub enum BlockType {
      $(
        #[doc = $desc]
        $bname,
      )*
    }

    impl BlockType {
      /// Returns all known block types.
      pub const fn all() -> &'static [Self] {
        return &[ $(Self::$bname,)* ];
      }

      /// Instantiates the decoder for this data block type.
      pub fn init_decoder(&self, flavour: Flavour) -> Box<dyn OpaqueDecoder> {
        return match self {
          $(
            Self::$bname => Box::from(<$dec as BlockDecoder>::new(flavour)),
          )*
        };
      }

      /// Returns the name of the block.
      pub const fn desc(&self) -> &'static str {
        return match self {
          $(Self::$bname => $desc,)*
        };
      }

      /// Returns the known upper-case, "spaced" forms that signal the
      /// beginning of this block.
      pub fn headers(&self) -> &'static [&'static str] {
        return match self {
          $(Self::$bname => &$spaceds,)*
        };
      }

      /// Returns the small name of the variant, CamelCase.
      pub const fn short_name(&self) -> &'static str {
        return match self {
          $(Self::$bname => stringify!($bname),)*
        };
      }

      /// Returns the small, snake case name of the variant.
      pub fn snake_case_name(&self) -> String {
        return match self {
          $(Self::$bname => self.short_name().to_case(Case::Snake),)*
        };
      }

      /// If this block type relates to an element type, its type.
      pub const fn elem_type(&self) -> Option<ElementType> {
        return match self {
          $(Self::$bname => $etype,)*
        };
      }

    }
  }
}

gen_block_types!(
  // displacements
  {
    "Grid point displacements",
    Displacements,
    DisplacementsDecoder,
    None,
    ["DISPLACEMENTS", "DISPLACEMENT VECTOR"]
  },
  // grid point force balance
  {
    "Grid point force balance",
    GridPointForceBalance,
    GridPointForceBalanceDecoder,
    None,
    ["GRID POINT FORCE BALANCE"]
  },
  // spc forces
  {
    "Forces of single-point constraint",
    SpcForces,
    SpcForcesDecoder,
    None,
    ["SPC FORCES", "FORCES OF SINGLE-POINT CONSTRAINT"]
  },
  // applied forces
  {
    "Applied forces",
    AppliedForces,
    AppliedForcesDecoder,
    None,
    ["APPLIED FORCES", "LOAD VECTOR"]
  },
  // elas1 forces
  {
    "Engineering forces in ELAS1 elements",
    Elas1Forces,
    Elas1ForcesDecoder,
    Some(ElementType::Elas1),
    [
      "FORCES IN SCALAR SPRINGS (CELAS1)",
      "ELEMENT ENGINEERING FORCES FOR ELEMENT TYPE ELAS1"
    ]
  },
  // elas1 stresses
  {
    "Stresses in ELAS1 elements",
    Elas1Stresses,
    Elas1StressesDecoder,
    Some(ElementType::Elas1),
    [
      "STRESSES IN SCALAR SPRINGS (CELAS1)",
      concat!(
        "ELEMENT STRESSES IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE ELAS1"
      )
    ]
  },
  // elas1 strains
  {
    "Strains in ELAS1 elements",
    Elas1Strains,
    Elas1StrainsDecoder,
    Some(ElementType::Elas1),
    [
      "STRAINS IN SCALAR SPRINGS (CELAS1)",
      concat!(
        "ELEMENT STRAINS IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE ELAS1"
      )
    ]
  },
  // rod forces
  {
    "Engineering forces in rod elements",
    RodForces,
    RodForcesDecoder,
    Some(ElementType::Rod),
    [
      "FORCES IN ROD ELEMENTS (CROD)",
      "ELEMENT ENGINEERING FORCES FOR ELEMENT TYPE ROD"
    ]
  },
  // rod stresses
  {
    "Stresses in rod elements",
    RodStresses,
    RodStressesDecoder,
    Some(ElementType::Rod),
    [
      "STRESSES IN ROD ELEMENTS (CROD)",
      concat!(
        "ELEMENT STRESSES IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE ROD"
      )
    ]
  },
  // rod strains
  {
    "Strains in rod elements",
    RodStrains,
    RodStrainsDecoder,
    Some(ElementType::Rod),
    [
      "STRAINS IN ROD ELEMENTS (CROD)",
      concat!(
        "ELEMENT STRAINS IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE ROD"
      )
    ]
  },
  // bar forces
  {
    "Engineering forces in bar elements",
    BarForces,
    BarForcesDecoder,
    Some(ElementType::Bar),
    [
      "FORCES IN BAR ELEMENTS (CBAR)",
      "ELEMENT ENGINEERING FORCES FOR ELEMENT TYPE BAR"
    ]
  },
  // bar stresses
  {
    "Stresses in bar elements",
    BarStresses,
    BarStressesDecoder,
    Some(ElementType::Bar),
    [
      "STRESSES IN BAR ELEMENTS (CBAR)",
      concat!(
        "ELEMENT STRESSES IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE BAR"
      )
    ]
  },
  // bar strains
  {
    "Strains in bar elements",
    BarStrains,
    BarStrainsDecoder,
    Some(ElementType::Bar),
    [
      "STRAINS IN BAR ELEMENTS (CBAR)",
      concat!(
        "ELEMENT STRAINS IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE BAR"
      )
    ]
  },
  // tria forces
  {
    "Engineering forces in triangular elements",
    TriaForces,
    TriaForcesDecoder,
    Some(ElementType::Tria3),
    [
      "FORCES IN TRIANGULAR ELEMENTS (CTRIA3)",
      "ELEMENT ENGINEERING FORCES FOR ELEMENT TYPE TRIA3",
      "FORCES IN TRIANGULAR ELEMENTS (TRIA3)"
    ]
  },
  // tria stresses
  {
    "Stresses in triangular elements",
    TriaStresses,
    TriaStressesDecoder,
    Some(ElementType::Tria3),
    [
      "STRESSES IN TRIANGULAR ELEMENTS (CTRIA3)",
      concat!(
        "ELEMENT STRESSES IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE TRIA3",
      ),
      "STRESSES IN TRIANGULAR ELEMENTS (TRIA3)"
    ]
  },
  // tria strains
  {
    "Strains in triangular elements",
    TriaStrains,
    TriaStrainsDecoder,
    Some(ElementType::Tria3),
    [
      "STRAINS IN TRIANGULAR ELEMENTS (CTRIA3)",
      concat!(
        "ELEMENT STRAINS IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE TRIA3"
      )
    ]
  },
  // quad forces
  {
    "Engineering forces in quadrilateral elements",
    QuadForces,
    QuadForcesDecoder,
    Some(ElementType::Quad4),
    [
      "FORCES IN QUADRILATERAL ELEMENTS (QUAD4)",
      "ELEMENT ENGINEERING FORCES FOR ELEMENT TYPE QUAD4"
    ]
  },
  // quad stresses
  {
    "Stresses in quadrilateral elements",
    QuadStresses,
    QuadStressesDecoder,
    Some(ElementType::Quad4),
    [
      "STRESSES IN QUADRILATERAL ELEMENTS (QUAD4)",
      concat!(
        "ELEMENT STRESSES IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE QUAD4"
      )
    ]
  },
  // quad strains
  {
    "Strains in quadrilateral elements",
    QuadStrains,
    QuadStrainsDecoder,
    Some(ElementType::Quad4),
    [
      "STRAINS IN QUADRILATERAL ELEMENTS (QUAD4)",
      concat!(
        "ELEMENT STRAINS IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE QUAD4"
      )
    ]
  },
  // bush forces
  {
    "Engineering forces in BUSH elements",
    BushForces,
    BushForcesDecoder,
    Some(ElementType::Bush),
    [
      "FORCES IN BUSH ELEMENTS (CBUSH)",
      "ELEMENT ENGINEERING FORCES FOR ELEMENT TYPE BUSH"
    ]
  },
  // bush stresses
  {
    "Stresses in BUSH elements",
    BushStresses,
    BushStressesDecoder,
    Some(ElementType::Bush),
    [
      "STRESSES IN BUSH ELEMENTS (CBUSH)",
      concat!(
        "ELEMENT STRESSES IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE BUSH"
      )
    ]
  },
  // bush strains
  {
    "Strains in BUSH elements",
    BushStrains,
    BushStrainsDecoder,
    Some(ElementType::Bush),
    [
      "STRAINS IN BUSH ELEMENTS (CBUSH)",
      concat!(
        "ELEMENT STRAINS IN LOCAL ELEMENT COORDINATE SYSTEM ",
        "FOR ELEMENT TYPE BUSH"
      )
    ]
  },
  // eigenvectors
  {
    "Eigenvector",
    Eigenvector,
    EigenvectorDecoder,
    None,
    [
      "EIGENVECTOR",
    ]
  },
  // real eigenvalues
  {
    "Eigenvalues",
    RealEigenvalues,
    RealEigenvaluesDecoder,
    None,
    [
      "REAL EIGENVALUES",
    ]
  },
);

impl Display for BlockType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}", self.desc());
  }
}

impl FromStr for BlockType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    for cand in Self::all() {
      let snk = cand.snake_case_name();
      let l = [cand.desc(), cand.short_name(), snk.as_str()];
      if l.iter().any(|p| p.eq_ignore_ascii_case(s)) {
        return Ok(*cand);
      }
    }
    return Err(format!("invalid block type name \"{s}\""));
  }
}
