//! This module implements the specific decoders for known data block types.

use std::collections::BTreeMap;

use log::*;

use crate::prelude::*;
use crate::util::*;

/// Returns column indexes for DOFs. Used by a lot of things.
fn dof_cols() -> BTreeMap<Dof, usize> {
  return Dof::all()
    .iter()
    .copied()
    .enumerate()
    .map(|(a, b)| (b, a))
    .collect();
}

/// Creates a decoder that performs pure conversions from an inner decoder.
macro_rules! converting_decoder {
  (
    // description of the outer decoder
    $outer_desc:literal,
    // name of the outer decoder
    $outer_type:ident,
    // inner decoder
    $inner_type:ty,
    // scalar type of both
    $scalar_type:ty,
    // row index types of the outer and inner
    ($row_type:ident, $inner_row_type:ident),
    // column index type of the outer and inner
    ($col_type:ident, $inner_col_type:ident),
    // block type of the outer
    $block_type:expr,
    // matwidth of both
    $matwidth:literal
  ) => {
    #[doc = $outer_desc]
    pub(crate) struct $outer_type {
      /// The inner decoder.
      inner: $inner_type,
    }

    impl BlockDecoder for $outer_type {
      type MatScalar = $scalar_type;
      type RowIndex = $row_type;
      type ColumnIndex = $col_type;
      const MATWIDTH: usize = $matwidth;
      const BLOCK_TYPE: BlockType = $block_type;

      fn new(flavour: Flavour) -> Self {
        return Self {
          inner: <$inner_type>::new(flavour),
        };
      }

      fn good_header(&mut self, header: &str) -> bool {
        return BlockDecoder::good_header(&mut self.inner, header);
      }

      fn hint_last(&mut self, last: NasIndex) {
        BlockDecoder::hint_last(&mut self.inner, last);
      }

      fn last_row_index(&self) -> Option<NasIndex> {
        return BlockDecoder::last_row_index(&self.inner);
      }

      fn unwrap(
        self,
        subcase: usize,
        line_range: Option<(usize, usize)>,
      ) -> FinalBlock {
        let mut fb = self.inner.unwrap(subcase, line_range);
        fb.col_indices = fb
          .col_indices
          .into_iter()
          .map(|(ci, n)| {
            if let NasIndex::$inner_col_type(col) = ci {
              return ($col_type::from(col).into(), n);
            } else {
              debug!("got: {}, expected: {}", ci, stringify!($col_type));
              panic!("bad col index conversion in wrapped");
            }
          })
          .collect();
        fb.row_indices = fb
          .row_indices
          .into_iter()
          .map(|(ci, n)| {
            if let NasIndex::$inner_row_type(row) = ci {
              return ($row_type::from(row).into(), n);
            } else {
              debug!("got: {}, expected: {}", ci, stringify!($col_type));
              panic!("bad row index conversion in wrapped");
            }
          })
          .collect();
        fb.block_type = Self::BLOCK_TYPE;
        return fb;
      }

      fn consume(&mut self, line: &str) -> LineResponse {
        return BlockDecoder::consume(&mut self.inner, line);
      }
    }
  };
}

/// This decodes a displacements block.
pub(crate) struct DisplacementsDecoder {
  /// The flavour of F06 file we're decoding displacements for.
  flavour: Flavour,
  /// The displacement data.
  data: RowBlock<f64, GridPointRef, Dof, { Self::MATWIDTH }>,
}

impl BlockDecoder for DisplacementsDecoder {
  type MatScalar = f64;
  type RowIndex = GridPointRef;
  type ColumnIndex = Dof;
  const MATWIDTH: usize = SIXDOF;
  const BLOCK_TYPE: BlockType = BlockType::Displacements;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(dof_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let dofs: [f64; SIXDOF] = if let Some(arr) = extract_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    if let Some(gid) = nth_integer(line, 0) {
      self.data.insert_raw((gid as usize).into(), &dofs);
      return LineResponse::Data;
    }
    return LineResponse::Useless;
  }
}

/// The decoder for grid point force balance blocks.
pub(crate) struct GridPointForceBalanceDecoder {
  /// The flavour of F06 file we're decoding displacements for.
  flavour: Flavour,
  /// The current grid point ID.
  gpref: Option<GridPointRef>,
  /// The force balance data.
  data: RowBlock<f64, GridPointForceOrigin, Dof, { Self::MATWIDTH }>,
}

