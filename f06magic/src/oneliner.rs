//! "One-liner" check mode for f06magic.
//!
//! Lets the user specify a single PASS/FAIL check on a single F06 file
//! through one quoted CLI string instead of authoring a TOML script. Useful
//! as a CI primitive.
//!
//! ## Spec format
//!
//! `subcase <N> <block> <row> <col> <A> <to|delta> <B>` (8 tokens)
//!
//! - `<A> to <B>`: inclusive range `[min(A,B), max(A,B)]`.
//! - `<A> delta <B>`: inclusive symmetric range `[A - B, A + B]`; `B` must be
//!   non-negative.
//!
//! On success the value is looked up by reusing the same machinery that
//! powers TOML scripts ([`SimpleExtraction::resolve`] +
//! [`Extraction::lookup`] + [`DatumIndex::get_from`]).

use std::error::Error;
use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;

use f06::prelude::*;

use crate::script::errors::ScriptValidationError;
use crate::script::extraction::SimpleExtraction;
use crate::script::index::LenientNasIndex;
use crate::utils::{AnyAmount, NumListRange};

/// The result of running a one-liner.
#[derive(Debug, Copy, Clone)]
pub(crate) enum OnelinerOutcome {
  /// The value was extracted and lies within the bounds.
  Pass,
  /// The value was extracted and lies outside the bounds (or is NaN).
  Fail,
}

/// Bounds half of a one-liner spec.
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum Bounds {
  /// `<A> to <B>`, normalised so `lo <= hi`.
  Range {
    /// Lower bound, inclusive.
    lo: f64,
    /// Upper bound, inclusive.
    hi: f64,
  },
  /// `<A> delta <B>`. `tol` is non-negative.
  Delta {
    /// Centre value.
    center: f64,
    /// Tolerance (non-negative).
    tol: f64,
  },
}

impl Bounds {
  /// Returns true iff `value` lies within this bound. Always false for NaN.
  pub(crate) fn contains(&self, value: f64) -> bool {
    if value.is_nan() {
      return false;
    }
    return match *self {
      Bounds::Range { lo, hi } => lo <= value && value <= hi,
      Bounds::Delta { center, tol } => (value - center).abs() <= tol,
    };
  }
}

/// A parsed one-liner spec, ready to be resolved against an F06 file.
#[derive(Debug, Clone)]
pub(crate) struct OnelinerSpec {
  /// Subcase number.
  pub(crate) subcase: usize,
  /// Block type.
  pub(crate) block: BlockType,
  /// Raw row token (resolved against `block` later).
  pub(crate) row: String,
  /// Raw column token (resolved against `block` later).
  pub(crate) col: String,
  /// Bounds.
  pub(crate) bounds: Bounds,
}

/// Errors raised when parsing or running a one-liner spec.
#[derive(Debug)]
pub(crate) enum OnelinerError {
  /// The spec did not contain exactly eight whitespace-separated tokens.
  BadTokenCount(usize),
  /// The first token was not the literal word `subcase`.
  BadSubcaseLiteral(String),
  /// The subcase number could not be parsed.
  BadSubcase(String),
  /// The block name could not be parsed.
  BadBlock(String, String),
  /// The bounds operator (token 7) was not `to` or `delta`.
  BadOperator(String),
  /// `delta` was given a negative tolerance.
  NegativeDelta(f64),
  /// One of the numeric bounds was NaN.
  NanBound,
  /// One of the numeric bounds could not be parsed as f64.
  BadBound(String, String),
  /// Validation of the row/col against the block's index types failed.
  Validation(ScriptValidationError),
  /// The F06 file could not be parsed.
  Parser(ParserCrash),
  /// The lookup matched a single index but could not be read.
  Extraction(ExtractionError),
  /// No (subcase, block, row, col) tuple matched in the file.
  NoMatch {
    /// Subcase that was searched.
    subcase: usize,
    /// Block type that was searched.
    block: BlockType,
    /// Raw row token.
    row: String,
    /// Raw column token.
    col: String,
  },
  /// More than one (subcase, block, row, col) tuple matched.
  Ambiguous(usize),
}

impl Display for OnelinerError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::BadTokenCount(n) => write!(
        f,
        "one-liner spec must have exactly 8 tokens (got {n}); \
         expected: subcase <N> <block> <row> <col> <A> <to|delta> <B>",
      ),
      Self::BadSubcaseLiteral(t) => write!(
        f,
        "one-liner spec must start with the literal \"subcase\", got \"{t}\"",
      ),
      Self::BadSubcase(t) => write!(f, "bad subcase number \"{t}\""),
      Self::BadBlock(t, why) => {
        write!(f, "bad block name \"{t}\": {why}")
      }
      Self::BadOperator(t) => write!(
        f,
        "bounds operator must be \"to\" or \"delta\", got \"{t}\"",
      ),
      Self::NegativeDelta(v) => {
        write!(f, "delta tolerance must be non-negative, got {v}")
      }
      Self::NanBound => write!(f, "numeric bounds cannot be NaN"),
      Self::BadBound(t, why) => write!(f, "bad numeric bound \"{t}\": {why}"),
      Self::Validation(e) => write!(f, "{e}"),
      Self::Parser(e) => write!(f, "could not parse F06: {e}"),
      Self::Extraction(e) => write!(f, "could not read value: {e}"),
      Self::NoMatch {
        subcase,
        block,
        row,
        col,
      } => write!(
        f,
        "no value matched subcase={subcase}, block={}, row=\"{row}\", \
         col=\"{col}\"",
        block.short_name(),
      ),
      Self::Ambiguous(n) => write!(
        f,
        "one-liner expects a single value but {n} matched; \
         narrow the spec",
      ),
    };
  }
}

