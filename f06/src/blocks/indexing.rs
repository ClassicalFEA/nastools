//! This submodule implements several indexing types used to acces values in an
//! output block.

use std::collections::BTreeMap;
use std::fmt::{Debug as DebugTrait, Display};
use std::str::FromStr;

use convert_case::{Case, Casing};
use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// Folds a user-supplied index name down to a comparable form: lower-cased,
/// with all non-alphanumeric runs collapsed into single underscores.
pub fn fold_index_name(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut last_us = true;
  for c in s.chars() {
    if c.is_ascii_alphanumeric() {
      for d in c.to_lowercase() {
        out.push(d);
      }
      last_us = false;
    } else if !last_us {
      out.push('_');
      last_us = true;
    }
  }
  while out.ends_with('_') {
    out.pop();
  }
  return out;
}

/// Returns true if two strings match after folding via [`fold_index_name`].
pub fn name_matches(a: &str, b: &str) -> bool {
  return fold_index_name(a) == fold_index_name(b);
}

/// Generates a NasIndex type from pure enum fields. Saves some time.
/// Pass `()` for the index_name to skip the [`IndexType`] impl (used for
/// auxiliary enums like `BarEnd` that aren't full row/column indices).
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
    from_enum!(@core $desc, $tname, [$(($varname, $varstr),)+]);
  };

  (
    $desc:literal,
    $tname:ident,
    $idx_name:literal,
    [
      $(
        ($varname:ident, $varstr:literal),
      )+
    ]
  ) => {
    from_enum!(@core $desc, $tname, [$(($varname, $varstr),)+]);

    impl IndexType for $tname {
      const INDEX_NAME: &'static str = $idx_name;

      fn legal_values() -> Option<Vec<String>> {
        return Some(Self::canonical_legal_values());
      }
    }
  };

  (
    @core
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

      /// Returns the canonical (snake_case) string form of every variant.
      pub fn canonical_legal_values() -> Vec<String> {
        return [$(stringify!($varname),)+]
          .iter()
          .map(|n| n.to_case(Case::Snake))
          .collect();
      }
    }

    impl Display for $tname {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        return write!(f, "{}", self.name());
      }
    }

    impl FromStr for $tname {
      type Err = String;

      fn from_str(s: &str) -> Result<Self, Self::Err> {
        let folded = fold_index_name(s);
        $(
          if folded == fold_index_name(stringify!($varname))
            || folded == stringify!($varname).to_case(Case::Snake)
            || folded == fold_index_name($varstr)
          {
            return Ok(Self::$varname);
          }
        )+
        return Err(format!(
          concat!("\"{}\" is not a valid ", stringify!($tname), " value"),
          s
        ));
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

      fn legal_values() -> Option<Vec<String>> {
        return <$inner_type as IndexType>::legal_values();
      }
    }

    impl FromStr for $outer_type {
      type Err = <$inner_type as FromStr>::Err;

      fn from_str(s: &str) -> Result<Self, Self::Err> {
        return <$inner_type as FromStr>::from_str(s).map(Self);
      }
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

      /// Returns the snake_case name of the type of this index, suitable as
      /// a key in a script.
      pub fn type_key(&self) -> String {
        return match self {
          $(Self::$tn(_) => stringify!($tn).to_case(Case::Snake),)*
        };
      }

      /// Returns the entries of the parser registry: one per known index
      /// subtype, with its snake_case key, its all-caps display name, an
      /// optional list of legal canonical values, and a parser function.
      pub fn parsers() -> Vec<NasIndexParser> {
        return vec![
          $(
            NasIndexParser {
              type_key: stringify!($tn).to_case(Case::Snake),
              type_name: <$tn as IndexType>::INDEX_NAME,
              legal_values: <$tn as IndexType>::legal_values(),
              parse: |s| <$tn as FromStr>::from_str(s)
                .map(NasIndex::from)
                .map_err(|e| e.to_string()),
            },
          )*
        ];
      }

      /// Tries to parse a string into a NasIndex. If `narrow` is supplied,
      /// only parsers whose type_name is in the slice are tried.
      ///
      /// An optional `type:value` prefix can force a specific subtype, in
      /// which case `narrow` is still respected as an additional filter.
      pub fn parse_lenient(
        s: &str,
        narrow: Option<&[&str]>,
      ) -> Result<Self, ParseLenientError> {
        let (forced, payload) = match s.split_once(':') {
          Some((tag, rest)) => {
            let tag_folded = fold_index_name(tag);
            let mut found: Option<&'static str> = None;
            for p in Self::parsers().iter() {
              if fold_index_name(&p.type_key) == tag_folded
                || fold_index_name(p.type_name) == tag_folded
              {
                found = Some(p.type_name);
                break;
              }
            }
            match found {
              Some(t) => (Some(t), rest.trim()),
              None => (None, s),
            }
          }
          None => (None, s),
        };
        let mut hits: Vec<NasIndex> = Vec::new();
        let mut hit_types: Vec<&'static str> = Vec::new();
        let mut considered = 0usize;
        for p in Self::parsers().into_iter() {
          if let Some(t) = forced {
            if p.type_name != t {
              continue;
            }
          }
          if let Some(allow) = narrow {
            if !allow.contains(&p.type_name) {
              continue;
            }
          }
          considered += 1;
          if let Ok(v) = (p.parse)(payload) {
            hits.push(v);
            hit_types.push(p.type_name);
          }
        }
        if considered == 0 {
          return Err(ParseLenientError::NoCandidateTypes {
            input: s.to_owned(),
          });
        }
        match hits.len() {
          0 => Err(ParseLenientError::NoMatch {
            input: s.to_owned(),
          }),
          1 => Ok(hits.into_iter().next().unwrap()),
          _ => Err(ParseLenientError::Ambiguous {
            input: s.to_owned(),
            matched_types: hit_types
              .into_iter()
              .map(|t| t.to_owned())
              .collect(),
          }),
        }
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
  EigenModeNumber,
);

/// All field indexing types must implement this trait.
pub trait IndexType:
  Copy + Ord + Eq + Into<NasIndex> + Display + DebugTrait + FromStr
{
  /// The name of this type of index, all caps.
  const INDEX_NAME: &'static str;

  /// Returns the canonical, lower-case list of all legal values for this
  /// index type, if it can be enumerated. Returns `None` for index types
  /// that range over an unbounded domain (e.g. element IDs).
  fn legal_values() -> Option<Vec<String>> {
    return None;
  }
}

/// Description of one entry in the [`NasIndex`] parser registry.
pub struct NasIndexParser {
  /// The snake_case Rust type name; usable as a `type:value` prefix in a
  /// script.
  pub type_key: String,
  /// The all-caps display name (matches `IndexType::INDEX_NAME`).
  pub type_name: &'static str,
  /// All legal canonical string values, if enumerable.
  pub legal_values: Option<Vec<String>>,
  /// Tries to parse the value portion of an index string into a NasIndex.
  pub parse: fn(&str) -> Result<NasIndex, String>,
}

/// Errors that may arise when parsing a string into a [`NasIndex`].
#[derive(Debug, Clone)]
pub enum ParseLenientError {
  /// No index type matched the input.
  NoMatch {
    /// The input string.
    input: String,
  },
  /// More than one index type matched the input.
  Ambiguous {
    /// The input string.
    input: String,
    /// The all-caps names of the index types that matched.
    matched_types: Vec<String>,
  },
  /// The narrowing filter excluded all parsers.
  NoCandidateTypes {
    /// The input string.
    input: String,
  },
}

impl Display for ParseLenientError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::NoMatch { input } => {
        write!(f, "\"{input}\" does not name any known index value")
      }
      Self::Ambiguous {
        input,
        matched_types,
      } => write!(
        f,
        "\"{input}\" is ambiguous, matched: {} (try a `type:value` prefix)",
        matched_types.join(", ")
      ),
      Self::NoCandidateTypes { input } => {
        write!(f, "no index types apply to \"{input}\" in this context")
      }
    };
  }
}

