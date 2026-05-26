//! "One-liner" check mode for f06magic.
//!
//! Lets the user specify a single PASS/FAIL check on an F06 file through
//! one quoted CLI string instead of authoring a TOML script. Useful as a
//! CI primitive.
//!
//! ## Spec format
//!
//! 8-token forms:
//!
//! ```text
//! subcase <N> <block> <row> <col> <A> to     <B>
//! subcase <N> <block> <row> <col> <A> delta  <B>
//! subcase <N> <block> <row> <col> <A> percent <P>   (alias: pct)
//! subcase <N> <block> <row> <col> <A> ±      <B>    (alias: +-)
//! subcase <N> <block> <row> <col> <A> ±      <P>%   (alias: +-)
//! ```
//!
//! 10-token form (`percent`/`pct` or `± ...%` with optional near-zero floor):
//!
//! ```text
//! subcase <N> <block> <row> <col> <A> percent <P> floor <E>
//! subcase <N> <block> <row> <col> <A> ±       <P>% floor <E>
//! ```
//!
//! Variable-length `satisfies` form (everything after `satisfies` is the
//! expression):
//!
//! ```text
//! subcase <N> <block> <row> <col> satisfies <equation ...>
//! ```
//!
//! - `<A> to <B>`: inclusive range `[min(A,B), max(A,B)]`.
//! - `<A> delta <B>`, `<A> ± <B>`: inclusive symmetric range
//!   `[A - B, A + B]`; `B >= 0`.
//! - `<A> percent <P>`, `<A> ± <P>%`: PASS iff `100*|test/A - 1| <= P`. `A`
//!   is the reference value (so e.g. `... 10 percent 5` matches
//!   `[9.5, 10.5]`). With `... floor <E>` appended: when both `|A|` and
//!   `|test|` are below `E` the check passes; when exactly one is below it
//!   fails; otherwise the percent formula applies. With no floor and
//!   `A == 0` the check fails unless `test == 0`.
//! - `satisfies <expr>`: the cell's value is bound to `x` / `t` in the
//!   expression; per-cell PASS iff the expression evaluates to a non-zero,
//!   non-NaN value. Magic stat variables (`min`, `max`, `mina`, `maxa`,
//!   `avg`, `sum`, `std`, `stdp`, `stds`, `n`) are computed across the
//!   cartesian product. See the f06magic `script::equation` module for
//!   the full grammar.
//!
//! `<N>`, `<row>`, and `<col>` may be comma-separated lists (no spaces);
//! every combination in the cartesian product is evaluated. The numeric
//! bounds (`A`, `B`, `P`, `E`) are always single values.
//!
//! On success the value is looked up by reusing the same machinery that
//! powers TOML scripts ([`SimpleExtraction::resolve`] +
//! [`Extraction::lookup`] + [`DatumIndex::get_from`]).

use std::error::Error;
use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;

use f06::prelude::*;

use crate::script::equation::{
  Equation, EquationError, EvalOutcome, Scope, Stats,
};
use crate::script::errors::ScriptValidationError;
use crate::script::extraction::SimpleExtraction;
use crate::script::index::LenientNasIndex;
use crate::utils::{AnyAmount, NumListRange};

/// The result of running a single one-liner cell.
#[derive(Debug, Copy, Clone)]
pub(crate) enum OnelinerOutcome {
  /// The value was extracted and lies within the bounds.
  Pass,
  /// The value was extracted and lies outside the bounds (or is NaN).
  Fail,
}