impl BlockDecoder for GridPointForceBalanceDecoder {
  type MatScalar = f64;
  type RowIndex = GridPointForceOrigin;
  type ColumnIndex = Dof;
  const MATWIDTH: usize = SIXDOF;
  const BLOCK_TYPE: BlockType = BlockType::GridPointForceBalance;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      gpref: None,
      data: RowBlock::new(dof_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    if line.contains("FORCE BALANCE FOR GRID POINT") {
      self.gpref = nth_integer(line, 0).map(|x| (x as usize).into());
      return LineResponse::Metadata;
    }
    if line.contains("*TOTALS*") {
      return LineResponse::Useless;
    }
    let fo: ForceOrigin = match self.flavour.solver {
      Some(Solver::Mystran) => {
        if line.contains("APPLIED FORCE") {
          ForceOrigin::Load
        } else if line.contains("SPC FORCE") {
          ForceOrigin::SinglePointConstraint
        } else if line.contains("MPC FORCE") {
          ForceOrigin::MultiPointConstraint
        } else if line.contains("ELEM") {
          if let Some(eid) = nth_integer(line, 0) {
            ForceOrigin::Element {
              elem: ElementRef {
                eid: eid as usize,
                etype: nth_etype(line, 0),
              },
            }
          } else {
            return LineResponse::Useless;
          }
        } else {
          return LineResponse::Useless;
        }
      }
      Some(Solver::Simcenter | Solver::Msc) => {
        let i0 = nth_integer(line, 0).map(|x| x as usize);
        let i1 = nth_integer(line, 1).map(|x| x as usize);
        self.gpref = match (i0, i1) {
          (Some(0), Some(x)) => Some(x),
          (Some(x), _) => Some(x),
          _ => None,
        }
        .map(|x| x.into());
        if line.contains("*TOTALS*") {
          return LineResponse::Useless;
        } else if line.contains("APP-LOAD") {
          ForceOrigin::Load
        } else if line.contains("F-OF-SPC") {
          ForceOrigin::SinglePointConstraint
        } else if line.contains("F-OF-MPC") {
          ForceOrigin::MultiPointConstraint
        } else {
          let eid = line_breakdown(line)
            .filter_map(|lf| {
              if let LineField::Integer(eid) = lf {
                return Some(eid as usize);
              } else {
                return None;
              }
            })
            .last();
          let etype_opt = nth_etype(line, 0);
          match (self.gpref, eid, etype_opt) {
            (Some(_), Some(eid), Some(etype)) => ForceOrigin::Element {
              elem: ElementRef {
                eid,
                etype: Some(etype),
              },
            },
            _ => return LineResponse::Useless,
          }
        }
      }
      None => return LineResponse::BadFlavour,
    };
    if let Some(gpref) = self.gpref {
      let ri = GridPointForceOrigin {
        grid_point: gpref,
        force_origin: fo,
      };
      if let Some(arr) = extract_reals::<SIXDOF>(line) {
        self.data.insert_raw(ri, &arr);
        return LineResponse::Data;
      } else {
        return LineResponse::BadFlavour;
      }
    }
    return LineResponse::Useless;
  }
}

/// Decoder for the SPC forces block type.
pub(crate) struct SpcForcesDecoder {
  /// The flavour of F06 file we're decoding SPC forces for.
  flavour: Flavour,
  /// The displacement data.
  data: RowBlock<f64, GridPointRef, Dof, { Self::MATWIDTH }>,
}

impl BlockDecoder for SpcForcesDecoder {
  type MatScalar = f64;
  type RowIndex = GridPointRef;
  type ColumnIndex = Dof;
  const MATWIDTH: usize = SIXDOF;
  const BLOCK_TYPE: BlockType = BlockType::SpcForces;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(dof_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let dofs: [f64; SIXDOF] = if let Some(arr) = extract_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    if let Some(gid) = nth_integer(line, 0) {
      self.data.insert_raw((gid as usize).into(), &dofs);
      return LineResponse::Data;
    }
    return LineResponse::Useless;
  }
}

/// This decodes an applied forces (load vector) block.
pub(crate) struct AppliedForcesDecoder {
  /// The flavour of F06 file we're decoding displacements for.
  flavour: Flavour,
  /// The displacement data.
  data: RowBlock<f64, GridPointRef, Dof, { Self::MATWIDTH }>,
}

impl BlockDecoder for AppliedForcesDecoder {
  type MatScalar = f64;
  type RowIndex = GridPointRef;
  type ColumnIndex = Dof;
  const MATWIDTH: usize = SIXDOF;
  const BLOCK_TYPE: BlockType = BlockType::AppliedForces;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(dof_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let dofs: [f64; Self::MATWIDTH] = if let Some(arr) = extract_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    if let Some(gid) = nth_integer(line, 0) {
      self.data.insert_raw((gid as usize).into(), &dofs);
      return LineResponse::Data;
    }
    return LineResponse::Useless;
  }
}

/// A decoder for the "stresses in quad elements" table.
pub(crate) struct QuadStressesDecoder {
  /// The flavour of solver we're decoding for.
  flavour: Flavour,
  /// The inner block of data.
  data: RowBlock<f64, ElementSidedPoint, PlateStressField, { Self::MATWIDTH }>,
  /// Current row reference.
  cur_row: Option<<Self as BlockDecoder>::RowIndex>,
  /// Element type, hinted by the header.
  etype: Option<ElementType>,
  /// 1-based corner position counter, reset for each new element.
  corner_counter: u8,
}