impl std::error::Error for ParseLenientError {}

impl IndexType for Dof {
  const INDEX_NAME: &'static str = "DOF";

  fn legal_values() -> Option<Vec<String>> {
    return Some(
      Self::all()
        .iter()
        .map(|d| format!("{}{}", d.dof_type.letter(), d.axis.letter()))
        .map(|s| s.to_lowercase())
        .collect(),
    );
  }
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

impl FromStr for ForceOrigin {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let folded = fold_index_name(s);
    if folded == "load" || folded == "applied_load" {
      return Ok(Self::Load);
    }
    if folded == "spc" || folded == "single_point_constraint" {
      return Ok(Self::SinglePointConstraint);
    }
    if folded == "mpc" || folded == "multi_point_constraint" {
      return Ok(Self::MultiPointConstraint);
    }
    if let Some(rest) = folded.strip_prefix("elem_") {
      let eid: usize = rest
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
      return Ok(Self::Element { elem: eid.into() });
    }
    return Err(format!(
      "\"{s}\" is not a valid force origin; try one of \
       load, spc, mpc, elem_<id>"
    ));
  }
}

impl FromStr for GridPointForceOrigin {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let (origin_part, gid_part) = s.split_once('@').ok_or_else(|| {
      format!(
        "\"{s}\" is not a valid GRID POINT FORCE ORIGIN; \
         expected \"<origin>@<gid>\" -- e.g. \"load@42\", \"spc@42\", \
         \"mpc@42\", \"elem_100@42\""
      )
    })?;
    let force_origin = ForceOrigin::from_str(origin_part.trim())?;
    let gid: usize = gid_part
      .trim()
      .parse()
      .map_err(|e: std::num::ParseIntError| e.to_string())?;
    return Ok(Self {
      grid_point: gid.into(),
      force_origin,
    });
  }
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

