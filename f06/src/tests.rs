use crate::util::{decode_nasfloat, line_breakdown, unspace, LineField};

#[test]
fn test_decode_nasfloat() {
  let epsilon = 1e-6_f64;
  let assert_near = |a: f64, b: f64| assert!((a - b).abs() < epsilon);
  let direct = |s: &str, f: f64| assert_near(decode_nasfloat(s).unwrap(), f);
  let parsed = |s: &str| direct(s, s.parse().unwrap());
  let must_fail = |s: &str| assert_eq!(decode_nasfloat(s), None);
  let may_fail = |s: &str, f: f64| {
    assert_near(decode_nasfloat(s).unwrap_or(f), f);
  };
  // first, some "normal" cases
  // possible signs
  let signs = ["", "+", "-"];
  // possible separators
  let seps = ["", "e", "E"];
  // some mantissas
  let mantissas = ["0", "1", "0.25", ".25", "3.1415"];
  // some exponents
  let exponents = ["0", "1", "2", "3", "10"];
  for msign in signs.iter() {
    for m in mantissas.iter() {
      // test just a mantissa
      parsed(&format!("{msign}{m}"));
      for sep in seps.iter() {
        for e in exponents.iter() {
          for esign in signs.iter() {
            if sep.is_empty() && esign.is_empty() {
              continue;
            }
            let nf = format!("{msign}{m}{sep}{esign}{e}");
            let rf = format!("{msign}{m}e{esign}{e}");
            direct(&nf, rf.parse().unwrap());
          }
        }
      }
    }
  }
  // some weird zeros that we don't really care about
  for ksep in seps.iter().chain(signs.iter()).filter(|s| !s.is_empty()) {
    for msign in signs.iter() {
      may_fail(&format!("{msign}.{ksep}."), 0.0);
    }
  }
  // now some bad cases
  must_fail("");
  must_fail("+");
  must_fail("-");
  must_fail("e");
  must_fail("E");
  must_fail("++");
  must_fail("--");
  must_fail(".");
  must_fail("..");
  must_fail("e.");
  must_fail("E.");
  must_fail(".e");
  must_fail(".E");
}

#[test]
fn test_decode_headers() {
  let my_displ = "                                  D I S P L A C E M E N T S";
  assert_ne!(unspace(my_displ), None);
}

#[test]
fn test_line_breakdown_splits_glued_exponent() {
  let eps = 1e-9_f64;
  let near = |a: f64, b: f64| assert!((a - b).abs() < eps, "{a} vs {b}");
  let fields = |s: &'static str| line_breakdown(s).collect::<Vec<_>>();

  // The bug-driving case: a zero-valued field abutting an 8-digit element ID.
  match fields("0.000000E+0022345678")[..] {
    [LineField::Real(x), LineField::Integer(i)] => {
      near(x, 0.0);
      assert_eq!(i, 22345678);
    }
    ref other => panic!("unexpected: {other:?}"),
  }

  // Non-zero mantissa case (would have become +inf without the split).
  match fields("1.293243E+031234")[..] {
    [LineField::Real(x), LineField::Integer(i)] => {
      near(x, 1.293243e3);
      assert_eq!(i, 1234);
    }
    ref other => panic!("unexpected: {other:?}"),
  }

  // Negative exponent.
  match fields("1.500000E-031234")[..] {
    [LineField::Real(x), LineField::Integer(i)] => {
      near(x, 1.5e-3);
      assert_eq!(i, 1234);
    }
    ref other => panic!("unexpected: {other:?}"),
  }

  // Lower-case `e` form.
  match fields("2.5e+0142")[..] {
    [LineField::Real(x), LineField::Integer(i)] => {
      near(x, 2.5e1);
      assert_eq!(i, 42);
    }
    ref other => panic!("unexpected: {other:?}"),
  }

  // No collision -- must pass through untouched.
  match fields("1.293243E+03")[..] {
    [LineField::Real(x)] => near(x, 1.293243e3),
    ref other => panic!("unexpected: {other:?}"),
  }

  // Plain integer -- untouched.
  match fields("12345")[..] {
    [LineField::Integer(12345)] => {}
    ref other => panic!("unexpected: {other:?}"),
  }

  // Plain decimal without exponent -- untouched.
  match fields("1.23")[..] {
    [LineField::Real(x)] => near(x, 1.23),
    ref other => panic!("unexpected: {other:?}"),
  }

  // Tail doesn't start with a digit -- must NOT split.
  // `1.5E+03ROD` would not be a glued-ID case; we should leave it alone so
  // the `ElementType`/`NoIdea` matcher can handle it.
  let f = fields("1.5E+03ROD");
  assert_eq!(f.len(), 1);
  assert!(matches!(
    f[0],
    LineField::ElementType(_) | LineField::NoIdea(_) | LineField::Real(_)
  ));

  // 1-digit exponent (shouldn't appear in F06, but defensive) -- untouched.
  match fields("1.234567E+1")[..] {
    [LineField::Real(x)] => near(x, 1.234567e1),
    ref other => panic!("unexpected: {other:?}"),
  }

  // Multiple whitespace-separated tokens, one of which is glued.
  let f = fields("5  0.000000E+00  0.000000E+0022345678  1.293243E+03");
  assert_eq!(f.len(), 5, "fields: {f:?}");
  match f[..] {
    [LineField::Integer(5), LineField::Real(a), LineField::Real(b), LineField::Integer(22345678), LineField::Real(c)] =>
    {
      near(a, 0.0);
      near(b, 0.0);
      near(c, 1.293243e3);
    }
    ref other => panic!("unexpected: {other:?}"),
  }
}