impl BlockDecoder for QuadStressesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementSidedPoint;
  type ColumnIndex = PlateStressField;
  const MATWIDTH: usize = 8;
  const BLOCK_TYPE: BlockType = BlockType::QuadStresses;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(PlateStressField::canonical_cols()),
      cur_row: None,
      etype: None,
      corner_counter: 0,
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn good_header(&mut self, header: &str) -> bool {
    self.etype = nth_etype(header, 0);
    if header.contains("THERMAL") || header.contains("ELASTIC") {
      return false;
    }
    return true;
  }

  fn hint_last(&mut self, last: NasIndex) {
    if let NasIndex::ElementSidedPoint(esp) = last {
      self.corner_counter = esp.corner_index.unwrap_or(0);
      self.cur_row = Some(esp);
    }
  }

  fn last_row_index(&self) -> Option<NasIndex> {
    return self.cur_row.map(|q| q.into());
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    // first, take eight floats. if there aren't any, we're toast.
    let cols: [f64; Self::MATWIDTH] = if let Some(arr) = lax_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    // okay, now we get the sided point.
    let fields = line_breakdown(line).collect::<Vec<_>>();
    let ints = fields
      .iter()
      .filter_map(|lf| {
        if let LineField::Integer(i) = lf {
          Some(i)
        } else {
          None
        }
      })
      .copied()
      .collect::<Vec<_>>();
    match self.flavour.solver {
      Some(Solver::Mystran) => {
        if ints.is_empty() {
          // cont. line
          if let Some(ref mut ri) = self.cur_row {
            ri.flip_side();
          } else {
            warn!("cont line without row index at {line}");
            return LineResponse::Abort;
          }
        } else {
          // line has row info
          let point = if line.contains("CENTER") {
            ElementPoint::Centroid
          } else if let Some(gid) = ints.last() {
            ElementPoint::Corner((*gid as usize).into())
          } else {
            warn!("no point at {line}");
            return LineResponse::Abort;
          };
          let side = ElementSide::Bottom;
          let eid = if let Some(LineField::Integer(eid)) = fields.first() {
            *eid as usize
          } else if let Some(ri) = self.cur_row {
            ri.element.eid
          } else {
            warn!("no eid at {line}");
            return LineResponse::Abort;
          };
          let corner_index = match point {
            ElementPoint::Corner(_) => {
              let is_new_element =
                self.cur_row.is_none_or(|r| r.element.eid != eid);
              if is_new_element {
                self.corner_counter = 1;
              } else {
                self.corner_counter += 1;
              }
              Some(self.corner_counter)
            }
            _ => {
              self.corner_counter = 0;
              None
            }
          };
          self.cur_row.replace(ElementSidedPoint {
            element: ElementRef {
              eid,
              etype: self.etype,
            },
            point,
            side,
            corner_index,
          });
        }
      }
      Some(Solver::Simcenter | Solver::Msc) => {
        if ints.is_empty() {
          // cont. line
          if let Some(ref mut ri) = self.cur_row {
            ri.flip_side();
          } else {
            warn!("cont line without row index at {line}");
            return LineResponse::Abort;
          }
        } else {
          // line has row info
          let point = if line.contains("CEN/4") {
            ElementPoint::Centroid
          } else if let Some(gid) = ints.last() {
            ElementPoint::Corner((*gid as usize).into())
          } else {
            warn!("no point at {line}");
            return LineResponse::Abort;
          };
          let side = ElementSide::Bottom;
          let eid = if let Some(x) = ints.get(1) {
            *x as usize
          } else if let Some(ri) = self.cur_row {
            ri.element.eid
          } else {
            warn!("no eid at {line}");
            return LineResponse::Abort;
          };
          let corner_index = match point {
            ElementPoint::Corner(_) => {
              let is_new_element =
                self.cur_row.is_none_or(|r| r.element.eid != eid);
              if is_new_element {
                self.corner_counter = 1;
              } else {
                self.corner_counter += 1;
              }
              Some(self.corner_counter)
            }
            _ => {
              self.corner_counter = 0;
              None
            }
          };
          self.cur_row.replace(ElementSidedPoint {
            element: ElementRef {
              eid,
              etype: self.etype,
            },
            point,
            side,
            corner_index,
          });
        }
      }
      None => return LineResponse::BadFlavour,
    }
    if let Some(rid) = self.cur_row {
      self.data.insert_raw(rid, &cols);
      return LineResponse::Data;
    } else {
      warn!("found data but couldn't construct row index at {line}");
      return LineResponse::Abort;
    }
  }
}

converting_decoder!(
  "Block decoder for strains in quadrilateral elements.",
  QuadStrainsDecoder,
  QuadStressesDecoder,
  f64,
  (ElementSidedPoint, ElementSidedPoint),
  (PlateStrainField, PlateStressField),
  BlockType::QuadStrains,
  8
);

/// Decoder for quad element engineering forces.
pub(crate) struct QuadForcesDecoder {
  /// The flavour of solver we're decoding for.
  flavour: Flavour,
  /// The inner block of data.
  data: RowBlock<f64, PointInElement, PlateForceField, { Self::MATWIDTH }>,
  /// Current row reference.
  cur_row: Option<<Self as BlockDecoder>::RowIndex>,
  /// Element type, hinted by the header.
  etype: Option<ElementType>,
  /// Does this block hold grid-IDs?
  has_grid_id: bool,
}

impl BlockDecoder for QuadForcesDecoder {
  type MatScalar = f64;
  type RowIndex = PointInElement;
  type ColumnIndex = PlateForceField;
  const MATWIDTH: usize = 8;
  const BLOCK_TYPE: BlockType = BlockType::QuadForces;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(PlateForceField::canonical_cols()),
      cur_row: None,
      etype: None,
      has_grid_id: false,
    };
  }

  fn good_header(&mut self, header: &str) -> bool {
    self.etype = nth_etype(header, 0);
    return true;
  }

  fn hint_last(&mut self, last: NasIndex) {
    if let NasIndex::PointInElement(esp) = last {
      self.cur_row = Some(esp);
    }
  }

  fn last_row_index(&self) -> Option<NasIndex> {
    return self.cur_row.map(|q| q.into());
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    if line.contains("GRID-ID") {
      self.has_grid_id = true;
      return LineResponse::Metadata;
    }
    // first, take eight floats. if there aren't any, we're toast.
    let cols: [f64; Self::MATWIDTH] = if let Some(arr) = extract_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    // get the row ID.
    let fields = line_breakdown(line).collect::<Vec<_>>();
    let ints = fields
      .iter()
      .filter_map(|lf| {
        if let LineField::Integer(i) = lf {
          Some(i)
        } else {
          None
        }
      })
      .copied()
      .collect::<Vec<_>>();
    match self.flavour.solver {
      Some(Solver::Mystran) => {
        if let Some(LineField::Integer(eid)) = fields.first() {
          // first token is an integer: this is an element centroid row
          self.cur_row.replace(PointInElement {
            element: ElementRef {
              eid: *eid as usize,
              etype: self.etype,
            },
            point: ElementPoint::Centroid,
          });
        } else if let Some(gid) = ints.first() {
          // first token is not an integer (e.g. "GRD"): corner row
          if let Some(ri) = self.cur_row {
            self.cur_row.replace(PointInElement {
              element: ri.element,
              point: ElementPoint::Corner((*gid as usize).into()),
            });
          } else {
            warn!("GRD row without prior element row at {line}");
            return LineResponse::Abort;
          }
        } else {
          self.cur_row = None;
        }
      }
      Some(Solver::Simcenter | Solver::Msc) => {
        // line has row info
        let eid: usize;
        let point: ElementPoint;
        if self.has_grid_id {
          // line has grid id
          point = if line.contains("CEN/4") {
            ElementPoint::Centroid
          } else if let Some(gid) = ints.last() {
            ElementPoint::Corner((*gid as usize).into())
          } else {
            warn!("no point at {line}");
            return LineResponse::Abort;
          };
          eid = if let Some(x) = ints.get(1) {
            *x as usize
          } else if let Some(ri) = self.cur_row {
            ri.element.eid
          } else {
            warn!("no eid at {line}");
            return LineResponse::Abort;
          };
        } else {
          // line with no grid id. easier.
          point = ElementPoint::Centroid;
          eid = if let Some(x) = ints.last() {
            *x as usize
          } else {
            warn!("no eid at {line}");
            return LineResponse::Abort;
          };
        }
        self.cur_row.replace(PointInElement {
          element: ElementRef {
            eid,
            etype: self.etype,
          },
          point,
        });
      }
      None => return LineResponse::BadFlavour,
    };
    // if we got a row ID, insert.
    if let Some(rid) = self.cur_row {
      self.data.insert_raw(rid, &cols);
      return LineResponse::Data;
    } else {
      warn!("found data but couldn't construct row index at {line}");
      return LineResponse::Abort;
    }
  }
}

