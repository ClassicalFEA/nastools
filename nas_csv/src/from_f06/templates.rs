//! This submodule implements conversion templates for the supported F06 block
//! types.

use std::collections::BTreeMap;

use f06::prelude::*;

use crate::prelude::index_fns::*;
use crate::prelude::*;

/// Macro to generate a sequence of ColumnValue generators.
macro_rules! cols {
  (
    $(
      $vnt:ident,
      [
        $(
          $before:expr,
        )*
      ],
      [
        $(
          $direct:expr,
        )*
      ],
      [
        $(
          $byvar:ident,
        )*
      ],
      [
        $(
          $after:expr,
        )*
      ],
    )*
  ) => {
    [
      $(
        $($before,)*
        $(
          ColumnGenerator::ColumnValue(NasIndex::$vnt($direct)),
        )*
        $(
          ColumnGenerator::ColumnValue(NasIndex::$vnt($vnt::$byvar)),
        )*
        $($after,)*
      )*
    ]
  };
}

/// Same as cols, but for types with inners.
macro_rules! cols_inner {
  (
    $(
      $outer:ident,
      $inner:ident,
      [
        $(
          $before:expr,
        )*
      ],
      [
        $(
          $direct:expr,
        )*
      ],
      [
        $(
          $byvar:ident,
        )*
      ],
      [
        $(
          $after:expr,
        )*
      ],
    )*
  ) => {
    [
      $(
        $($before,)*
        $(
          ColumnGenerator::ColumnValue(NasIndex::$outer($outer($direct))),
        )*
        $(
          ColumnGenerator::ColumnValue(NasIndex::$outer($outer($inner::$byvar))),
        )*
        $($after,)*
      )*
    ]
  };
}

/// Generator that always produces a zero.
const ZERO: ColumnGenerator =
  ColumnGenerator::ConstantNumber(F06Number::Natural(0));

/// Generator for End A columns.
const END_A: ColumnGenerator = ColumnGenerator::ConstantString("End A");

/// Generator for End A columns.
const END_B: ColumnGenerator = ColumnGenerator::ConstantString("End B");

/// Generator that always produces a blank.
const BLANK: ColumnGenerator = ColumnGenerator::Blank;

/// Contains all the block converters in this source file.
pub const ALL_CONVERTERS: &[BlockConverter] = &[
  // displacements
  CT_DISPLACEMENTS,
  // grid point force balance
  CT_GPFORCEBALANCE,
  // element stresses
  CT_STRESSES_QUAD,
  CT_STRESSES_TRIA,
  CT_STRESSES_ROD,
  CT_STRESSES_BAR,
  CT_STRESSES_ELAS1,
  CT_STRESSES_BUSH,
  // element strains
  CT_STRAINS_QUAD,
  CT_STRAINS_TRIA,
  CT_STRAINS_ROD,
  CT_STRAINS_BAR,
  CT_STRAINS_ELAS1,
  CT_STRAINS_BUSH,
  // element engineering forces
  CT_FORCES_QUAD,
  CT_FORCES_TRIA,
  CT_FORCES_ROD,
  CT_FORCES_BAR,
  CT_FORCES_ELAS1,
  CT_FORCES_BUSH,
  // applied forces
  CT_APPLIED_FORCES,
  // spc forces
  CT_SPC_FORCES,
  // eigen solutions
  CT_EIGENVECTOR,
  CT_REAL_EIGENVALUES,
];

/// Returns all the converters in this source file, coded per-type.
pub fn all_converters() -> BTreeMap<BlockType, BlockConverter> {
  return ALL_CONVERTERS
    .iter()
    .copied()
    .map(|c| (c.input_block_type, c))
    .collect();
}

/// Conversion template for displacements blocks.
pub const CT_DISPLACEMENTS: BlockConverter = BlockConverter {
  input_block_type: BlockType::Displacements,
  output_block_id: CsvBlockId::Displacements,
  generators: &[cols!(
    Dof,
    [ColumnGenerator::GridId, ColumnGenerator::Subcase,],
    [DOF_TX, DOF_TY, DOF_TZ, DOF_RX, DOF_RY, DOF_RZ,],
    [],
    [ZERO, BLANK,],
  )],
  headers: &[[
    "GID", "Subcase", "Tx", "Ty", "Tz", "Rx", "Ry", "Rz", "Coord", HBLANK,
  ]],
};