/// Bounds half of a one-liner spec.
#[derive(Debug)]
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
  /// `<A> percent <P>` with optional `floor <E>`. PASS iff
  /// `100*|test/center - 1| <= tol_pct`, with the same three-case floor
  /// logic as the libf06 `Criteria` percent check.
  Percent {
    /// Reference value (token `A`).
    center: f64,
    /// Percent tolerance (non-negative).
    tol_pct: f64,
    /// Optional near-zero floor (non-negative when `Some`).
    floor: Option<f64>,
  },
  /// `satisfies <expression>`. The cell's value is bound to `x`/`t` in the
  /// equation; magic stats (`min`, `max`, `avg`, `std`, `stdp`, `stds`,
  /// `sum`, `n`) are computed across the cartesian product. PASS iff the
  /// equation evaluates to a non-zero, non-NaN value.
  Satisfies(Box<Equation>),
}

impl PartialEq for Bounds {
  fn eq(&self, other: &Self) -> bool {
    return match (self, other) {
      (Bounds::Range { lo: a, hi: b }, Bounds::Range { lo: c, hi: d }) => {
        a == c && b == d
      }
      (
        Bounds::Delta { center: a, tol: b },
        Bounds::Delta { center: c, tol: d },
      ) => a == c && b == d,
      (
        Bounds::Percent {
          center: a,
          tol_pct: b,
          floor: c,
        },
        Bounds::Percent {
          center: d,
          tol_pct: e,
          floor: f,
        },
      ) => a == d && b == e && c == f,
      // Satisfies variants are never compared via `==` (they carry an
      // owned parser arena that has no meaningful equality).
      _ => false,
    };
  }
}

impl Bounds {
  /// Returns true iff `value` lies within this bound. Always false for NaN.
  ///
  /// Not valid for [`Bounds::Satisfies`]; the caller (`run_oneliner`)
  /// dispatches that variant through a separate two-pass path.
  pub(crate) fn contains(&self, value: f64) -> bool {
    if value.is_nan() {
      return false;
    }
    return match self {
      Bounds::Range { lo, hi } => *lo <= value && value <= *hi,
      Bounds::Delta { center, tol } => (value - *center).abs() <= *tol,
      Bounds::Percent {
        center,
        tol_pct,
        floor,
      } => percent_pass(*center, value, *tol_pct, *floor),
      Bounds::Satisfies(_) => {
        debug_assert!(
          false,
          "Bounds::Satisfies must be dispatched through run_oneliner",
        );
        false
      }
    };
  }
}

/// Implements the three-case percent + floor logic. `center` is the
/// reference, `value` is the test value. Mirrors `Criteria::check`.
fn percent_pass(
  center: f64,
  value: f64,
  tol_pct: f64,
  floor: Option<f64>,
) -> bool {
  let e = floor.unwrap_or(0.0);
  let a_small = center.abs() < e;
  let b_small = value.abs() < e;
  if a_small && b_small {
    return true;
  }
  if a_small != b_small {
    return false;
  }
  let pct = if center == 0.0 {
    if value == 0.0 { 0.0 } else { f64::INFINITY }
  } else {
    100.0 * (value / center - 1.0).abs()
  };
  return pct <= tol_pct;
}

/// A parsed one-liner spec, ready to be resolved against an F06 file.
#[derive(Debug)]
pub(crate) struct OnelinerSpec {
  /// Subcase numbers (cartesian-expanded).
  pub(crate) subcases: Vec<usize>,
  /// Block type.
  pub(crate) block: BlockType,
  /// Raw row tokens (cartesian-expanded; resolved per cell).
  pub(crate) rows: Vec<String>,
  /// Raw column tokens (cartesian-expanded; resolved per cell).
  pub(crate) cols: Vec<String>,
  /// Bounds.
  pub(crate) bounds: Bounds,
}

impl OnelinerSpec {
  /// Total number of (subcase, row, col) cells implied by this spec.
  pub(crate) fn cell_count(&self) -> usize {
    return self.subcases.len() * self.rows.len() * self.cols.len();
  }
}

/// One cell of a one-liner run.
#[derive(Debug)]
pub(crate) struct CellResult {
  /// Subcase for this cell.
  pub(crate) subcase: usize,
  /// Block type (same across all cells).
  pub(crate) block: BlockType,
  /// Raw row token for this cell.
  pub(crate) row: String,
  /// Raw column token for this cell.
  pub(crate) col: String,
  /// What happened.
  pub(crate) outcome: CellOutcome,
}

