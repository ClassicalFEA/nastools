//! This module implements utility functions without much need for defining
//! context or not enough of it to warrant them having their own modules.

use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::elements::ElementType;

/// Words that we can find in a spaced block header to make us suspicious.
pub(crate) const SUS_WORDS: &[&str] = &[
  "ELEMENT",
  "ELEM",
  "FORCE",
  "FORCES",
  "STRESS",
  "STRESSES",
  "STRAIN",
  "STRAINS",
  "SPC",
  "CONSTRAINT",
  "CONSTRAINTS",
  "MPC",
  "GRID",
  "DISPLACEMENT",
  "APPLIED",
  "LOAD",
  "TEMPERATURE",
  "HEAT",
  "FLUX",
  "GRAVITY",
  "GRID",
  "POINT",
  "COORDINATE",
  "COORD",
  "SYSTEM",
  "LOCAL",
  "EIGENVECTOR",
  "EIGENVALUES",
];

/// Words that make us ignore a block because it's definitely not gonna be
/// supported.
pub(crate) const BAD_WORDS: &[&str] =
  &["NASTRAN", "CONTROL", "BULK", "ECHO", "NODAL", "GENERATOR"];

/// Decodes a Nastran-format floating point number. Hyper-lenient and doesn't
/// require pulling a whole regex library.
pub(crate) fn decode_nasfloat(s: &str) -> Option<f64> {
  // mantissa start/end, exponent start/end
  let mut ixs: [usize; 4] = [0, 0, 0, 0];
  // 0-1 = looking for mantissa start/end, 2-3 = looking for exponent start/end
  let step: Cell<usize> = 0.into();
  let mut mark = |i| {
    ixs[step.get()] = i;
    step.replace(step.get() + 1);
  };
  let mut seen_chars: usize = 0;
  for (i, c) in s.chars().enumerate() {
    seen_chars += 1;
    match (
      step.get() % 2,
      c.is_numeric() || c == '.',
      c == '+' || c == '-',
    ) {
      // looking for number start. nothing yet. keep looking.
      (0, false, false) => continue,
      // looking for number start, found something. mark it and look for end.
      (0, _, _) => mark(i),
      // looking for number end, saw number/dot. keep looking.
      (1, true, _) => continue,
      // looking for number end, saw not numerical/dot/sign. mark end.
      (1, false, false) => mark(i),
      // looking for number end, saw sign. mark end, mark start.
      (1, _, true) => {
        mark(i);
        mark(i);
      }
      // should be unreachable
      _ => panic!("unreachable branch 1 in decoding nasfloat \"{s}\""),
    };
    if step.get() > 3 {
      break;
    }
  }
  // handle empty string
  if seen_chars == 0 {
    return None;
  }
  // handle end at end-of-string
  if step.get() % 2 == 1 {
    mark(seen_chars);
  }
  let mantissa = || s[ixs[0]..ixs[1]].parse::<f64>().ok();
  let exponent = || s[ixs[2]..ixs[3]].parse::<i32>().ok();
  match step.get() {
    // never found mantissa
    0 => return None,
    // only found a mantissa
    2 => return mantissa(),
    // found mantissa and exponent
    4 => return Some(mantissa()? * 10.0_f64.powi(exponent()?)),
    // should be unreachable
    _ => panic!("unreachable branch 2 in returning nasfloat \"{s}\""),
  };
}

/// A line field as decoded.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub(crate) enum LineField<'s> {
  /// Managed to parse an integer out of it.
  Integer(isize),
  /// Managed to parse a real out of it.
  Real(f64),
  /// Field is a single character.
  Character(char),
  /// Field is an element type.
  ElementType(ElementType),
  /// Couldn't parse it.
  NoIdea(&'s str),
}

impl<'s> LineField<'s> {
  /// Parses a single field into a LineField.
  fn parse(s: &'s str) -> Self {
    if let Ok(i) = s.parse::<isize>() {
      return Self::Integer(i);
    }
    if let Ok(x) = s.parse::<f64>()
    /*.or(decode_nasfloat(s))*/
    {
      return Self::Real(x);
    }
    if s.len() == 1 {
      return Self::Character(s.chars().nth(0).unwrap());
    }
    for cand in ElementType::all() {
      if s.contains(cand.name()) {
        return Self::ElementType(*cand);
      }
    }
    return Self::NoIdea(s);
  }
}

/// Breaks down a line into an iterator of fields.
pub(crate) fn line_breakdown(s: &str) -> impl Iterator<Item = LineField<'_>> {
  return s
    .split(' ')
    .filter(|subs| !subs.is_empty())
    .map(LineField::parse);
}

