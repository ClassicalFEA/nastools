use crate::util::{decode_nasfloat, unspace};

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