/// Per-cell outcome of a one-liner run.
#[derive(Debug)]
pub(crate) enum CellOutcome {
  /// Value resolved and bounds satisfied.
  Pass(F06Number),
  /// Value resolved but bounds violated (or value is NaN).
  Fail(F06Number),
  /// The cell could not be resolved/read at all.
  Error(OnelinerError),
}

/// Errors raised when parsing or running a one-liner spec.
#[derive(Debug)]
pub(crate) enum OnelinerError {
  /// The spec did not contain a supported number of tokens.
  BadTokenCount(usize),
  /// The first token was not the literal word `subcase`.
  BadSubcaseLiteral(String),
  /// The subcase number could not be parsed.
  BadSubcase(String),
  /// The block name could not be parsed.
  BadBlock(String, String),
  /// The bounds operator (token 7) was not `to`, `delta`, `percent`, or `pct`.
  BadOperator(String),
  /// `delta` was given a negative tolerance.
  NegativeDelta(f64),
  /// `percent` was given a negative tolerance.
  NegativePercent(f64),
  /// `floor` was given a negative epsilon.
  NegativeFloor(f64),
  /// One of the numeric bounds was NaN.
  NanBound,
  /// One of the numeric bounds could not be parsed as f64.
  BadBound(String, String),
  /// Token 9 (in a 10-token spec) was not the literal `floor`.
  BadFloorLiteral(String),
  /// `floor` was used with an operator other than `percent`/`pct`.
  FloorWithoutPercent(String),
  /// One of the comma-separated lists was empty (e.g. "1,,2").
  EmptyListEntry {
    /// Which token had the empty entry.
    field: &'static str,
  },
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
  /// The `satisfies` expression failed to parse (or referenced an
  /// out-of-scope variable).
  EquationParse(EquationError),
  /// `satisfies` was used but every cell either failed to resolve or
  /// produced a non-finite value, leaving no values for stats.
  EmptyEquationPool,
}

impl Display for OnelinerError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::BadTokenCount(n) => write!(
        f,
        "one-liner spec must have 8 or 10 tokens (got {n}); expected:\n  \
         subcase <N> <block> <row> <col> <A> <to|delta|percent|pct> <B>\n  \
         subcase <N> <block> <row> <col> <A> <percent|pct> <P> floor <E>\n  \
         subcase <N> <block> <row> <col> satisfies <equation ...>",
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
        "bounds operator must be \"to\", \"delta\", \"percent\", \"pct\", \
         \"\u{00b1}\" or \"+-\", got \"{t}\"",
      ),
      Self::NegativeDelta(v) => {
        write!(f, "delta tolerance must be non-negative, got {v}")
      }
      Self::NegativePercent(v) => {
        write!(f, "percent tolerance must be non-negative, got {v}")
      }
      Self::NegativeFloor(v) => {
        write!(f, "floor must be non-negative, got {v}")
      }
      Self::NanBound => write!(f, "numeric bounds cannot be NaN"),
      Self::BadBound(t, why) => write!(f, "bad numeric bound \"{t}\": {why}"),
      Self::BadFloorLiteral(t) => write!(
        f,
        "expected the literal \"floor\" before the epsilon, got \"{t}\"",
      ),
      Self::FloorWithoutPercent(op) => write!(
        f,
        "\"floor\" only applies to \"percent\"/\"pct\" (operator was \"{op}\")",
      ),
      Self::EmptyListEntry { field } => {
        write!(f, "empty entry in comma-separated {field} list")
      }
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
        "one-liner expects a single value per cell but {n} matched; \
         narrow the spec",
      ),
      Self::EquationParse(e) => write!(f, "{e}"),
      Self::EmptyEquationPool => write!(
        f,
        "`satisfies` needs at least one cell to resolve to a finite value; \
         the pool was empty",
      ),
    };
  }
}