impl FromStr for ElementPoint {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let folded = fold_index_name(s);
    if folded == "centroid" {
      return Ok(Self::Centroid);
    }
    if folded == "anywhere" || folded == "anywhere_in_the_element" {
      return Ok(Self::Anywhere);
    }
    if let Some(rest) = folded.strip_prefix("corner_") {
      let gid: usize = rest
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
      return Ok(Self::Corner(gid.into()));
    }
    if let Some(rest) = folded.strip_prefix("midpoint_") {
      let gid: usize = rest
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
      return Ok(Self::Midpoint(gid.into()));
    }
    return Err(format!(
      "\"{s}\" is not a valid element point; try one of \
       centroid, anywhere, corner_<gid>, midpoint_<gid>"
    ));
  }
}

impl FromStr for PointInElement {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let (eid_part, point_part) = s.split_once('/').ok_or_else(|| {
      format!(
        "\"{s}\" is not a valid POINT IN ELEMENT; \
         expected \"<eid>/<point>\" -- e.g. \"100/centroid\", \
         \"100/corner_42\", \"100/midpoint_42\", \"100/anywhere\""
      )
    })?;
    let eid: usize = eid_part
      .trim()
      .parse()
      .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let point = ElementPoint::from_str(point_part.trim())?;
    return Ok(Self {
      element: eid.into(),
      point,
    });
  }
}

/// An element and a point within it, plus a side.
#[derive(
  Copy, Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq,
)]
pub struct ElementSidedPoint {
  /// A reference to the element.
  pub element: ElementRef,
  /// The point within the element.
  pub point: ElementPoint,
  /// The side.
  pub side: ElementSide,
  /// The 1-based corner position within the element's connectivity list, if
  /// known. Only set for GRD (corner) rows in plate stress/strain blocks.
  pub corner_index: Option<u8>,
}

impl Display for ElementSidedPoint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}, {}, {}", self.element, self.point, self.side);
  }
}

impl IndexType for ElementSidedPoint {
  const INDEX_NAME: &'static str = "ELEMENT, POINT AND SIDE";
}

impl FromStr for ElementSide {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let folded = fold_index_name(s);
    return match folded.as_str() {
      "top" | "z2" | "top_side" => Ok(Self::Top),
      "bottom" | "z1" | "bottom_side" => Ok(Self::Bottom),
      _ => Err(format!(
        "\"{s}\" is not a valid element side; try \"top\" or \"bottom\""
      )),
    };
  }
}

impl FromStr for ElementSidedPoint {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let mut parts = s.splitn(3, '/');
    let eid_part = parts.next().unwrap_or("");
    let point_part = parts.next();
    let side_part = parts.next();
    let (point_part, side_part) = match (point_part, side_part) {
      (Some(p), Some(d)) => (p, d),
      _ => {
        return Err(format!(
          "\"{s}\" is not a valid ELEMENT, POINT AND SIDE; \
           expected \"<eid>/<point>/<side>\" -- e.g. \
           \"100/centroid/top\", \"100/corner_42/bottom\""
        ))
      }
    };
    let eid: usize = eid_part
      .trim()
      .parse()
      .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let point = ElementPoint::from_str(point_part.trim())?;
    let side = ElementSide::from_str(side_part.trim())?;
    return Ok(Self {
      element: eid.into(),
      point,
      side,
      corner_index: None,
    });
  }
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
  "PLATE STRESS FIELD",
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