/// Conversion template for grid point force balance blocks.
pub const CT_GPFORCEBALANCE: BlockConverter = BlockConverter {
  input_block_type: BlockType::GridPointForceBalance,
  output_block_id: CsvBlockId::GridPointForces,
  generators: &[cols!(
    Dof,
    [
      ColumnGenerator::GridId,
      ColumnGenerator::Subcase,
      ColumnGenerator::WithDefault(
        &ColumnGenerator::ElementId,
        &CsvField::Natural(0)
      ),
      ColumnGenerator::RowIndexFn(&(ixfn_fo as IndexFn)),
    ],
    [DOF_TX, DOF_TY, DOF_TZ, DOF_RX, DOF_RY, DOF_RZ,],
    [],
    [],
  )],
  headers: &[[
    "GID", "Subcase", "EID", "TYPE", "Fx", "Fy", "Fz", "Mx", "My", "Mz",
  ]],
};

/// Conversion template for quad stresses.
pub const CT_STRESSES_QUAD: BlockConverter = BlockConverter {
  input_block_type: BlockType::QuadStresses,
  output_block_id: CsvBlockId::Stresses,
  generators: &[cols!(
    PlateStressField,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      ColumnGenerator::WithDefault(
        &ColumnGenerator::GridId,
        &CsvField::Natural(0)
      ),
      ColumnGenerator::CornerIndex,
    ],
    [],
    [FibreDistance, NormalX, NormalY,],
    [BLANK,],
    PlateStressField,
    [],
    [],
    [ShearXY,],
    [BLANK,],
  )],
  headers: &[[
    "EID (QUAD4)",
    "Subcase",
    "GID",
    "CornerIdx",
    "FibreDistance",
    "NormalX",
    "NormalY",
    HBLANK,
    "ShearXY",
    HBLANK,
  ]],
};

/// Conversion template for tria stresses.
pub const CT_STRESSES_TRIA: BlockConverter = BlockConverter {
  input_block_type: BlockType::TriaStresses,
  output_block_id: CsvBlockId::Stresses,
  generators: CT_STRESSES_QUAD.generators,
  headers: &[[
    "EID (TRIA3)",
    "Subcase",
    "GID",
    "CornerIdx",
    "FibreDistance",
    "NormalX",
    "NormalY",
    HBLANK,
    "ShearXY",
    HBLANK,
  ]],
};

/// Conversion template for rod stresses.
pub const CT_STRESSES_ROD: BlockConverter = BlockConverter {
  input_block_type: BlockType::RodStresses,
  output_block_id: CsvBlockId::Stresses,
  generators: &[cols!(
    RodStressField,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [],
    [Axial,],
    [BLANK, BLANK,],
    RodStressField,
    [],
    [],
    [Torsional,],
    [BLANK, BLANK,],
  )],
  headers: &[[
    "EID (ROD)",
    "Subcase",
    HBLANK,
    HBLANK,
    "Axial",
    HBLANK,
    HBLANK,
    "Torsional",
    HBLANK,
    HBLANK,
  ]],
};

/// Header for bar stresses.
const BAR_STRESSES_HEADER: [&str; 10] = [
  "EID (BAR)",
  "Subcase",
  "GID",
  "End",
  "Axial",
  "S1",
  "S2",
  "S3",
  "S4",
  HBLANK,
];

/// Conversion template for bar stresses.
pub const CT_STRESSES_BAR: BlockConverter = BlockConverter {
  input_block_type: BlockType::BarStresses,
  output_block_id: CsvBlockId::Stresses,
  generators: &[
    cols!(
      BarStressField,
      [
        ColumnGenerator::ElementId,
        ColumnGenerator::Subcase,
        ZERO,
        END_A,
      ],
      [
        BarStressField::Axial,
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 1
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 2
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 3
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 4
        },
      ],
      [],
      [BLANK,],
    ),
    cols!(
      BarStressField,
      [
        ColumnGenerator::ElementId,
        ColumnGenerator::Subcase,
        ZERO,
        END_B,
      ],
      [
        BarStressField::Axial,
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 1
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 2
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 3
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 4
        },
      ],
      [],
      [BLANK,],
    ),
  ],
  headers: &[BAR_STRESSES_HEADER, BAR_STRESSES_HEADER],
};