impl Error for OnelinerError {}

/// Splits a comma-separated token, trimming whitespace and rejecting empty
/// entries. A token with no commas yields a single-element vector.
fn split_csv(
  token: &str,
  field: &'static str,
) -> Result<Vec<String>, OnelinerError> {
  let mut out = Vec::new();
  for piece in token.split(',') {
    let trimmed = piece.trim();
    if trimmed.is_empty() {
      return Err(OnelinerError::EmptyListEntry { field });
    }
    out.push(trimmed.to_owned());
  }
  return Ok(out);
}

/// Parses the one-liner spec string.
pub(crate) fn parse_oneliner(
  spec: &str,
) -> Result<OnelinerSpec, OnelinerError> {
  let tokens: Vec<&str> = spec.split_ascii_whitespace().collect();
  if tokens.len() < 5 {
    return Err(OnelinerError::BadTokenCount(tokens.len()));
  }
  if !tokens[0].eq_ignore_ascii_case("subcase") {
    return Err(OnelinerError::BadSubcaseLiteral(tokens[0].to_owned()));
  }
  // Subcases (token 1): may be comma-separated.
  let subcase_pieces = split_csv(tokens[1], "subcase")?;
  let mut subcases: Vec<usize> = Vec::with_capacity(subcase_pieces.len());
  for piece in &subcase_pieces {
    let n: usize = piece
      .parse()
      .map_err(|_| OnelinerError::BadSubcase(piece.clone()))?;
    subcases.push(n);
  }
  let block = BlockType::from_str(tokens[2])
    .map_err(|why| OnelinerError::BadBlock(tokens[2].to_owned(), why))?;
  let rows = split_csv(tokens[3], "row")?;
  let cols = split_csv(tokens[4], "col")?;
  // Variable-length `satisfies` form: subcase N block row col satisfies <expr ...>.
  if tokens.len() >= 6 && tokens[5].eq_ignore_ascii_case("satisfies") {
    if tokens.len() < 7 {
      return Err(OnelinerError::BadTokenCount(tokens.len()));
    }
    let expr = tokens[6..].join(" ");
    let eq = Equation::parse(&expr, Scope::OnelinerCell)
      .map_err(OnelinerError::EquationParse)?;
    return Ok(OnelinerSpec {
      subcases,
      block,
      rows,
      cols,
      bounds: Bounds::Satisfies(Box::new(eq)),
    });
  }
  if tokens.len() != 8 && tokens.len() != 10 {
    return Err(OnelinerError::BadTokenCount(tokens.len()));
  }
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
  let op = tokens[6];
  let is_pm = op == "\u{00b1}" || op == "+-";
  let is_percent_kw =
    op.eq_ignore_ascii_case("percent") || op.eq_ignore_ascii_case("pct");
  // `±` picks delta vs percent by the trailing `%` on the B token. For
  // `percent`/`pct` the B token is bare. For other operators we use the
  // raw B token as-is and the trailing-% check is irrelevant.
  let (b_token, pm_is_percent) = if is_pm {
    match tokens[7].strip_suffix('%') {
      Some(rest) => (rest, true),
      None => (tokens[7], false),
    }
  } else {
    (tokens[7], false)
  };
  let b = parse_f64(b_token)?;
  let is_percent = is_percent_kw || pm_is_percent;
  // 10-token form is only valid for percent semantics.
  if tokens.len() == 10 && !is_percent {
    return Err(OnelinerError::FloorWithoutPercent(op.to_owned()));
  }
  let floor = if tokens.len() == 10 {
    if !tokens[8].eq_ignore_ascii_case("floor") {
      return Err(OnelinerError::BadFloorLiteral(tokens[8].to_owned()));
    }
    let e = parse_f64(tokens[9])?;
    if e < 0.0 {
      return Err(OnelinerError::NegativeFloor(e));
    }
    Some(e)
  } else {
    None
  };
  let bounds = if op.eq_ignore_ascii_case("to") {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    Bounds::Range { lo, hi }
  } else if op.eq_ignore_ascii_case("delta") || (is_pm && !pm_is_percent) {
    if b < 0.0 {
      return Err(OnelinerError::NegativeDelta(b));
    }
    Bounds::Delta { center: a, tol: b }
  } else if is_percent {
    if b < 0.0 {
      return Err(OnelinerError::NegativePercent(b));
    }
    Bounds::Percent {
      center: a,
      tol_pct: b,
      floor,
    }
  } else {
    return Err(OnelinerError::BadOperator(op.to_owned()));
  };
  return Ok(OnelinerSpec {
    subcases,
    block,
    rows,
    cols,
    bounds,
  });
}