/// Decoder for tri element engineering forces.
pub(crate) struct TriaForcesDecoder {
  /// The flavour of solver we're decoding for.
  flavour: Flavour,
  /// The inner block of data.
  data: RowBlock<f64, ElementRef, PlateForceField, { Self::MATWIDTH }>,
  /// Element type, hinted by the header.
  etype: Option<ElementType>,
}

impl BlockDecoder for TriaForcesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = PlateForceField;
  const MATWIDTH: usize = 8;
  const BLOCK_TYPE: BlockType = BlockType::TriaForces;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(PlateForceField::canonical_cols()),
      etype: None,
    };
  }

  fn good_header(&mut self, header: &str) -> bool {
    self.etype = nth_etype(header, 0);
    return true;
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let cols: [f64; Self::MATWIDTH] = if let Some(arr) = extract_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    if let Some(eid) = nth_integer(line, 0) {
      let ri = ElementRef {
        eid: eid as usize,
        etype: self.etype,
      };
      self.data.insert_raw(ri, &cols);
      return LineResponse::Useless;
    } else {
      warn!("line had data but no eid");
      return LineResponse::Abort;
    }
  }
}

/// Decoder for ROD element engineering forces.
pub(crate) struct RodForcesDecoder {
  /// The inner block of data.
  data: RowBlock<f64, ElementRef, RodForceField, 2>,
}

impl BlockDecoder for RodForcesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = RodForceField;
  const MATWIDTH: usize = 2;
  const BLOCK_TYPE: BlockType = BlockType::RodForces;

  fn new(_flavour: Flavour) -> Self {
    return Self {
      data: RowBlock::new(RodForceField::canonical_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let mut fields = line_breakdown(line);
    let mut found = 0;
    loop {
      let (a, b, c) = (fields.next(), fields.next(), fields.next());
      match (a, b, c) {
        (
          Some(LineField::Integer(eid)),
          Some(LineField::Real(x)),
          Some(LineField::Real(y)),
        ) => {
          let ri = ElementRef {
            eid: eid as usize,
            etype: Some(ElementType::Rod),
          };
          self.data.insert_raw(ri, &[x, y]);
          found += 1;
        }
        _ => {
          break;
        }
      };
    }
    if found > 0 {
      return LineResponse::Data;
    } else {
      return LineResponse::Useless;
    }
  }
}

/// Decoder for bar engineering forces table.
pub(crate) struct BarForcesDecoder {
  /// The inner block of data.
  data: RowBlock<f64, ElementRef, BarForceField, 8>,
}

impl BlockDecoder for BarForcesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = BarForceField;
  const MATWIDTH: usize = 8;
  const BLOCK_TYPE: BlockType = BlockType::BarForces;

  fn new(_flavour: Flavour) -> Self {
    return Self {
      data: RowBlock::new(BarForceField::canonical_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let cols: [f64; 8] = if let Some(arr) = extract_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    if let Some(eid) = nth_integer(line, 0) {
      let ri = ElementRef {
        eid: eid as usize,
        etype: Some(ElementType::Bar),
      };
      self.data.insert_raw(ri, &cols);
      return LineResponse::Data;
    } else {
      warn!("no eid on bar force data line!");
      return LineResponse::Abort;
    }
  }
}

/// Decoder for ELAS1 engineering force blocks.
pub(crate) struct Elas1ForcesDecoder {
  /// The inner data block.
  data: RowBlock<f64, ElementRef, SingleForce, { Self::MATWIDTH }>,
}

impl BlockDecoder for Elas1ForcesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = SingleForce;
  const MATWIDTH: usize = 1;
  const BLOCK_TYPE: BlockType = BlockType::Elas1Forces;

  fn new(_flavour: Flavour) -> Self {
    return Self {
      data: RowBlock::new(SingleForce::canonical_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let mut fields = line_breakdown(line);
    let mut found = 0;
    loop {
      let (a, b) = (fields.next(), fields.next());
      match (a, b) {
        (Some(LineField::Integer(eid)), Some(LineField::Real(x))) => {
          let ri = ElementRef {
            eid: eid as usize,
            etype: Some(ElementType::Elas1),
          };
          self.data.insert_raw(ri, &[x]);
          found += 1;
        }
        _ => {
          break;
        }
      };
    }
    if found > 0 {
      return LineResponse::Data;
    } else {
      return LineResponse::Useless;
    }
  }
}

/// A decoder for triangular elements' stresses.
pub(crate) struct TriaStressesDecoder {
  /// The flavour of solver we're doing.
  flavour: Flavour,
  /// The data within.
  data: RowBlock<f64, ElementSidedPoint, PlateStressField, { Self::MATWIDTH }>,
  /// The current element ID.
  eid: Option<usize>,
  /// The element type (gleaned from the header).
  etype: Option<ElementType>,
}

impl BlockDecoder for TriaStressesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementSidedPoint;
  type ColumnIndex = PlateStressField;
  const MATWIDTH: usize = 8;
  const BLOCK_TYPE: BlockType = BlockType::TriaStresses;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(PlateStressField::canonical_cols()),
      eid: None,
      etype: None,
    };
  }

  fn good_header(&mut self, header: &str) -> bool {
    self.etype = nth_etype(header, 0);
    return true;
  }

  fn hint_last(&mut self, last: NasIndex) {
    if let NasIndex::ElementSidedPoint(esp) = last {
      self.etype = esp.element.etype;
      self.eid = Some(esp.element.eid);
    } else {
      panic!("bad header passed to hint_last");
    }
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let vals: [f64; 8] = if let Some(arr) = lax_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    let i0 = nth_natural(line, 0);
    let i1 = nth_natural(line, 1);
    self.eid = match (self.flavour.solver, self.eid) {
      (Some(Solver::Mystran), None) => i0,
      (Some(Solver::Mystran), Some(_)) => i0.or(self.eid),
      (Some(Solver::Simcenter | Solver::Msc), None) => i1,
      (Some(Solver::Simcenter | Solver::Msc), Some(_)) => i1.or(self.eid),
      (None, _) => return LineResponse::BadFlavour,
    };
    let esp = if let Some(eid) = self.eid {
      let element = ElementRef {
        eid,
        etype: self.etype,
      };
      let side = if nth_natural(line, 0).is_none() {
        ElementSide::Top
      } else {
        ElementSide::Bottom
      };
      let point = ElementPoint::Anywhere;
      ElementSidedPoint {
        element,
        point,
        side,
        corner_index: None,
      }
    } else {
      warn!("no eid on data line on {line}");
      return LineResponse::Abort;
    };
    self.data.insert_raw(esp, &vals);
    return LineResponse::Data;
  }
}