#[test]
fn test_parse_lenient_dof() {
  use crate::prelude::*;
  let parsed = NasIndex::parse_lenient("tx", None).unwrap();
  assert!(matches!(parsed, NasIndex::Dof(_)));
  let upper = NasIndex::parse_lenient("Tx", None).unwrap();
  assert_eq!(parsed, upper);
}

#[test]
fn test_parse_lenient_bar_stress_via_display() {
  use crate::prelude::*;
  let allow = [BlockType::BarStresses.col_index_kind()];
  let v = NasIndex::parse_lenient("max_at_end_a", Some(&allow)).unwrap();
  assert_eq!(v.type_name(), BlockType::BarStresses.col_index_kind());
}

#[test]
fn test_parse_lenient_ambiguous_disambiguated_by_narrow() {
  use crate::prelude::*;
  let allow = [BlockType::BarStresses.col_index_kind()];
  NasIndex::parse_lenient("axial", Some(&allow)).unwrap();
}

#[test]
fn test_parse_lenient_unknown() {
  use crate::prelude::*;
  let r = NasIndex::parse_lenient("totally_not_a_field", None);
  assert!(matches!(r, Err(ParseLenientError::NoMatch { .. })));
}

#[test]
fn test_parse_lenient_with_type_prefix() {
  use crate::prelude::*;
  let v = NasIndex::parse_lenient("dof:tx", None).unwrap();
  assert!(matches!(v, NasIndex::Dof(_)));
}

#[test]
fn test_legal_values_for_block() {
  use crate::prelude::*;
  let bf = BlockType::BarForces.col_index_legal_values().unwrap();
  assert!(bf.iter().any(|v| v == "axial_force"));
  assert!(bf.iter().any(|v| v == "torque"));
}

#[test]
fn test_round_trip_every_index_type() {
  use crate::prelude::*;
  for parser in NasIndex::parsers().into_iter() {
    if let Some(values) = parser.legal_values {
      for v in values {
        let parsed = (parser.parse)(&v).unwrap_or_else(|e| {
          panic!("type {} could not parse \"{}\": {}", parser.type_key, v, e)
        });
        assert_eq!(parsed.type_name(), parser.type_name);
      }
    }
  }
}

#[test]
fn test_parse_compound_grid_point_force_origin() {
  use crate::prelude::*;
  use std::str::FromStr;
  let g = GridPointForceOrigin::from_str("load@42").unwrap();
  assert_eq!(g.grid_point.gid, 42);
  assert!(matches!(g.force_origin, ForceOrigin::Load));
  let s = GridPointForceOrigin::from_str("spc@7").unwrap();
  assert!(matches!(s.force_origin, ForceOrigin::SinglePointConstraint));
  let m = GridPointForceOrigin::from_str("mpc@7").unwrap();
  assert!(matches!(m.force_origin, ForceOrigin::MultiPointConstraint));
  let e = GridPointForceOrigin::from_str("elem_100@9").unwrap();
  match e.force_origin {
    ForceOrigin::Element { elem } => assert_eq!(elem.eid, 100),
    _ => panic!("expected element origin"),
  }
  assert!(GridPointForceOrigin::from_str("nope").is_err());
}

#[test]
fn test_parse_compound_point_in_element() {
  use crate::prelude::*;
  use std::str::FromStr;
  let p = PointInElement::from_str("100/centroid").unwrap();
  assert_eq!(p.element.eid, 100);
  assert!(matches!(p.point, ElementPoint::Centroid));
  let c = PointInElement::from_str("100/corner_42").unwrap();
  assert!(matches!(
    c.point,
    ElementPoint::Corner(GridPointRef { gid: 42 })
  ));
  let m = PointInElement::from_str("100/midpoint_7").unwrap();
  assert!(matches!(
    m.point,
    ElementPoint::Midpoint(GridPointRef { gid: 7 })
  ));
  let a = PointInElement::from_str("100/anywhere").unwrap();
  assert!(matches!(a.point, ElementPoint::Anywhere));
  assert!(PointInElement::from_str("nope").is_err());
}

#[test]
fn test_parse_compound_element_sided_point() {
  use crate::prelude::*;
  use std::str::FromStr;
  let p = ElementSidedPoint::from_str("100/centroid/top").unwrap();
  assert_eq!(p.element.eid, 100);
  assert!(matches!(p.point, ElementPoint::Centroid));
  assert!(matches!(p.side, ElementSide::Top));
  let q = ElementSidedPoint::from_str("100/corner_42/bottom").unwrap();
  assert!(matches!(
    q.point,
    ElementPoint::Corner(GridPointRef { gid: 42 })
  ));
  assert!(matches!(q.side, ElementSide::Bottom));
  assert!(ElementSidedPoint::from_str("100/centroid").is_err());
}