/// Gets a certain number of reals from a line.
pub(crate) fn extract_reals<const N: usize>(line: &str) -> Option<[f64; N]> {
  let mut arr: [f64; N] = [0.0; N];
  let mut found = 0;
  for field in line_breakdown(line) {
    match field {
      LineField::Real(x) if found < N => {
        arr[found] = x;
        found += 1;
      }
      LineField::Real(_) if found == N => {
        return None;
      }
      _ => continue,
    }
  }
  if found == N {
    return Some(arr);
  } else {
    return None;
  }
}

/// Gets a certain number of reals from a line, but ignores extras.
pub(crate) fn lax_reals<const N: usize>(line: &str) -> Option<[f64; N]> {
  let mut arr: [f64; N] = [0.0; N];
  let mut found = 0;
  for field in line_breakdown(line) {
    match field {
      LineField::Real(x) if found < N => {
        arr[found] = x;
        found += 1;
      }
      _ => continue,
    }
  }
  if found == N {
    return Some(arr);
  } else {
    return None;
  }
}

/// Gets the N-th integer in a line.
pub(crate) fn nth_integer(line: &str, n: usize) -> Option<isize> {
  return line_breakdown(line)
    .filter_map(|field| {
      if let LineField::Integer(x) = field {
        return Some(x);
      } else {
        None
      }
    })
    .nth(n);
}

/// Returns the n-th integer in a line and casts it to a usize.
pub(crate) fn nth_natural(line: &str, n: usize) -> Option<usize> {
  return line_breakdown(line)
    .filter_map(|field| {
      if let LineField::Integer(x) = field {
        return x.try_into().ok();
      } else {
        None
      }
    })
    .nth(n);
}

/// Gets the N-th string in a line.
pub(crate) fn nth_string(line: &str, n: usize) -> Option<&str> {
  return line_breakdown(line)
    .filter_map(|field| {
      if let LineField::NoIdea(s) = field {
        return Some(s);
      } else {
        None
      }
    })
    .nth(n);
}

/// Gets the N-th element type in a line.
pub(crate) fn nth_etype(line: &str, n: usize) -> Option<ElementType> {
  return line_breakdown(line)
    .filter_map(|field| {
      if let LineField::ElementType(etype) = field {
        return Some(etype);
      } else {
        None
      }
    })
    .nth(n);
}

/// Extracts all forms given by integers followed by some floats in a line.
/// Ignores all other fields. Useful for some kinds of tables.
pub(crate) fn int_pattern(line: &str) -> BTreeMap<usize, Vec<f64>> {
  let mut res: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
  let mut current_nat: Option<(usize, Vec<f64>)> = None;
  let flush = |r: &mut BTreeMap<usize, Vec<f64>>,
               cur: &mut Option<(usize, Vec<f64>)>| {
    if let Some((i, v)) = cur.take() {
      r.insert(i, v);
    };
  };
  for field in line_breakdown(line) {
    match field {
      LineField::Integer(i) => {
        flush(&mut res, &mut current_nat);
        current_nat = Some((i as usize, Vec::new()));
      }
      LineField::Real(x) => {
        if let Some((_, ref mut v)) = current_nat {
          v.push(x);
        }
      }
      _ => flush(&mut res, &mut current_nat),
    };
  }
  flush(&mut res, &mut current_nat);
  return res;
}

/// Returns the last integer in a line.
pub(crate) fn last_int(line: &str) -> Option<isize> {
  return line_breakdown(line)
    .filter_map(|f| {
      if let LineField::Integer(i) = f {
        Some(i)
      } else {
        None
      }
    })
    .last();
}

/// Returns the last natural in a line.
pub(crate) fn last_natural(line: &str) -> Option<usize> {
  return last_int(line).and_then(|i| i.try_into().ok());
}

/// Checks if a character is an uppercase letter or a digit.
fn upper_or_digit_or_special(ch: char) -> bool {
  /// Allowed special characters in a spaced header line.
  const SPEC: &str = "()[]-.";
  return ch.is_ascii_uppercase() || ch.is_ascii_digit() || SPEC.contains(ch);
}