converting_decoder!(
  "Block decoder for strains in triangular elements.",
  TriaStrainsDecoder,
  TriaStressesDecoder,
  f64,
  (ElementSidedPoint, ElementSidedPoint),
  (PlateStrainField, PlateStressField),
  BlockType::TriaStrains,
  8
);

/// Decoder for "stresses in rod elements" tables.
pub(crate) struct RodStressesDecoder {
  /// The flavour of type we're decoding in.
  flavour: Flavour,
  /// The data within.
  data: RowBlock<f64, ElementRef, RodStressField, { Self::MATWIDTH }>,
}

impl BlockDecoder for RodStressesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = RodStressField;
  const MATWIDTH: usize = 4;
  const BLOCK_TYPE: BlockType = BlockType::RodStresses;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(RodStressField::canonical_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let mut added = 0;
    for (eid, floats) in int_pattern(line) {
      let arr: [f64; 4] = match floats.len() {
        2 => [floats[0], 0.0, floats[1], 0.0],
        3 => {
          if floats[0] == 0.0 {
            [floats[0], 0.0, floats[1], floats[2]]
          } else {
            [floats[0], floats[1], floats[2], 0.0]
          }
        }
        4 => [floats[0], floats[1], floats[2], floats[3]],
        0 => return LineResponse::Useless,
        _ => {
          warn!("got {} f64s for eid {} on line {}", floats.len(), eid, line);
          return LineResponse::Abort;
        }
      };
      let eref = ElementRef {
        eid,
        etype: Some(ElementType::Rod),
      };
      self.data.insert_raw(eref, &arr);
      added += 1;
    }
    if added > 0 {
      return LineResponse::Data;
    } else {
      return LineResponse::Useless;
    }
  }
}

converting_decoder!(
  "Decoder for \"strains in rod elements\" tables.",
  RodStrainsDecoder,
  RodStressesDecoder,
  f64,
  (ElementRef, ElementRef),
  (RodStrainField, RodStressField),
  BlockType::RodStrains,
  4
);

/// Decoder for "stresses in bar elements" tables.
pub(crate) struct BarStressesDecoder {
  /// The flavour of file we're decoding for.
  flavour: Flavour,
  /// The currently-known element ID and line data.
  curr: Option<(usize, BTreeMap<BarStressField, f64>)>,
  /// The data within.
  data: RowBlock<f64, ElementRef, BarStressField, 15>,
}

impl BlockDecoder for BarStressesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = BarStressField;
  const MATWIDTH: usize = 15;
  const BLOCK_TYPE: BlockType = BlockType::BarStresses;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      curr: None,
      data: RowBlock::new(BarStressField::canonical_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    /// Order of columns in the first row.
    const ORDER_L1: &[BarStressField] = &[
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 1,
      },
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 2,
      },
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 3,
      },
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndA,
        point: 4,
      },
      BarStressField::Axial,
      BarStressField::MaxAt(BarEnd::EndA),
      BarStressField::MinAt(BarEnd::EndA),
      BarStressField::SafetyMargin(NormalStressDirection::Tension),
    ];
    /// Order of columns in the second row.
    const ORDER_L2: &[BarStressField] = &[
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 1,
      },
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 2,
      },
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 3,
      },
      BarStressField::AtRecoveryPoint {
        end: BarEnd::EndB,
        point: 4,
      },
      BarStressField::MaxAt(BarEnd::EndB),
      BarStressField::MinAt(BarEnd::EndB),
      BarStressField::SafetyMargin(NormalStressDirection::Compression),
    ];
    let i0 = nth_natural(line, 0);
    let i1 = nth_natural(line, 1);
    if let Some(ui0) = i0 {
      // eid line
      let eid = match self.flavour.solver {
        Some(Solver::Mystran) => ui0,
        Some(Solver::Simcenter | Solver::Msc) => match i1 {
          Some(ui1) => ui1,
          None => {
            warn!("missing uid on data line {line}");
            return LineResponse::Abort;
          }
        },
        None => return LineResponse::BadFlavour,
      };
      // get data
      let vals: [f64; 8] = if let Some(arr) = extract_reals(line) {
        arr
      } else if let Some(arr7) = extract_reals::<7>(line) {
        [
          arr7[0], arr7[1], arr7[2], arr7[3], arr7[4], arr7[5], arr7[6], 0.0,
        ]
      } else {
        return LineResponse::Useless;
      };
      let cols: BTreeMap<BarStressField, f64> =
        ORDER_L1.iter().copied().zip(vals).collect();
      self.curr = Some((eid, cols));
      return LineResponse::Data;
    } else if let Some((eid, mut cols)) = self.curr.take() {
      // non-eid line. get some floats.
      let vals: [f64; 7] = if let Some(arr) = extract_reals(line) {
        arr
      } else if let Some(arr6) = extract_reals::<6>(line) {
        [arr6[0], arr6[1], arr6[2], arr6[3], arr6[4], arr6[5], 0.0]
      } else {
        warn!("non-data line whilst having an eid");
        return LineResponse::Abort;
      };
      ORDER_L2.iter().copied().zip(vals).for_each(|(k, v)| {
        cols.insert(k, v);
      });
      if cols.len() == Self::MATWIDTH {
        let eref = ElementRef {
          eid,
          etype: Some(ElementType::Bar),
        };
        self.data.insert_row(eref, &cols);
        return LineResponse::Data;
      } else {
        warn!("bad number of items in val map ({})", cols.len());
        return LineResponse::Abort;
      }
    } else if extract_reals::<7>(line).is_some() {
      // line has floats but no current line
      warn!("found second row without ever seeing a first, at {line}");
      return LineResponse::Abort;
    } else {
      // non-eid line with no data.
      return LineResponse::Useless;
    }
  }
}