/// Builds a single-value [`SimpleExtraction`] for a single (subcase, row,
/// col) cell.
fn make_extraction(
  block: BlockType,
  subcase: usize,
  row: &str,
  col: &str,
) -> SimpleExtraction {
  return SimpleExtraction {
    name: "oneliner".to_owned(),
    blocks: AnyAmount::One(block),
    subcases: NumListRange::Single(subcase),
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
    allow_empty: true,
  };
}

/// Resolves the row/col tokens for one cell into a real [`Extraction`].
///
/// On any row/col validation failure we transparently retry once with the
/// row and column tokens swapped, so the user does not have to memorise
/// per-block orientation. If the retry also fails we surface the *original*
/// error so the diagnostic reflects what the user actually typed.
fn resolve_cell_extraction(
  block: BlockType,
  subcase: usize,
  row: &str,
  col: &str,
) -> Result<Extraction, OnelinerError> {
  let first = make_extraction(block, subcase, row, col).resolve();
  return match first {
    Ok(e) => Ok(e),
    Err(err) => match make_extraction(block, subcase, col, row).resolve() {
      Ok(e) => Ok(e),
      Err(_) => Err(OnelinerError::Validation(err)),
    },
  };
}

/// Evaluates one (subcase, row, col) cell against an already-parsed F06.
fn run_cell(
  f06: &F06File,
  block: BlockType,
  subcase: usize,
  row: &str,
  col: &str,
  bounds: &Bounds,
) -> Result<(OnelinerOutcome, F06Number), OnelinerError> {
  let extraction = resolve_cell_extraction(block, subcase, row, col)?;
  let mut hits = extraction.lookup(f06);
  let first = hits.next().ok_or_else(|| OnelinerError::NoMatch {
    subcase,
    block,
    row: row.to_owned(),
    col: col.to_owned(),
  })?;
  // Count the rest to detect ambiguity, but cap so a pathological extraction
  // does not iterate forever in degenerate cases.
  let extra = hits.take(64).count();
  if extra > 0 {
    return Err(OnelinerError::Ambiguous(1 + extra));
  }
  let value = first.get_from(f06).map_err(OnelinerError::Extraction)?;
  let outcome = if bounds.contains(value.into()) {
    OnelinerOutcome::Pass
  } else {
    OnelinerOutcome::Fail
  };
  return Ok((outcome, value));
}