impl Error for OnelinerError {}

/// Parses the one-liner spec string.
pub(crate) fn parse_oneliner(
  spec: &str,
) -> Result<OnelinerSpec, OnelinerError> {
  let tokens: Vec<&str> = spec.split_ascii_whitespace().collect();
  if tokens.len() != 8 {
    return Err(OnelinerError::BadTokenCount(tokens.len()));
  }
  if !tokens[0].eq_ignore_ascii_case("subcase") {
    return Err(OnelinerError::BadSubcaseLiteral(tokens[0].to_owned()));
  }
  let subcase: usize = tokens[1]
    .parse()
    .map_err(|_| OnelinerError::BadSubcase(tokens[1].to_owned()))?;
  let block = BlockType::from_str(tokens[2])
    .map_err(|why| OnelinerError::BadBlock(tokens[2].to_owned(), why))?;
  let row = tokens[3].to_owned();
  let col = tokens[4].to_owned();
  let parse_f64 = |s: &str| -> Result<f64, OnelinerError> {
    let v: f64 = s.parse().map_err(|e: std::num::ParseFloatError| {
      OnelinerError::BadBound(s.to_owned(), e.to_string())
    })?;
    if v.is_nan() {
      return Err(OnelinerError::NanBound);
    }
    return Ok(v);
  };
  let a = parse_f64(tokens[5])?;
  let b = parse_f64(tokens[7])?;
  let bounds = if tokens[6].eq_ignore_ascii_case("to") {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    Bounds::Range { lo, hi }
  } else if tokens[6].eq_ignore_ascii_case("delta") {
    if b < 0.0 {
      return Err(OnelinerError::NegativeDelta(b));
    }
    Bounds::Delta { center: a, tol: b }
  } else {
    return Err(OnelinerError::BadOperator(tokens[6].to_owned()));
  };
  return Ok(OnelinerSpec {
    subcase,
    block,
    row,
    col,
    bounds,
  });
}

/// Builds a single-value [`SimpleExtraction`] from a one-liner spec, with the
/// caller-chosen row/col tokens.
fn make_extraction(
  spec: &OnelinerSpec,
  row: &str,
  col: &str,
) -> SimpleExtraction {
  return SimpleExtraction {
    name: "oneliner".to_owned(),
    blocks: AnyAmount::One(spec.block),
    subcases: NumListRange::Single(spec.subcase),
    nodes: NumListRange::None,
    elements: NumListRange::None,
    element_types: AnyAmount::None,
    cols: AnyAmount::One(LenientNasIndex {
      raw: col.to_owned(),
    }),
    rows: AnyAmount::One(LenientNasIndex {
      raw: row.to_owned(),
    }),
    raw_cols: AnyAmount::None,
    raw_rows: AnyAmount::None,
  };
}

/// Resolves a one-liner spec into a real [`Extraction`].
///
/// On any row/col validation failure we transparently retry once with the
/// row and column tokens swapped, so the user does not have to memorise
/// per-block orientation. If the retry also fails we surface the *original*
/// error so the diagnostic reflects what the user actually typed.
fn resolve_extraction(
  spec: &OnelinerSpec,
) -> Result<Extraction, OnelinerError> {
  let first = make_extraction(spec, &spec.row, &spec.col).resolve();
  return match first {
    Ok(e) => Ok(e),
    Err(err) => match make_extraction(spec, &spec.col, &spec.row).resolve() {
      Ok(e) => Ok(e),
      Err(_) => Err(OnelinerError::Validation(err)),
    },
  };
}

