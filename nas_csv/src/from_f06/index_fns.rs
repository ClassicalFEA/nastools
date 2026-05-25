//! Contains several functions to extract fields from NasIndex variants.

use f06::prelude::*;

use crate::prelude::*;

/// Constant for commonly-used error here.
fn bad_col_type<T>(index: NasIndex) -> Result<T, ConversionError> {
  return Err(ConversionError::BadColIndexType(index));
}

/// Attempts to extract a grid point ID from an index type.
pub fn ixfn_gid(index: NasIndex) -> Result<CsvField, ConversionError> {
  return Ok(
    match index {
      NasIndex::GridPointRef(g) => g.gid,
      NasIndex::PointInElement(pie) => match pie.point {
        ElementPoint::Corner(g) => g.gid,
        _ => return bad_col_type(index),
      },
      NasIndex::GridPointForceOrigin(gpfo) => gpfo.grid_point.gid,
      NasIndex::ElementSidedPoint(esp) => match esp.point {
        ElementPoint::Corner(g) => g.gid,
        _ => return bad_col_type(index),
      },
      NasIndex::GridPointCsys(g) => g.gid.gid,
      _ => return bad_col_type(index),
    }
    .into(),
  );
}

/// Utility functions: extracts element references from index types.
fn util_eref(index: NasIndex) -> Result<ElementRef, ConversionError> {
  return Ok(match index {
    NasIndex::ElementRef(eref) => eref,
    NasIndex::PointInElement(pie) => pie.element,
    NasIndex::GridPointForceOrigin(gpfo) => match gpfo.force_origin {
      ForceOrigin::Element { elem } => elem,
      _ => return bad_col_type(index),
    },
    NasIndex::ElementSidedPoint(esp) => esp.element,
    _ => return bad_col_type(index),
  });
}

/// Attempts to extract an element ID from an index type.
pub fn ixfn_eid(index: NasIndex) -> Result<CsvField, ConversionError> {
  return util_eref(index).map(|eref| eref.eid.into());
}

/// Attempts to extract an element type from an index type.
pub fn ixfn_etype(index: NasIndex) -> Result<CsvField, ConversionError> {
  if let Some(etype) = util_eref(index).map(|eref| eref.etype)? {
    return Ok(etype.into());
  } else {
    return Ok("<UNKNOWN>".to_owned().into());
  }
}

/// Extracts a force origin into a shorter string.
pub fn ixfn_fo(index: NasIndex) -> Result<CsvField, ConversionError> {
  if let NasIndex::GridPointForceOrigin(gpfo) = index {
    return Ok(
      match gpfo.force_origin {
        ForceOrigin::Load => "APPLIED".to_owned(),
        ForceOrigin::Element { elem } => match elem.etype {
          Some(et) => et.to_string(),
          None => "<ELEM>".to_string(),
        },
        ForceOrigin::SinglePointConstraint => "SPC".to_string(),
        ForceOrigin::MultiPointConstraint => "MPC".to_string(),
      }
      .into(),
    );
  } else {
    return Err(ConversionError::BadColIndexType(index));
  }
}

/// Extracts the Eigen solution mode
pub fn ixfn_eigen_mode(index: NasIndex) -> Result<CsvField, ConversionError> {
  match index {
    NasIndex::EigenModeNumber(m) => Ok(CsvField::Natural(m.0 as usize)),
    idx => Err(ConversionError::BadRowIndexType(idx)),
  }
}

/// Extracts a Coordinate System ID
pub fn ixfn_csys(index: NasIndex) -> Result<CsvField, ConversionError> {
  match index {
    NasIndex::GridPointCsys(idx) => Ok(CsvField::Natural(idx.cid.cid)),
    idx => Err(ConversionError::BadRowIndexType(idx)),
  }
}

/// Extracts the 1-based corner position within the element's connectivity
/// list. Returns 0 for centroid rows or any element point that is not a
/// numbered corner (e.g. Anywhere).
pub fn ixfn_corner_index(index: NasIndex) -> Result<CsvField, ConversionError> {
  match index {
    NasIndex::ElementSidedPoint(esp) => {
      Ok(CsvField::Natural(esp.corner_index.unwrap_or(0) as usize))
    }
    _ => bad_col_type(index),
  }
}