converting_decoder!(
  "Decoder for \"strains in bar elements\" tables.",
  BarStrainsDecoder,
  BarStressesDecoder,
  f64,
  (ElementRef, ElementRef),
  (BarStrainField, BarStressField),
  BlockType::BarStrains,
  15
);

/// Decoder for ELAS1 element stresses.
pub(crate) struct Elas1StressesDecoder {
  /// The data within.
  data: RowBlock<f64, ElementRef, SingleStress, { Self::MATWIDTH }>,
}

impl BlockDecoder for Elas1StressesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = SingleStress;
  const MATWIDTH: usize = 1;
  const BLOCK_TYPE: BlockType = BlockType::Elas1Stresses;

  fn new(_flavour: Flavour) -> Self {
    return Self {
      data: RowBlock::new(SingleStress::canonical_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let etype = Some(ElementType::Elas1);
    let mut added: usize = 0;
    for (eid, floats) in int_pattern(line) {
      match floats.len() {
        0 => continue,
        1 => {
          let eref = ElementRef { eid, etype };
          let vals = [floats[0]];
          self.data.insert_raw(eref, &vals);
          added += 1;
        }
        _ => {
          warn!("more than one float in elas1 stress/strain line {line}");
          return LineResponse::Abort;
        }
      };
    }
    if added > 0 {
      return LineResponse::Data;
    } else {
      return LineResponse::Useless;
    }
  }
}

converting_decoder!(
  "Decoder for ELAS1 element strains",
  Elas1StrainsDecoder,
  Elas1StressesDecoder,
  f64,
  (ElementRef, ElementRef),
  (SingleStrain, SingleStress),
  BlockType::Elas1Strains,
  1
);

/// This decodes a bush forces block.
pub(crate) struct BushForcesDecoder {
  /// The flavour of F06 file we're decoding displacements for.
  flavour: Flavour,
  /// The displacement data.
  data: RowBlock<f64, ElementRef, Dof, { Self::MATWIDTH }>,
}

impl BlockDecoder for BushForcesDecoder {
  type MatScalar = f64;
  type RowIndex = ElementRef;
  type ColumnIndex = Dof;
  const MATWIDTH: usize = SIXDOF;
  const BLOCK_TYPE: BlockType = BlockType::BushForces;

  fn new(flavour: Flavour) -> Self {
    return Self {
      flavour,
      data: RowBlock::new(dof_cols()),
    };
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    return self.data.finalise(Self::BLOCK_TYPE, subcase, line_range);
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let dofs: [f64; SIXDOF] = if let Some(arr) = extract_reals(line) {
      arr
    } else {
      return LineResponse::Useless;
    };
    if let Some(eid) = last_natural(line) {
      let eref = ElementRef {
        eid,
        etype: Some(ElementType::Bush),
      };
      self.data.insert_raw(eref, &dofs);
      return LineResponse::Data;
    } else {
      warn!("bush line has six floats but no EID!");
      return LineResponse::Abort;
    }
  }
}

converting_decoder!(
  "Decoder for BUSH stresses",
  BushStressesDecoder,
  BushForcesDecoder,
  f64,
  (ElementRef, ElementRef),
  (Dof, Dof),
  BlockType::BushStresses,
  6
);

converting_decoder!(
  "Decoder for BUSH strains",
  BushStrainsDecoder,
  BushStressesDecoder,
  f64,
  (ElementRef, ElementRef),
  (Dof, Dof),
  BlockType::BushStrains,
  6
);

/// Decoder for eigenvectors.
pub struct EigenvectorDecoder {
  /// The flavour of F06 file we're decoding eigenvectors for.
  flavour: Flavour,
  /// The eigenvector data
  data: RowBlock<
    <Self as BlockDecoder>::MatScalar,
    <Self as BlockDecoder>::RowIndex,
    <Self as BlockDecoder>::ColumnIndex,
    { Self::MATWIDTH },
  >,
}

impl BlockDecoder for EigenvectorDecoder {
  type MatScalar = f64;
  type RowIndex = GridPointRef;
  type ColumnIndex = Dof;
  const MATWIDTH: usize = SIXDOF;
  const BLOCK_TYPE: BlockType = BlockType::Eigenvector;

  fn new(flavour: Flavour) -> Self {
    Self {
      flavour,
      data: RowBlock::new(dof_cols()),
    }
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    self.data.finalise(Self::BLOCK_TYPE, subcase, line_range)
  }

  // TODO: validate correctness for NX NASTRAN
  fn consume(&mut self, line: &str) -> LineResponse {
    const USELESS_DATA: [&str; 3] = ["MAX", "MIN", "ABS"];

    if USELESS_DATA.iter().any(|u| line.contains(u)) {
      return LineResponse::Useless;
    }
    let Some(dof) = extract_reals::<SIXDOF>(line) else {
      return LineResponse::Useless;
    };
    let Some(gid) = nth_natural(line, 0) else {
      return LineResponse::Unsupported;
    };
    // let Some(cid) = nth_natural(line, 1) else {
    //   return LineResponse::Unsupported;
    // };
    self.data.insert_raw((gid).into(), &dof);
    LineResponse::Data
  }
}

#[test]
fn eigenvector_mystran() {
  const TEST_BLOCK: &str =
  "                                            (in global coordinate system at each grid)
           GRID     COORD      T1            T2            T3            R1            R2            R3
                     SYS
              1        0  0.0           0.0           0.0           0.0           0.0           0.0
              2        0  0.0           0.0          -4.544341E-01  0.0           2.526497E-02  0.0
              3        0  0.0           0.0          -8.281750E-01  0.0          -1.678897E-03  0.0
              4        0  0.0           0.0          -2.724529E-01  0.0          -4.038712E-02  0.0
              5        0  0.0           0.0           1.000000E+00  0.0          -5.615361E-02  0.0
                         ------------- ------------- ------------- ------------- ------------- -------------
                MAX* :    0.0           0.0           1.000000E+00  0.0           2.526497E-02  0.0
                MIN* :    0.0           0.0          -8.281750E-01  0.0          -5.615361E-02  0.0

                ABS* :    0.0           0.0           1.000000E+00  0.0           5.615361E-02  0.0
                *for output set
  ";
  let mut dec = EigenvectorDecoder::new(Flavour {
    solver: Some(Solver::Mystran),
    soltype: Some(SolType::Eigenvalue),
  });
  for line in TEST_BLOCK.lines() {
    BlockDecoder::consume(&mut dec, line);
  }
  assert_eq!(dec.data.data.as_ref().unwrap().column_iter().count(), 6);
  assert_eq!(dec.data.data.as_ref().unwrap().row_iter().count(), 5);

  let grid_ids: Vec<_> = dec.data.row_indices.keys().map(|k| k.gid).collect();
  assert_eq!(grid_ids, [1, 2, 3, 4, 5]);
}

#[test]
fn eigenvector_scnastran() {
  const TEST_BLOCK: &str = "
      POINT ID.   TYPE          T1             T2             T3             R1             R2             R3
          1011      G      0.0            0.0            0.0           -1.784537E-02  -1.991141E-02  -1.244397E-04
          1012      G      0.0            0.0            0.0            1.689572E-01  -3.594943E-16  -7.820579E-16
          1013      G      0.0            0.0            0.0           -1.784537E-02   1.991141E-02   1.244397E-04
          1021      G      9.031904E-03   3.953749E-02  -1.000000E+00  -1.309015E-02  -1.874126E-01  -3.349282E-05
          1022      G      5.631406E-14   4.129383E-02   5.615814E+00  -2.341982E-02  -3.505873E-14  -3.092876E-15
          1023      G     -9.031904E-03   3.953749E-02  -1.000000E+00  -1.309015E-02   1.874126E-01   3.349282E-05
          1031      G      5.147679E-03   7.718235E-02  -1.302239E+00  -8.952033E-03   7.225054E-03   1.708938E-04
          1032      G      2.788087E-14   8.444247E-02  -2.354308E+00  -2.145707E-01   4.536302E-15   1.409571E-15
          1033      G     -5.147679E-03   7.718235E-02  -1.302239E+00  -8.952033E-03  -7.225054E-03  -1.708938E-04
          1041      G      0.0            0.0           -9.832847E-02   0.0            0.0            0.0
          1042      G      0.0            0.0           -1.865488E-01   0.0            0.0            0.0
          1043      G      0.0            0.0           -9.832847E-02   0.0            0.0            0.0
          1051      G      0.0            0.0            0.0            0.0            0.0            0.0
  ";
  let mut dec = EigenvectorDecoder::new(Flavour {
    solver: Some(Solver::Simcenter),
    soltype: Some(SolType::Eigenvalue),
  });
  for line in TEST_BLOCK.lines() {
    BlockDecoder::consume(&mut dec, line);
  }

  assert_eq!(dec.data.data.as_ref().unwrap().row_iter().count(), 13);

  let mut gids = dec.data.row_indices().keys().map(|k| k.gid);
  assert_eq!(gids.next(), Some(1011));
  assert_eq!(gids.next(), Some(1012));
  assert_eq!(gids.next(), Some(1013));
  assert_eq!(gids.next(), Some(1021));
  assert_eq!(gids.next(), Some(1022));
  assert_eq!(gids.next(), Some(1023));
  assert_eq!(gids.next(), Some(1031));
  assert_eq!(gids.next(), Some(1032));
  assert_eq!(gids.next(), Some(1033));
  assert_eq!(gids.next(), Some(1041));
  assert_eq!(gids.next(), Some(1042));
  assert_eq!(gids.next(), Some(1043));
  assert_eq!(gids.next(), Some(1051));
  assert_eq!(gids.next(), None);
}

/// Decoder for real eigenvalues.
pub struct RealEigenvaluesDecoder {
  /// The flavour of F06 file we're decoding eigenvectors for.
  flavour: Flavour,
  /// The eigenvalue data
  data: RowBlock<
    <Self as BlockDecoder>::MatScalar,
    <Self as BlockDecoder>::RowIndex,
    <Self as BlockDecoder>::ColumnIndex,
    { Self::MATWIDTH },
  >,
}

impl BlockDecoder for RealEigenvaluesDecoder {
  type MatScalar = f64;
  type RowIndex = EigenModeNumber;
  type ColumnIndex = RealEigenvalueField;
  const MATWIDTH: usize = 5;
  const BLOCK_TYPE: BlockType = BlockType::RealEigenvalues;

  fn new(flavour: Flavour) -> Self {
    // TODO: validate order for NX NASTRAN
    let col_idxs = RealEigenvalueField::all()
      .iter()
      .copied()
      .enumerate()
      .map(|(i, f)| (f, i))
      .collect();

    Self {
      flavour,
      data: RowBlock::new(col_idxs),
    }
  }

  fn unwrap(
    self,
    subcase: usize,
    line_range: Option<(usize, usize)>,
  ) -> FinalBlock {
    self.data.finalise(Self::BLOCK_TYPE, subcase, line_range)
  }

  fn consume(&mut self, line: &str) -> LineResponse {
    let Some(dof) = extract_reals::<5>(line) else {
      return LineResponse::Useless;
    };
    let Some(mode) = nth_natural(line, 0) else {
      return LineResponse::Unsupported;
    };
    self.data.insert_raw(EigenModeNumber(mode as i32), &dof);
    LineResponse::Data
  }
}

#[test]
fn real_eigenvalues_mystran() {
  const MYSTRAN_BLOCK: &str =
  "
      MODE  EXTRACTION      EIGENVALUE           RADIANS              CYCLES            GENERALIZED         GENERALIZED
     NUMBER   ORDER                                                                        MASS              STIFFNESS

        1       1        4.509067E+03        6.714959E+01        1.068719E+01        6.800571E-04        3.066423E+00
        2       2        1.557785E+05        3.946879E+02        6.281653E+01        9.499997E-04        1.479896E+02
        3       3        1.092406E+06        1.045182E+03        1.663460E+02        2.094704E-03        2.288268E+03
        4       4        3.318753E+06        1.821744E+03        2.899396E+02        1.147843E-02        3.809406E+04
  ";

  let mut dec = RealEigenvaluesDecoder::new(Flavour {
    solver: Some(Solver::Mystran),
    soltype: Some(SolType::Eigenvalue),
  });

  for line in MYSTRAN_BLOCK.lines() {
    BlockDecoder::consume(&mut dec, line);
  }
  let mut row_idxs = dec.data.row_indices().keys().copied();

  assert_eq!(row_idxs.next(), Some(EigenModeNumber(1)));
  assert_eq!(row_idxs.next(), Some(EigenModeNumber(2)));
  assert_eq!(row_idxs.next(), Some(EigenModeNumber(3)));
  assert_eq!(row_idxs.next(), Some(EigenModeNumber(4)));
  assert_eq!(row_idxs.next(), None);

  assert_eq!(dec.data.data.as_ref().unwrap().row_iter().count(), 4);
  assert_eq!(dec.data.data.as_ref().unwrap().column_iter().count(), 5);
}

#[test]
fn real_eigenvalues_scnastran() {
  const MYSTRAN_BLOCK: &str =
  "   MODE    EXTRACTION      EIGENVALUE            RADIANS             CYCLES            GENERALIZED         GENERALIZED
    NO.       ORDER                                                                       MASS              STIFFNESS
        1        20        4.690559E+04        2.165770E+02        3.446930E+01        1.753081E-01        8.222928E+03
        2        21        6.113262E+04        2.472501E+02        3.935108E+01        1.865598E-01        1.140489E+04
        3        19        1.568042E+05        3.959851E+02        6.302299E+01        9.966058E+00        1.562720E+06
        4        18        2.163330E+05        4.651161E+02        7.402553E+01        4.716262E+00        1.020283E+06
        5        16        2.885671E+05        5.371845E+02        8.549556E+01        0.0                 0.0
        6        17        3.162935E+05        5.623998E+02        8.950871E+01        0.0                 0.0
        7        15        1.028630E+06        1.014214E+03        1.614171E+02        0.0                 0.0
        8        10        4.670556E+06        2.161147E+03        3.439572E+02        0.0                 0.0
        9        13        5.985235E+06        2.446474E+03        3.893684E+02        0.0                 0.0
       10        14        6.138342E+06        2.477568E+03        3.943171E+02        0.0                 0.0
       11        11        6.832876E+06        2.613977E+03        4.160274E+02        0.0                 0.0
       12        12        7.011209E+06        2.647869E+03        4.214214E+02        0.0                 0.0
       13         9        1.140055E+07        3.376469E+03        5.373818E+02        0.0                 0.0
       14         8        1.437963E+07        3.792048E+03        6.035232E+02        0.0                 0.0
       15         7        1.889738E+07        4.347112E+03        6.918643E+02        0.0                 0.0
       16         6        2.477192E+07        4.977140E+03        7.921364E+02        0.0                 0.0
       17         5        3.338171E+07        5.777691E+03        9.195480E+02        0.0                 0.0
       18         4        3.980928E+07        6.309460E+03        1.004182E+03        0.0                 0.0
       19         3        4.313654E+07        6.567841E+03        1.045304E+03        0.0                 0.0
       20         2        5.013103E+07        7.080327E+03        1.126869E+03        0.0                 0.0
       21         1        5.639166E+07        7.509438E+03        1.195164E+03        0.0                 0.0
  ";

  let mut dec = RealEigenvaluesDecoder::new(Flavour {
    solver: Some(Solver::Mystran),
    soltype: Some(SolType::Eigenvalue),
  });

  for line in MYSTRAN_BLOCK.lines() {
    BlockDecoder::consume(&mut dec, line);
  }
  assert!(dec
    .data
    .row_indices()
    .keys()
    .copied()
    .enumerate()
    .all(|(i, idx)| idx.0 as usize == i + 1));
  assert_eq!(dec.data.data.as_ref().unwrap().row_iter().count(), 21);
  assert_eq!(dec.data.data.as_ref().unwrap().column_iter().count(), 5);
}