/// Runs a parsed one-liner against an F06 file at `path`.
///
/// Parses the F06 once, then iterates the cartesian product of
/// `subcases x rows x cols` and returns one [`CellResult`] per cell. Only
/// truly global errors (F06 parsing) propagate via `Err`; per-cell failures
/// to resolve/look up are captured in [`CellOutcome::Error`].
pub(crate) fn run_oneliner<P: AsRef<Path>>(
  spec: &OnelinerSpec,
  path: P,
) -> Result<Vec<CellResult>, OnelinerError> {
  let f06 = OnePassParser::parse_file(path).map_err(OnelinerError::Parser)?;
  if let Bounds::Satisfies(eq) = &spec.bounds {
    return Ok(run_oneliner_satisfies(&f06, spec, eq));
  }
  let mut results = Vec::with_capacity(spec.cell_count());
  for &subcase in &spec.subcases {
    for row in &spec.rows {
      for col in &spec.cols {
        let outcome =
          match run_cell(&f06, spec.block, subcase, row, col, &spec.bounds) {
            Ok((OnelinerOutcome::Pass, v)) => CellOutcome::Pass(v),
            Ok((OnelinerOutcome::Fail, v)) => CellOutcome::Fail(v),
            Err(e) => CellOutcome::Error(e),
          };
        results.push(CellResult {
          subcase,
          block: spec.block,
          row: row.clone(),
          col: col.clone(),
          outcome,
        });
      }
    }
  }
  return Ok(results);
}

/// Two-pass execution for `satisfies`: first resolve every cell to a value
/// (or per-cell error), then compute pool stats over the finite values and
/// evaluate the equation per cell.
fn run_oneliner_satisfies(
  f06: &F06File,
  spec: &OnelinerSpec,
  eq: &Equation,
) -> Vec<CellResult> {
  // Pass 1: resolve each cell to either a value or a per-cell error.
  struct Pending {
    subcase: usize,
    row: String,
    col: String,
    resolved: Result<F06Number, OnelinerError>,
  }
  let mut pending: Vec<Pending> = Vec::with_capacity(spec.cell_count());
  for &subcase in &spec.subcases {
    for row in &spec.rows {
      for col in &spec.cols {
        let resolved = resolve_cell_value(f06, spec.block, subcase, row, col);
        pending.push(Pending {
          subcase,
          row: row.clone(),
          col: col.clone(),
          resolved,
        });
      }
    }
  }
  // Compute pool stats over finite, successfully-resolved values.
  let pool = pending
    .iter()
    .filter_map(|p| p.resolved.as_ref().ok().map(|v| (*v).into()));
  let stats = Stats::from_values(pool);
  // Pass 2: evaluate the equation per cell.
  let mut results = Vec::with_capacity(pending.len());
  for p in pending {
    let outcome = match (p.resolved, &stats) {
      (Err(e), _) => CellOutcome::Error(e),
      (Ok(_), None) => CellOutcome::Error(OnelinerError::EmptyEquationPool),
      (Ok(v), Some(st)) => {
        let x: f64 = v.into();
        match eq.evaluate(x, None, st, None) {
          EvalOutcome::Pass { .. } => CellOutcome::Pass(v),
          EvalOutcome::Fail { .. } => CellOutcome::Fail(v),
          EvalOutcome::Error { message } => CellOutcome::Error(
            OnelinerError::EquationParse(EquationError::Parse {
              raw: eq.raw().to_owned(),
              message,
            }),
          ),
        }
      }
    };
    results.push(CellResult {
      subcase: p.subcase,
      block: spec.block,
      row: p.row,
      col: p.col,
      outcome,
    });
  }
  return results;
}