/// Runs a parsed one-liner against an F06 file at `path`.
///
/// Returns `Ok(Pass)` / `Ok(Fail)` on a successful extraction. The resolved
/// numeric value is written to `stderr_value` for the caller to print on
/// stderr; on PASS/FAIL the caller is expected to print exactly one of
/// `PASS` / `FAIL` to stdout.
pub(crate) fn run_oneliner<P: AsRef<Path>>(
  spec: &OnelinerSpec,
  path: P,
) -> Result<(OnelinerOutcome, F06Number), OnelinerError> {
  let extraction = resolve_extraction(spec)?;
  let f06 = OnePassParser::parse_file(path).map_err(OnelinerError::Parser)?;
  let mut hits = extraction.lookup(&f06);
  let first = hits.next().ok_or_else(|| OnelinerError::NoMatch {
    subcase: spec.subcase,
    block: spec.block,
    row: spec.row.clone(),
    col: spec.col.clone(),
  })?;
  // Count the rest to detect ambiguity, but cap so a pathological extraction
  // does not iterate forever in degenerate cases.
  let extra = hits.take(64).count();
  if extra > 0 {
    return Err(OnelinerError::Ambiguous(1 + extra));
  }
  let value = first.get_from(&f06).map_err(OnelinerError::Extraction)?;
  let outcome = if spec.bounds.contains(value.into()) {
    OnelinerOutcome::Pass
  } else {
    OnelinerOutcome::Fail
  };
  return Ok((outcome, value));
}

/// Exit code for an [`OnelinerError`], following the contract documented on
/// the `--oneliner` CLI flag.
pub(crate) fn error_exit_code(err: &OnelinerError) -> i32 {
  return match err {
    OnelinerError::BadTokenCount(_)
    | OnelinerError::BadSubcaseLiteral(_)
    | OnelinerError::BadSubcase(_)
    | OnelinerError::BadBlock(_, _)
    | OnelinerError::BadOperator(_)
    | OnelinerError::NegativeDelta(_)
    | OnelinerError::NanBound
    | OnelinerError::BadBound(_, _)
    | OnelinerError::Validation(_) => 3,
    OnelinerError::Parser(_) => 4,
    OnelinerError::Extraction(_)
    | OnelinerError::NoMatch { .. }
    | OnelinerError::Ambiguous(_) => 2,
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_range_form() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx -1.0 to 1.0").unwrap();
    assert_eq!(s.subcase, 1);
    assert_eq!(s.row, "grid_1");
    assert_eq!(s.col, "tx");
    assert_eq!(s.bounds, Bounds::Range { lo: -1.0, hi: 1.0 });
  }

  #[test]
  fn range_normalises_swapped_operands() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx 5.0 to -2.0").unwrap();
    assert_eq!(s.bounds, Bounds::Range { lo: -2.0, hi: 5.0 });
  }

  #[test]
  fn parses_delta_form() {
    let s = parse_oneliner("subcase 2 displacements grid_3 ty 10.0 delta 0.5")
      .unwrap();
    assert_eq!(
      s.bounds,
      Bounds::Delta {
        center: 10.0,
        tol: 0.5,
      },
    );
  }

  #[test]
  fn case_insensitive_keywords() {
    let s = parse_oneliner("SUBCASE 1 Displacements grid_1 tx 0 TO 1").unwrap();
    assert!(matches!(s.bounds, Bounds::Range { .. }));
  }

  #[test]
  fn rejects_wrong_token_count() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 0 to"),
      Err(OnelinerError::BadTokenCount(7)),
    ));
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 0 to 1 extra"),
      Err(OnelinerError::BadTokenCount(9)),
    ));
  }

  #[test]
  fn rejects_missing_subcase_literal() {
    assert!(matches!(
      parse_oneliner("foo 1 displacements grid_1 tx 0 to 1"),
      Err(OnelinerError::BadSubcaseLiteral(_)),
    ));
  }

  #[test]
  fn rejects_bad_subcase_number() {
    assert!(matches!(
      parse_oneliner("subcase x displacements grid_1 tx 0 to 1"),
      Err(OnelinerError::BadSubcase(_)),
    ));
  }

  #[test]
  fn rejects_unknown_block() {
    assert!(matches!(
      parse_oneliner("subcase 1 not_a_block grid_1 tx 0 to 1"),
      Err(OnelinerError::BadBlock(_, _)),
    ));
  }

  #[test]
  fn rejects_bad_operator() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 0 between 1"),
      Err(OnelinerError::BadOperator(_)),
    ));
  }

  #[test]
  fn rejects_negative_delta() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 0 delta -1"),
      Err(OnelinerError::NegativeDelta(_)),
    ));
  }

  #[test]
  fn rejects_nan_bound() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx NaN to 1"),
      Err(OnelinerError::NanBound),
    ));
  }

  #[test]
  fn rejects_bad_bound() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx zero to 1"),
      Err(OnelinerError::BadBound(_, _)),
    ));
  }

  #[test]
  fn bounds_contains() {
    let r = Bounds::Range { lo: 0.0, hi: 1.0 };
    assert!(r.contains(0.0));
    assert!(r.contains(0.5));
    assert!(r.contains(1.0));
    assert!(!r.contains(-0.001));
    assert!(!r.contains(1.001));
    assert!(!r.contains(f64::NAN));
    let d = Bounds::Delta {
      center: 10.0,
      tol: 0.5,
    };
    assert!(d.contains(9.5));
    assert!(d.contains(10.5));
    assert!(!d.contains(10.501));
    assert!(!d.contains(f64::NAN));
  }
}