/// Conversion template for ELAS1 stresses.
pub const CT_STRESSES_ELAS1: BlockConverter = BlockConverter {
  input_block_type: BlockType::Elas1Stresses,
  output_block_id: CsvBlockId::Stresses,
  generators: &[cols!(
    SingleStress,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [],
    [Stress,],
    [BLANK, BLANK, BLANK, BLANK, BLANK,],
  )],
  headers: &[[
    "EID (ELAS1)",
    "Subcase",
    HBLANK,
    HBLANK,
    "Stress",
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
  ]],
};

/// Header for bush stresses.
pub const BUSH_STRESSES_HEADER: [&str; 10] = [
  "EID (BUSH)",
  "Subcase",
  HBLANK,
  HBLANK,
  "Sx",
  "Sy",
  "Sx",
  "Mx",
  "My",
  "Mz",
];

/// Conversion template for BUSH stresses;
pub const CT_STRESSES_BUSH: BlockConverter = BlockConverter {
  input_block_type: BlockType::BushStresses,
  output_block_id: CsvBlockId::Stresses,
  generators: &[cols!(
    Dof,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [DOF_TX, DOF_TY, DOF_TZ, DOF_RX, DOF_RY, DOF_RZ,],
    [],
    [],
  )],
  headers: &[BUSH_STRESSES_HEADER],
};

/// Conversion template for quad strains.
pub const CT_STRAINS_QUAD: BlockConverter = BlockConverter {
  input_block_type: BlockType::QuadStrains,
  output_block_id: CsvBlockId::Strains,
  generators: &[cols_inner!(
    PlateStrainField,
    PlateStressField,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      ColumnGenerator::WithDefault(
        &ColumnGenerator::GridId,
        &CsvField::Natural(0)
      ),
      ColumnGenerator::CornerIndex,
    ],
    [],
    [FibreDistance, NormalX, NormalY,],
    [BLANK,],
    PlateStrainField,
    PlateStressField,
    [],
    [],
    [ShearXY,],
    [BLANK,],
  )],
  headers: CT_STRESSES_QUAD.headers,
};

/// Conversion template for tria strains.
pub const CT_STRAINS_TRIA: BlockConverter = BlockConverter {
  input_block_type: BlockType::TriaStrains,
  output_block_id: CsvBlockId::Strains,
  generators: CT_STRAINS_QUAD.generators,
  headers: CT_STRESSES_TRIA.headers,
};

/// Conversion template for rod strains.
pub const CT_STRAINS_ROD: BlockConverter = BlockConverter {
  input_block_type: BlockType::RodStrains,
  output_block_id: CsvBlockId::Strains,
  generators: &[cols_inner!(
    RodStrainField,
    RodStressField,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [],
    [Axial,],
    [BLANK, BLANK,],
    RodStrainField,
    RodStressField,
    [],
    [],
    [Torsional,],
    [BLANK, BLANK,],
  )],
  headers: CT_STRESSES_ROD.headers,
};

/// Conversion template for bar strains.
pub const CT_STRAINS_BAR: BlockConverter = BlockConverter {
  input_block_type: BlockType::BarStrains,
  output_block_id: CsvBlockId::Strains,
  generators: &[
    cols_inner!(
      BarStrainField,
      BarStressField,
      [
        ColumnGenerator::ElementId,
        ColumnGenerator::Subcase,
        ZERO,
        END_A,
      ],
      [
        BarStressField::Axial,
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 1
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 2
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 3
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndA,
          point: 4
        },
      ],
      [],
      [BLANK,],
    ),
    cols_inner!(
      BarStrainField,
      BarStressField,
      [
        ColumnGenerator::ElementId,
        ColumnGenerator::Subcase,
        ZERO,
        END_B,
      ],
      [
        BarStressField::Axial,
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 1
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 2
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 3
        },
        BarStressField::AtRecoveryPoint {
          end: BarEnd::EndB,
          point: 4
        },
      ],
      [],
      [BLANK,],
    ),
  ],
  headers: CT_STRESSES_BAR.headers,
};