/// Resolves a single cell to its `F06Number` value or a per-cell error,
/// without applying any bounds. Used by the `satisfies` two-pass runner.
fn resolve_cell_value(
  f06: &F06File,
  block: BlockType,
  subcase: usize,
  row: &str,
  col: &str,
) -> Result<F06Number, OnelinerError> {
  let extraction = resolve_cell_extraction(block, subcase, row, col)?;
  let mut hits = extraction.lookup(f06);
  let first = hits.next().ok_or_else(|| OnelinerError::NoMatch {
    subcase,
    block,
    row: row.to_owned(),
    col: col.to_owned(),
  })?;
  let extra = hits.take(64).count();
  if extra > 0 {
    return Err(OnelinerError::Ambiguous(1 + extra));
  }
  return first.get_from(f06).map_err(OnelinerError::Extraction);
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
    | OnelinerError::NegativePercent(_)
    | OnelinerError::NegativeFloor(_)
    | OnelinerError::NanBound
    | OnelinerError::BadBound(_, _)
    | OnelinerError::BadFloorLiteral(_)
    | OnelinerError::FloorWithoutPercent(_)
    | OnelinerError::EmptyListEntry { .. }
    | OnelinerError::Validation(_)
    | OnelinerError::EquationParse(_) => 3,
    OnelinerError::Parser(_) => 4,
    OnelinerError::Extraction(_)
    | OnelinerError::NoMatch { .. }
    | OnelinerError::Ambiguous(_)
    | OnelinerError::EmptyEquationPool => 2,
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_range_form() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx -1.0 to 1.0").unwrap();
    assert_eq!(s.subcases, vec![1]);
    assert_eq!(s.rows, vec!["grid_1".to_owned()]);
    assert_eq!(s.cols, vec!["tx".to_owned()]);
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
    assert!(matches!(
      parse_oneliner(
        "subcase 1 displacements grid_1 tx 0 percent 5 floor 1e-6 extra",
      ),
      Err(OnelinerError::BadTokenCount(11)),
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

  #[test]
  fn parses_percent_form() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx 10 percent 5").unwrap();
    assert_eq!(
      s.bounds,
      Bounds::Percent {
        center: 10.0,
        tol_pct: 5.0,
        floor: None,
      },
    );
  }

  #[test]
  fn pct_is_alias_of_percent() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx 10 pct 5").unwrap();
    assert!(matches!(s.bounds, Bounds::Percent { floor: None, .. }));
  }

  #[test]
  fn parses_percent_with_floor() {
    let s = parse_oneliner(
      "subcase 1 displacements grid_1 tx 10 percent 5 floor 1e-6",
    )
    .unwrap();
    assert_eq!(
      s.bounds,
      Bounds::Percent {
        center: 10.0,
        tol_pct: 5.0,
        floor: Some(1e-6),
      },
    );
  }

  #[test]
  fn rejects_negative_percent() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 10 percent -1"),
      Err(OnelinerError::NegativePercent(_)),
    ));
  }

  #[test]
  fn rejects_negative_floor() {
    assert!(matches!(
      parse_oneliner(
        "subcase 1 displacements grid_1 tx 10 percent 5 floor -1e-9",
      ),
      Err(OnelinerError::NegativeFloor(_)),
    ));
  }

  #[test]
  fn rejects_bad_floor_literal() {
    assert!(matches!(
      parse_oneliner(
        "subcase 1 displacements grid_1 tx 10 percent 5 epsilon 1e-9",
      ),
      Err(OnelinerError::BadFloorLiteral(_)),
    ));
  }

  #[test]
  fn floor_only_for_percent() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 0 to 1 floor 1e-9",),
      Err(OnelinerError::FloorWithoutPercent(_)),
    ));
  }

  #[test]
  fn percent_bounds_floor_three_cases() {
    let b = Bounds::Percent {
      center: 0.0,
      tol_pct: 5.0,
      floor: Some(1e-6),
    };
    // Both below floor -> pass.
    assert!(b.contains(0.0));
    assert!(b.contains(1e-9));
    // Asymmetric -> fail.
    assert!(!b.contains(1.0));
    let b2 = Bounds::Percent {
      center: 10.0,
      tol_pct: 5.0,
      floor: Some(1e-6),
    };
    // Above floor on both sides -> percent formula.
    assert!(b2.contains(10.4));
    assert!(b2.contains(9.6));
    assert!(!b2.contains(11.0));
    // Asymmetric (ref above, test below floor) -> fail.
    assert!(!b2.contains(1e-9));
  }

  #[test]
  fn percent_no_floor_zero_ref() {
    let b = Bounds::Percent {
      center: 0.0,
      tol_pct: 5.0,
      floor: None,
    };
    assert!(b.contains(0.0));
    assert!(!b.contains(1e-300));
  }

  #[test]
  fn parses_comma_separated_lists() {
    let s =
      parse_oneliner("subcase 1,2,3 displacements 11,12 tx,ty 0 to 1").unwrap();
    assert_eq!(s.subcases, vec![1, 2, 3]);
    assert_eq!(s.rows, vec!["11".to_owned(), "12".to_owned()]);
    assert_eq!(s.cols, vec!["tx".to_owned(), "ty".to_owned()]);
    assert_eq!(s.cell_count(), 12);
  }

  #[test]
  fn rejects_empty_csv_entry() {
    assert!(matches!(
      parse_oneliner("subcase 1,,2 displacements grid_1 tx 0 to 1"),
      Err(OnelinerError::EmptyListEntry { field: "subcase" }),
    ));
  }

  #[test]
  fn parses_plus_minus_delta() {
    let s = parse_oneliner("subcase 1 displacements grid_1 tx 10 \u{00b1} 0.5")
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
  fn parses_plus_minus_ascii_alias_delta() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx 10 +- 0.5").unwrap();
    assert!(matches!(s.bounds, Bounds::Delta { .. }));
  }

  #[test]
  fn parses_plus_minus_percent() {
    let s = parse_oneliner("subcase 1 displacements grid_1 tx 10 \u{00b1} 5%")
      .unwrap();
    assert_eq!(
      s.bounds,
      Bounds::Percent {
        center: 10.0,
        tol_pct: 5.0,
        floor: None,
      },
    );
  }

  #[test]
  fn parses_plus_minus_percent_with_floor() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx 10 +- 5% floor 1e-6")
        .unwrap();
    assert_eq!(
      s.bounds,
      Bounds::Percent {
        center: 10.0,
        tol_pct: 5.0,
        floor: Some(1e-6),
      },
    );
  }

  #[test]
  fn plus_minus_delta_rejects_floor() {
    assert!(matches!(
      parse_oneliner(
        "subcase 1 displacements grid_1 tx 10 \u{00b1} 0.5 floor 1e-6",
      ),
      Err(OnelinerError::FloorWithoutPercent(_)),
    ));
  }

  #[test]
  fn plus_minus_negative_delta_rejected() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 10 \u{00b1} -0.5"),
      Err(OnelinerError::NegativeDelta(_)),
    ));
  }

  #[test]
  fn plus_minus_negative_percent_rejected() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx 10 \u{00b1} -5%"),
      Err(OnelinerError::NegativePercent(_)),
    ));
  }

  #[test]
  fn parses_satisfies_form() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx satisfies abs(x) < 1")
        .unwrap();
    assert!(matches!(s.bounds, Bounds::Satisfies(_)));
  }

  #[test]
  fn satisfies_is_case_insensitive() {
    let s =
      parse_oneliner("subcase 1 displacements grid_1 tx SATISFIES x == max")
        .unwrap();
    assert!(matches!(s.bounds, Bounds::Satisfies(_)));
  }

  #[test]
  fn satisfies_with_comma_lists() {
    let s = parse_oneliner(
      "subcase 1,2 displacements 11,12 tx,ty satisfies abs(x) <= 3*std",
    )
    .unwrap();
    assert!(matches!(s.bounds, Bounds::Satisfies(_)));
    assert_eq!(s.cell_count(), 8);
  }

  #[test]
  fn satisfies_rejects_missing_expression() {
    assert!(matches!(
      parse_oneliner("subcase 1 displacements grid_1 tx satisfies"),
      Err(OnelinerError::BadTokenCount(6)),
    ));
  }

  #[test]
  fn satisfies_rejects_bad_equation() {
    let r = parse_oneliner("subcase 1 displacements grid_1 tx satisfies y > 0");
    assert!(matches!(r, Err(OnelinerError::EquationParse(_))));
  }
}