gen_with_inner!(
  "The columns for the strains table for plate elements.",
  "PLATE STRAIN FIELD",
  PlateStrainField,
  PlateStressField
);

from_enum!(
  "The columns for the engineering forces table for a quadrilateral element.",
  PlateForceField,
  "2D ELEM FORCE FIELD",
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

from_enum!(
  "Engineering forces for ROD elements.",
  RodForceField,
  "ROD FORCE FIELD",
  [(AxialForce, "AXIAL FORCE"), (Torque, "TORQUE"),]
);

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

  fn legal_values() -> Option<Vec<String>> {
    return Some(
      Self::all()
        .iter()
        .map(|v| fold_index_name(&v.to_string()))
        .collect(),
    );
  }
}

impl FromStr for BarForceField {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let folded = fold_index_name(s);
    for v in Self::all().iter() {
      if fold_index_name(&v.to_string()) == folded {
        return Ok(*v);
      }
    }
    return Err(format!(
      "\"{s}\" is not a valid BarForceField; try one of {}",
      Self::all()
        .iter()
        .map(|v| fold_index_name(&v.to_string()))
        .collect::<Vec<_>>()
        .join(", ")
    ));
  }
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
  "FORCE",
  [(Force, "FORCE"),]
);

from_enum!(
  "Generic single-stress field.",
  SingleStress,
  "STRESS",
  [(Stress, "STRESS"),]
);

from_enum!(
  "Generic single-strain field.",
  SingleStrain,
  "STRAIN",
  [(Strain, "STRAIN"),]
);

impl From<SingleStress> for SingleStrain {
  fn from(_value: SingleStress) -> Self {
    return Self::Strain;
  }
}

from_enum!(
  "Rod element stress field.",
  RodStressField,
  "ROD STRESS FIELD",
  [
    (Axial, "AXIAL"),
    (AxialSafetyMargin, "AXIAL SAFETY MARGIN"),
    (Torsional, "TORSIONAL"),
    (TorsionalSafetyMargin, "TORSIONAL SAFETY MARGIN"),
  ]
);

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

  fn legal_values() -> Option<Vec<String>> {
    return Some(
      Self::all()
        .iter()
        .map(|v| fold_index_name(&v.to_string()))
        .collect(),
    );
  }
}

impl FromStr for BarStressField {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let folded = fold_index_name(s);
    for v in Self::all().iter() {
      if fold_index_name(&v.to_string()) == folded {
        return Ok(*v);
      }
    }
    return Err(format!(
      "\"{s}\" is not a valid BarStressField; try one of {}",
      Self::all()
        .iter()
        .map(|v| fold_index_name(&v.to_string()))
        .collect::<Vec<_>>()
        .join(", ")
    ));
  }
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

impl FromStr for GridPointCsys {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let parts: Vec<&str> = s
      .split(|c: char| !c.is_ascii_digit())
      .filter(|p| !p.is_empty())
      .collect();
    if parts.len() != 2 {
      return Err(format!(
        "\"{s}\" is not a valid GridPointCsys; expected \"<gid> on <cid>\""
      ));
    }
    let gid: usize = parts[0]
      .parse()
      .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let cid: usize = parts[1]
      .parse()
      .map_err(|e: std::num::ParseIntError| e.to_string())?;
    return Ok(Self::from((gid, cid)));
  }
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
pub struct EigenModeNumber(pub i32);

impl IndexType for EigenModeNumber {
  const INDEX_NAME: &'static str = "MODE";
}

impl FromStr for EigenModeNumber {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let trimmed = s.trim();
    let digits = trimmed
      .strip_prefix("mode_")
      .or_else(|| trimmed.strip_prefix("MODE_"))
      .or_else(|| trimmed.strip_prefix("mode"))
      .or_else(|| trimmed.strip_prefix("MODE"))
      .map(|r| r.trim_start_matches('_').trim())
      .unwrap_or(trimmed);
    return digits.parse::<i32>().map(Self).map_err(|e| e.to_string());
  }
}

impl Display for EigenModeNumber {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("MODE")
  }
}

from_enum!(
  "Field Values for Real Eigenvalues",
  RealEigenvalueField,
  "EIGENVALUE FIELDS",
  [
    (Eigenvalue, "EIGENVALUE"),
    (Radians, "RADIANS"),
    (Cycles, "CYCLES"),
    (GeneralizedMass, "GENERALIZED MASS"),
    (GeneralizedStiffness, "GENERALIZED STIFFNESS"),
  ]
);