/// Conversion template for ELAS1 strains.
pub const CT_STRAINS_ELAS1: BlockConverter = BlockConverter {
  input_block_type: BlockType::Elas1Strains,
  output_block_id: CsvBlockId::Strains,
  generators: &[cols!(
    SingleStrain,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [],
    [Strain,],
    [BLANK, BLANK, BLANK, BLANK, BLANK,],
  )],
  headers: &[[
    "EID (ELAS1)",
    "Subcase",
    HBLANK,
    HBLANK,
    "Strain",
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
  ]],
};

/// Conversion template for BUSH strains.
pub const CT_STRAINS_BUSH: BlockConverter = BlockConverter {
  input_block_type: BlockType::BushStrains,
  output_block_id: CsvBlockId::Strains,
  generators: CT_STRESSES_BUSH.generators,
  headers: CT_STRESSES_BUSH.headers,
};

/// Conversion template for quad forces.
pub const CT_FORCES_QUAD: BlockConverter = BlockConverter {
  input_block_type: BlockType::QuadForces,
  output_block_id: CsvBlockId::EngForces,
  generators: &[cols!(
    PlateForceField,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [],
    [NormalX, NormalY, NormalXY, MomentX, MomentY, MomentXY,],
    [],
  )],
  headers: &[[
    "EID (QUAD4)",
    "Subcase",
    HBLANK,
    HBLANK,
    "NormalX",
    "NormalY",
    "NormalXY",
    "MomentX",
    "MomentY",
    "MomentXY",
  ]],
};

/// Conversion template for tria forces.
pub const CT_FORCES_TRIA: BlockConverter = BlockConverter {
  input_block_type: BlockType::TriaForces,
  output_block_id: CsvBlockId::EngForces,
  generators: CT_FORCES_QUAD.generators,
  headers: CT_FORCES_QUAD.headers,
};

/// Conversion template for rod forces.
pub const CT_FORCES_ROD: BlockConverter = BlockConverter {
  input_block_type: BlockType::RodForces,
  output_block_id: CsvBlockId::EngForces,
  generators: &[cols!(
    RodForceField,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [],
    [AxialForce,],
    [BLANK, BLANK, BLANK, BLANK,],
    RodForceField,
    [],
    [],
    [Torque,],
    [],
  )],
  headers: &[[
    "EID (ROD)",
    "Subcase",
    HBLANK,
    HBLANK,
    "Axial",
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
    "Torque",
  ]],
};

/// Header for bar forces. It appears twice.
const BAR_FORCES_HEADER: [&str; 10] = [
  "EID (BAR)",
  "Subcase",
  "GID",
  "End",
  "Axial",
  "S1",
  "S2",
  "M1",
  "M2",
  "Torque",
];

/// Conversion template for bar forces.
pub const CT_FORCES_BAR: BlockConverter = BlockConverter {
  input_block_type: BlockType::BarForces,
  output_block_id: CsvBlockId::EngForces,
  generators: &[
    cols!(
      BarForceField,
      [
        ColumnGenerator::ElementId,
        ColumnGenerator::Subcase,
        ZERO,
        END_A,
      ],
      [
        BarForceField::AxialForce,
        BarForceField::Shear {
          plane: BarPlane::Plane1
        },
        BarForceField::Shear {
          plane: BarPlane::Plane2
        },
        BarForceField::BendMoment {
          end: BarEnd::EndA,
          plane: BarPlane::Plane1
        },
        BarForceField::BendMoment {
          end: BarEnd::EndA,
          plane: BarPlane::Plane2
        },
      ],
      [Torque,],
      [],
    ),
    cols!(
      BarForceField,
      [
        ColumnGenerator::ElementId,
        ColumnGenerator::Subcase,
        ZERO,
        END_B,
      ],
      [
        BarForceField::AxialForce,
        BarForceField::Shear {
          plane: BarPlane::Plane1
        },
        BarForceField::Shear {
          plane: BarPlane::Plane2
        },
        BarForceField::BendMoment {
          end: BarEnd::EndB,
          plane: BarPlane::Plane1
        },
        BarForceField::BendMoment {
          end: BarEnd::EndB,
          plane: BarPlane::Plane2
        },
      ],
      [Torque,],
      [],
    ),
  ],
  headers: &[BAR_FORCES_HEADER, BAR_FORCES_HEADER],
};