/// Turns a line made of spaced upper-case ASCII into a line of upper-case
/// words, used for detecting block headers.
pub(crate) fn unspace(line: &str) -> Option<String> {
  let mut cap: usize = 0;
  let mut last: char = ' ';
  let mut stop_at: usize = 0;

  // special case for SC NASTRAN eigen solutions
  let line = if line
    .split_ascii_whitespace()
    .next()
    .is_some_and(|w| w == "CYCLES")
  {
    &line[(line.find("R")? - 1)..]
  } else {
    line
  };

  for ch in line.chars() {
    stop_at += 1;
    if upper_or_digit_or_special(ch) {
      if last == ' ' {
        last = ch;
        cap += 2;
        continue;
      } else {
        // not spaced. but have we seen a lot?
        if cap > 20 {
          // we've seen enough, this is fine. drop the extra chars tho
          stop_at = stop_at.saturating_sub(2);
          break;
        } else {
          // nah, we've seen it too soon.
          return None;
        }
      }
    }
    if ch == ' ' {
      last = ch;
      continue;
    }
    // bad char
    return None;
  }
  if cap < 4 {
    // too small
    return None;
  }
  let mut sb = String::with_capacity(cap);
  let mut space_run: usize = 0;
  let mut started = false;
  for (i, ch) in line.chars().enumerate() {
    if i == stop_at {
      break;
    }
    if ch == ' ' {
      space_run += 1;
    }
    if space_run > 3 {
      started = true;
    }
    if !started {
      continue;
    }
    if upper_or_digit_or_special(ch) {
      if (2..15).contains(&space_run) {
        sb.push(' ');
      }
      sb.push(ch);
      space_run = 0;
    }
  }
  return Some(sb.trim().to_string());
}

/// Checks if a line is a likely block header.
pub(crate) fn check_header(line: &str) -> Option<String> {
  // unspace it
  let unspaced = unspace(line)?;
  // check for sus words
  if SUS_WORDS.iter().any(|w| unspaced.contains(w)) {
    return Some(unspaced);
  }
  // check for element type names
  if ElementType::all()
    .iter()
    .any(|et| unspaced.contains(et.name()))
  {
    return Some(unspaced);
  }
  return None;
}

use std::cmp::Ordering;

/// This contains a potential header.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PotentialHeader {
  /// Starting line.
  pub start: usize,
  /// Number of lines this takes up.
  pub span: usize,
  /// The unspaced text.
  pub text: String,
}

impl AsRef<str> for PotentialHeader {
  fn as_ref(&self) -> &str {
    return self.text.as_str();
  }
}

impl PartialEq for PotentialHeader {
  fn eq(&self, other: &Self) -> bool {
    return self.start == other.start;
  }
}

impl Eq for PotentialHeader {}

impl PartialOrd for PotentialHeader {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for PotentialHeader {
  fn cmp(&self, other: &Self) -> Ordering {
    return self.start.cmp(&other.start);
  }
}

impl PotentialHeader {
  /// Returns the range of lines.
  pub fn lines(&self) -> impl Iterator<Item = usize> {
    return self.start..(self.start + self.span);
  }

  /// Merges this potential header with another, if possible.
  pub fn try_merge(self, other: Self) -> Result<Self, (Self, Self)> {
    // put them in order
    let (mut first, second) = if self.start <= other.start {
      (self, other)
    } else {
      (other, self)
    };
    // check if the ranges work glued together
    if first.lines().last().unwrap() == (second.lines().nth(0).unwrap() - 1) {
      first.text.push(' ');
      first.text.push_str(&second.text);
      first.span += second.span;
      return Ok(first);
    }
    return Err((first, second));
  }
}

/// Custom float formatting, stolen from StackOverflow but changed to use an
/// actual formatter and some other small things.
pub fn fmt_f64<W: Write>(
  f: &mut W,
  num: f64,
  width: usize,
  precision: usize,
  exp_pad: usize,
  capital_e: bool,
  omit_plus: bool,
) -> std::fmt::Result {
  let mut num = if omit_plus {
    format!("{num:.precision$e}")
  } else {
    format!("{num:+.precision$e}")
  };
  // safe to `unwrap` as `num` is guaranteed to contain `'e'`
  let exp = num.split_off(num.find('e').unwrap());
  /* removed due to clippy warning
  let (sign, exp) = if exp.starts_with("e-") {
    ('-', &exp[2..])
  } else {
    ('+', &exp[1..])
  };*/
  let (sign, exp) = if let Some(spd) = exp.strip_prefix("e-") {
    ('-', spd)
  } else {
    ('+', &exp[1..])
  };
  let e = if capital_e { 'E' } else { 'e' };
  num.push_str(&format!("{e}{sign}{exp:0>exp_pad$}"));

  return write!(f, "{num:>width$}");
}