/// Conversion template for ELAS1 forces.
pub const CT_FORCES_ELAS1: BlockConverter = BlockConverter {
  input_block_type: BlockType::Elas1Forces,
  output_block_id: CsvBlockId::EngForces,
  generators: &[cols!(
    SingleForce,
    [
      ColumnGenerator::ElementId,
      ColumnGenerator::Subcase,
      BLANK,
      BLANK,
    ],
    [],
    [Force,],
    [BLANK, BLANK, BLANK, BLANK, BLANK,],
  )],
  headers: &[[
    "EID (ELAS1)",
    "Subcase",
    HBLANK,
    HBLANK,
    "Force",
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
  ]],
};

/// Conversion template for BUSH forces.
pub const CT_FORCES_BUSH: BlockConverter = BlockConverter {
  input_block_type: BlockType::BushForces,
  output_block_id: CsvBlockId::EngForces,
  generators: CT_STRESSES_BUSH.generators,
  headers: &[[
    "EID (BUSH)",
    "Subcase",
    HBLANK,
    HBLANK,
    "Fx",
    "Fy",
    "Fz",
    "Mx",
    "My",
    "Mz",
  ]],
};

/// Conversion template for the load vector.
pub const CT_APPLIED_FORCES: BlockConverter = BlockConverter {
  input_block_type: BlockType::AppliedForces,
  output_block_id: CsvBlockId::AppliedForces,
  generators: &[cols!(
    Dof,
    [ColumnGenerator::GridId, ColumnGenerator::Subcase,],
    [DOF_TX, DOF_TY, DOF_TZ, DOF_RX, DOF_RY, DOF_RZ,],
    [],
    [BLANK, BLANK,],
  )],
  headers: &[[
    "GID", "Subcase", "Fx", "Fy", "Fz", "Mx", "My", "Mz", HBLANK, HBLANK,
  ]],
};

/// Conversion template for SPC forces.
pub const CT_SPC_FORCES: BlockConverter = BlockConverter {
  input_block_type: BlockType::SpcForces,
  output_block_id: CsvBlockId::SpcForces,
  generators: &[cols!(
    Dof,
    [ColumnGenerator::GridId, ColumnGenerator::Subcase,],
    [DOF_TX, DOF_TY, DOF_TZ, DOF_RX, DOF_RY, DOF_RZ,],
    [],
    [BLANK, BLANK,],
  )],
  headers: &[[
    "GID", "Subcase", "Fx", "Fy", "Fz", "Mx", "My", "Mz", HBLANK, HBLANK,
  ]],
};

/// Conversion template for EIGENVECTOR
pub const CT_EIGENVECTOR: BlockConverter = BlockConverter {
  input_block_type: BlockType::Eigenvector,
  output_block_id: CsvBlockId::Eigenvectors,
  generators: &[cols!(
    Dof,
    [ColumnGenerator::GridId, ColumnGenerator::Subcase,],
    [DOF_TX, DOF_TY, DOF_TZ, DOF_RX, DOF_RY, DOF_RZ,],
    [],
    [BLANK, BLANK,],
  )],
  headers: &[[
    "GID", "Mode", "Fx", "Fy", "Fz", "Mx", "My", "Mz", HBLANK, HBLANK,
  ]],
};

/// Conversion template for REAL EIGENVALUES
pub const CT_REAL_EIGENVALUES: BlockConverter = BlockConverter {
  input_block_type: BlockType::RealEigenvalues,
  output_block_id: CsvBlockId::Eigenvalues,
  generators: &[cols!(
    RealEigenvalueField,
    [ColumnGenerator::RowIndexFn(&(ixfn_eigen_mode as IndexFn)),],
    [
      RealEigenvalueField::Eigenvalue,
      RealEigenvalueField::Radians,
      RealEigenvalueField::Cycles,
      RealEigenvalueField::GeneralizedMass,
      RealEigenvalueField::GeneralizedStiffness,
    ],
    [],
    [BLANK, BLANK, BLANK, BLANK,],
  )],
  headers: &[[
    "Mode",
    "Eigenvalue",
    "Radians",
    "Cycles",
    "GeneralizedMass",
    "GeneralizedStiffness",
    HBLANK,
    HBLANK,
    HBLANK,
    HBLANK,
  ]],
};
