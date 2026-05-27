//! User-supplied boolean predicates over extracted values.
//!
//! An [`Predicate`] is parsed once at script-prepare time (or oneliner-parse
//! time) and evaluated per-datum. The grammar is a thin wrapper around
//! [`fasteval2`]:
//!
//! - Arithmetic: `+ - * / %`, exponentiation `^` (right-associative); the
//!   alias `**` is rewritten to `^` during preprocessing.
//! - Comparisons: `< <= == != >= >`.
//! - Booleans (symbol): `&& || !`.
//! - Booleans (word, case-insensitive): `and`, `or`, `not`, `xor`. `xor` is
//!   rewritten to `!=`, which behaves as logical xor for the boolean (0.0 /
//!   1.0) operands that comparison operators produce.
//! - Identifiers and keywords are case-insensitive (the whole expression is
//!   lowercased before parsing).
//! - Functions (parser-level): `abs`, `log(base, val)`, `min(..)`, `max(..)`,
//!   `sin`, `cos`, `tan`, plus the inverses and hyperbolic variants, plus
//!   `int`, `ceil`, `floor`, `sign`, `round`. `pi()` and `e()` give the
//!   constants; bare `pi`/`e` also work via namespace variables.
//! - Functions (namespace-level): `sqrt(x)`, `exp(x)`, `ln(x)`.
//!
//! ## Variables
//!
//! Variables are scoped by [`Scope`]:
//!
//! | scope                 | value names | stats prefix |
//! |-----------------------|-------------|--------------|
//! | `Scope::Check`        | `x`, `t`    | none         |
//! | `Scope::OnelinerCell` | `x`, `t`    | none         |
//! | `Scope::Comparison`   | `x`, `t` (test) and `y`, `r` (reference) | `x*`/`t*` for test, `y*`/`r*` for reference |
//!
//! Stat names (with the appropriate prefix in comparisons): `min`, `max`,
//! `mina`, `maxa` (minimum / maximum **absolute** value in the pool),
//! `avg`, `sum`, `std`, `stdp` (population stddev, same as `std`), `stds`
//! (sample stddev), `n` (count of values in the pool).
//!
//! ## Result semantics
//!
//! An predicate evaluates to an `f64`. The datum is flagged as **FAIL** when:
//!
//! - the result is NaN, or
//! - the result equals `0.0` (logical false).
//!
//! Any other finite or infinite, non-zero value is a **PASS**. Comparison
//! operators in fasteval2 already produce `1.0`/`0.0`, so the natural form
//! `x < 1.0 and y >= 0` works as expected.
//!
//! ## Stats
//!
//! [`Stats`] are computed once per evaluation scope (per `(file, extraction)`
//! for checks, per `(reference, test, extraction-union)` for comparisons,
//! once over the cartesian product for oneliners). NaN/Inf values are
//! skipped. An empty pool is a hard error.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};

use fasteval2::{Evaler, Parser, Slab};

/// The scope in which an [`Predicate`] is evaluated. Determines which
/// variable names are allowed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
  /// A `[[check]]` table -- single file, no reference value.
  Check,
  /// A `[[comparison]]` table -- has both test and reference values.
  Comparison,
  /// A single cell of a `--oneliner satisfies` run -- one file, no
  /// reference value. Variable set matches [`Scope::Check`].
  OnelinerCell,
}

impl Scope {
  /// Returns true when the reference-value variables (`y`, `r`, `y*`, `r*`)
  /// are in scope.
  pub(crate) fn has_reference(self) -> bool {
    return matches!(self, Scope::Comparison);
  }
}

/// Population/sample statistics over an extraction's matched values. Built
/// from the `f64` values for one scope; NaN and infinite values are skipped.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Stats {
  /// Number of finite values in the pool (after NaN/Inf skipping).
  pub(crate) n: usize,
  /// Sum of all finite values.
  pub(crate) sum: f64,
  /// Minimum.
  pub(crate) min: f64,
  /// Maximum.
  pub(crate) max: f64,
  /// Minimum absolute value (smallest `|x|` in the pool).
  pub(crate) min_abs: f64,
  /// Maximum absolute value (largest `|x|` in the pool).
  pub(crate) max_abs: f64,
  /// Arithmetic mean.
  pub(crate) avg: f64,
  /// Population standard deviation (\u03a3(x-\u03bc)^2 / N)^{1/2}.
  pub(crate) std_pop: f64,
  /// Sample standard deviation (\u03a3(x-\u03bc)^2 / (N-1))^{1/2}, or 0 when
  /// only one value is in the pool.
  pub(crate) std_sam: f64,
}

impl Stats {
  /// Computes stats from an iterator of values. Skips NaN/Inf. Returns
  /// `None` when the resulting pool is empty.
  pub(crate) fn from_values<I: IntoIterator<Item = f64>>(
    values: I,
  ) -> Option<Self> {
    let kept: Vec<f64> = values.into_iter().filter(|v| v.is_finite()).collect();
    if kept.is_empty() {
      return None;
    }
    let n = kept.len();
    let sum: f64 = kept.iter().sum();
    let avg = sum / n as f64;
    let mut min = kept[0];
    let mut max = kept[0];
    let mut min_abs = kept[0].abs();
    let mut max_abs = kept[0].abs();
    let mut sq_dev: f64 = 0.0;
    for v in &kept {
      if *v < min {
        min = *v;
      }
      if *v > max {
        max = *v;
      }
      let a = v.abs();
      if a < min_abs {
        min_abs = a;
      }
      if a > max_abs {
        max_abs = a;
      }
      let d = *v - avg;
      sq_dev += d * d;
    }
    let std_pop = (sq_dev / n as f64).sqrt();
    let std_sam = if n > 1 {
      (sq_dev / (n - 1) as f64).sqrt()
    } else {
      0.0
    };
    return Some(Self {
      n,
      sum,
      min,
      max,
      min_abs,
      max_abs,
      avg,
      std_pop,
      std_sam,
    });
  }
}

/// Stat name suffixes that may be requested (with or without a side
/// prefix). All lowercase.
const STAT_SUFFIXES: &[&str] = &[
  "min", "max", "mina", "maxa", "avg", "sum", "std", "stdp", "stds", "n",
];

/// Bare value-variable names for a check / oneliner cell.
const VALUE_NAMES_CHECK: &[&str] = &["x", "t"];
/// Bare value-variable names for a comparison (test side).
const TEST_NAMES_COMPARISON: &[&str] = &["x", "t"];
/// Bare value-variable names for a comparison (reference side).
const REF_NAMES_COMPARISON: &[&str] = &["y", "r"];

/// User-function names handled by our namespace (not by the fasteval2
/// parser as builtins). Used during scope validation so the names are not
/// flagged as unknown variables.
const NS_FUNCTIONS: &[&str] = &["sqrt", "exp", "ln"];

/// Mathematical constants exposed as bare variables (in addition to the
/// `pi()` / `e()` parser-level functions).
const MATH_CONSTANTS: &[&str] = &["pi", "e"];

/// A parsed predicate, ready to be evaluated against per-datum values plus
/// a precomputed [`Stats`] pool.
#[derive(Debug)]
pub(crate) struct Predicate {
  /// Original (un-preprocessed) source string, kept for human-readable
  /// reporting in flagged-datum lines.
  raw: String,
  /// fasteval2's parser arena; owned alongside the parsed expression to
  /// keep `Expression` references valid for the lifetime of this struct.
  slab: Slab,
  /// Index of the parsed expression within `slab.ps`.
  expr_i: fasteval2::ExpressionI,
  /// Scope this predicate was parsed for.
  scope: Scope,
}

impl Predicate {
  /// Returns the raw (original) source string, useful for reporting.
  pub(crate) fn raw(&self) -> &str {
    return &self.raw;
  }

  /// Returns the scope this predicate was parsed for.
  pub(crate) fn scope(&self) -> Scope {
    return self.scope;
  }

  /// Parses and validates an predicate for the given scope.
  pub(crate) fn parse(raw: &str, scope: Scope) -> Result<Self, PredicateError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
      return Err(PredicateError::Empty);
    }
    let preprocessed = preprocess(trimmed);
    let mut slab = Slab::new();
    let parser = Parser::new();
    let expr_i = parser.parse(&preprocessed, &mut slab.ps).map_err(|e| {
      PredicateError::Parse {
        raw: raw.to_owned(),
        message: format!("{e}"),
      }
    })?;
    // Walk variable names and reject any not in scope.
    let used: BTreeSet<String> = slab.ps.get_expr(expr_i).var_names(&slab);
    let allowed = allowed_identifiers(scope);
    let unknown: Vec<String> = used
      .iter()
      .filter(|n| !allowed.contains(n.as_str()))
      .cloned()
      .collect();
    if !unknown.is_empty() {
      return Err(PredicateError::UnknownIdentifier {
        raw: raw.to_owned(),
        unknown,
        scope,
      });
    }
    return Ok(Self {
      raw: raw.to_owned(),
      slab,
      expr_i,
      scope,
    });
  }

  /// Evaluates this predicate. Returns the verdict and the underlying
  /// numeric result (useful for verbose reporting).
  ///
  /// `x_val` is the test value (or the single value for checks/oneliner).
  /// `y_val` is the reference value (must be `Some` in [`Scope::Comparison`]
  /// and is ignored otherwise). `x_stats` is the test-side stats (or the
  /// single pool's stats for checks). `y_stats` is the reference-side stats
  /// (required in `Scope::Comparison`).
  pub(crate) fn evaluate(
    &self,
    x_val: f64,
    y_val: Option<f64>,
    x_stats: &Stats,
    y_stats: Option<&Stats>,
  ) -> EvalOutcome {
    debug_assert!(
      !(self.scope.has_reference() && (y_val.is_none() || y_stats.is_none())),
      "comparison predicate needs reference value and stats",
    );
    let mut variables: BTreeMap<String, f64> = BTreeMap::new();
    populate_constants(&mut variables);
    populate_for_scope(
      self.scope,
      &mut variables,
      x_val,
      y_val,
      x_stats,
      y_stats,
    );
    let expr_ref = self.slab.ps.get_expr(self.expr_i);
    let mut ns = build_namespace(variables);
    return match expr_ref.eval(&self.slab, &mut ns) {
      Ok(v) => {
        if v.is_nan() || v == 0.0 {
          EvalOutcome::Fail { value: v }
        } else {
          EvalOutcome::Pass { value: v }
        }
      }
      Err(e) => EvalOutcome::Error {
        message: format!("{e}"),
      },
    };
  }
}

/// Outcome of evaluating an [`Predicate`] against one datum.
#[derive(Clone, Debug)]
pub(crate) enum EvalOutcome {
  /// Predicate returned a non-zero, non-NaN value.
  Pass {
    /// The numeric result (useful for diagnostic reporting).
    value: f64,
  },
  /// Predicate returned `0.0` or `NaN`.
  Fail {
    /// The numeric result.
    value: f64,
  },
  /// Evaluation itself failed (e.g. undefined function call). Treated as
  /// a hard failure of the rule by callers.
  Error {
    /// Error message rendered by `fasteval2`.
    message: String,
  },
}

impl EvalOutcome {
  /// True iff this outcome should NOT flag the datum (i.e. it is a PASS).
  pub(crate) fn passed(&self) -> bool {
    return matches!(self, EvalOutcome::Pass { .. });
  }
}

/// Errors raised by [`Predicate::parse`].
#[derive(Clone, Debug)]
pub(crate) enum PredicateError {
  /// The predicate source string was empty or whitespace-only.
  Empty,
  /// fasteval2 rejected the (preprocessed) expression.
  Parse {
    /// Original source.
    raw: String,
    /// Underlying message.
    message: String,
  },
  /// The expression references an identifier that is not in scope.
  UnknownIdentifier {
    /// Original source.
    raw: String,
    /// Identifiers that were not recognised.
    unknown: Vec<String>,
    /// Scope under which the predicate was parsed.
    scope: Scope,
  },
}

impl Display for PredicateError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    return match self {
      Self::Empty => write!(f, "predicate is empty"),
      Self::Parse { raw, message } => {
        write!(f, "could not parse predicate \"{raw}\": {message}")
      }
      Self::UnknownIdentifier {
        raw,
        unknown,
        scope,
      } => {
        let scope_name = match scope {
          Scope::Check => "check",
          Scope::Comparison => "comparison",
          Scope::OnelinerCell => "oneliner",
        };
        write!(
          f,
          "predicate \"{raw}\" uses identifier(s) not in scope for a \
           {scope_name}: {}; valid identifiers are: {}",
          unknown.join(", "),
          allowed_identifiers_pretty(*scope).join(", "),
        )
      }
    };
  }
}

impl std::error::Error for PredicateError {}

/// Preprocesses an predicate source string into a form fasteval2 understands:
/// lowercases everything, rewrites `**` to `^`, and replaces word-boundary
/// keyword operators `not`, `and`, `or`, `xor` with `!`, `&&`, `||`, `!=`.
fn preprocess(input: &str) -> String {
  let lowered = input.to_ascii_lowercase();
  let no_pow_alias = lowered.replace("**", "^");
  return replace_word_operators(&no_pow_alias);
}

/// Replaces standalone keyword operators with their symbolic equivalents.
/// A keyword only matches when surrounded by ASCII non-identifier characters
/// (or string boundaries), so identifiers like `tornado` or `band` are
/// untouched.
fn replace_word_operators(input: &str) -> String {
  // (keyword, replacement)
  const REPLS: &[(&str, &str)] = &[
    ("not", " !"),
    ("and", " && "),
    ("or", " || "),
    ("xor", " != "),
  ];
  let bytes = input.as_bytes();
  let mut out = String::with_capacity(input.len());
  let mut i = 0usize;
  'outer: while i < bytes.len() {
    let c = bytes[i] as char;
    if c.is_ascii_alphabetic() || c == '_' {
      // Find the end of this identifier-like run.
      let start = i;
      while i < bytes.len() {
        let cc = bytes[i] as char;
        if cc.is_ascii_alphanumeric() || cc == '_' {
          i += 1;
        } else {
          break;
        }
      }
      let word = &input[start..i];
      let prev_ok = start == 0 || {
        let pc = bytes[start - 1] as char;
        !(pc.is_ascii_alphanumeric() || pc == '_')
      };
      let next_ok = i >= bytes.len() || {
        let nc = bytes[i] as char;
        !(nc.is_ascii_alphanumeric() || nc == '_')
      };
      if prev_ok && next_ok {
        for (kw, repl) in REPLS {
          if word == *kw {
            out.push_str(repl);
            continue 'outer;
          }
        }
      }
      out.push_str(word);
    } else {
      out.push(c);
      i += 1;
    }
  }
  return out;
}

/// Returns the set of identifiers (variables + namespace-handled function
/// names + math constants) that are in scope.
fn allowed_identifiers(scope: Scope) -> BTreeSet<&'static str> {
  let mut s: BTreeSet<&'static str> = BTreeSet::new();
  for c in MATH_CONSTANTS {
    s.insert(c);
  }
  for f in NS_FUNCTIONS {
    s.insert(f);
  }
  match scope {
    Scope::Check | Scope::OnelinerCell => {
      for n in VALUE_NAMES_CHECK {
        s.insert(n);
      }
      for suf in STAT_SUFFIXES {
        s.insert(suf);
      }
    }
    Scope::Comparison => {
      for n in TEST_NAMES_COMPARISON {
        s.insert(n);
      }
      for n in REF_NAMES_COMPARISON {
        s.insert(n);
      }
      // Comparison stats are always prefixed with a side letter.
      for prefix in ["x", "t", "y", "r"] {
        for suf in STAT_SUFFIXES {
          // SAFETY: leak intentionally; identifier names live for the
          // process lifetime and the set is small (~32 entries).
          let owned = format!("{prefix}{suf}");
          let leaked: &'static str = Box::leak(owned.into_boxed_str());
          s.insert(leaked);
        }
      }
    }
  }
  return s;
}

/// Human-readable listing of allowed identifiers (sorted) for use in error
/// messages.
fn allowed_identifiers_pretty(scope: Scope) -> Vec<String> {
  return allowed_identifiers(scope)
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
}

/// Populates the math constants (`pi`, `e`) into the variable map.
fn populate_constants(variables: &mut BTreeMap<String, f64>) {
  variables.insert("pi".to_owned(), std::f64::consts::PI);
  variables.insert("e".to_owned(), std::f64::consts::E);
}

/// Inserts the per-scope value and stat variables into the variable map.
fn populate_for_scope(
  scope: Scope,
  variables: &mut BTreeMap<String, f64>,
  x_val: f64,
  y_val: Option<f64>,
  x_stats: &Stats,
  y_stats: Option<&Stats>,
) {
  match scope {
    Scope::Check | Scope::OnelinerCell => {
      for n in VALUE_NAMES_CHECK {
        variables.insert((*n).to_owned(), x_val);
      }
      insert_stats(variables, "", x_stats);
    }
    Scope::Comparison => {
      for n in TEST_NAMES_COMPARISON {
        variables.insert((*n).to_owned(), x_val);
      }
      let y = y_val.expect("comparison scope needs reference value");
      for n in REF_NAMES_COMPARISON {
        variables.insert((*n).to_owned(), y);
      }
      let ys = y_stats.expect("comparison scope needs reference stats");
      for prefix in ["x", "t"] {
        insert_stats(variables, prefix, x_stats);
      }
      for prefix in ["y", "r"] {
        insert_stats(variables, prefix, ys);
      }
    }
  }
}

/// Writes stat values into the variable map under the given prefix.
fn insert_stats(
  variables: &mut BTreeMap<String, f64>,
  prefix: &str,
  stats: &Stats,
) {
  let pairs: [(&str, f64); 10] = [
    ("min", stats.min),
    ("max", stats.max),
    ("mina", stats.min_abs),
    ("maxa", stats.max_abs),
    ("avg", stats.avg),
    ("sum", stats.sum),
    ("std", stats.std_pop),
    ("stdp", stats.std_pop),
    ("stds", stats.std_sam),
    ("n", stats.n as f64),
  ];
  for (suf, val) in pairs {
    variables.insert(format!("{prefix}{suf}"), val);
  }
}

/// Builds the [`fasteval2::EvalNamespace`] used for evaluation. Resolves
/// variables from the supplied map and a few namespace-level functions
/// (`sqrt`, `exp`, `ln`).
fn build_namespace(
  variables: BTreeMap<String, f64>,
) -> impl FnMut(&str, Vec<f64>) -> Option<f64> {
  return move |name: &str, args: Vec<f64>| -> Option<f64> {
    if args.is_empty()
      && let Some(v) = variables.get(name)
    {
      return Some(*v);
    }
    if args.len() == 1 {
      let a = args[0];
      return match name {
        "sqrt" => Some(a.sqrt()),
        "exp" => Some(a.exp()),
        "ln" => Some(a.ln()),
        _ => None,
      };
    }
    return None;
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  fn stats(values: &[f64]) -> Stats {
    return Stats::from_values(values.iter().copied()).expect("non-empty");
  }

  #[test]
  fn preprocess_lowercases() {
    assert_eq!(preprocess("X + Y"), "x + y");
  }

  #[test]
  fn preprocess_rewrites_power_alias() {
    assert_eq!(preprocess("x**2"), "x^2");
    assert_eq!(preprocess("(a+b)**(c-d)"), "(a+b)^(c-d)");
  }

  #[test]
  fn preprocess_rewrites_word_ops_only_when_standalone() {
    let s = preprocess("a and b or not c xor d");
    // Whitespace can be irregular but the operator tokens should appear.
    assert!(s.contains("&&"), "missing &&: {s}");
    assert!(s.contains("||"), "missing ||: {s}");
    assert!(s.contains('!'), "missing !: {s}");
    assert!(s.contains("!="), "missing !=: {s}");
  }

  #[test]
  fn preprocess_does_not_touch_identifiers_containing_keywords() {
    // tornado has "or" inside, band has "and" inside, candor has "and"
    // and "or" inside. None should be rewritten.
    let s = preprocess("tornado + band + candor");
    assert_eq!(s, "tornado + band + candor");
  }

  #[test]
  fn stats_skips_nan_and_inf() {
    let s = Stats::from_values(vec![1.0, f64::NAN, 2.0, f64::INFINITY, 3.0])
      .expect("non-empty");
    assert_eq!(s.n, 3);
    assert_eq!(s.sum, 6.0);
    assert_eq!(s.min, 1.0);
    assert_eq!(s.max, 3.0);
    assert!((s.avg - 2.0).abs() < 1e-12);
  }

  #[test]
  fn stats_empty_pool_returns_none() {
    let s: Option<Stats> = Stats::from_values(vec![f64::NAN, f64::INFINITY]);
    assert!(s.is_none());
  }

  #[test]
  fn stats_stddev_pop_vs_sample() {
    let s = stats(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
    // population variance = 4, so std_pop = 2.0
    assert!((s.std_pop - 2.0).abs() < 1e-12);
    // sample variance = 32/7 \u2248 4.5714, so std_sam \u2248 2.1380
    assert!((s.std_sam - (32.0_f64 / 7.0).sqrt()).abs() < 1e-12);
  }

  #[test]
  fn stats_single_value_sample_std_is_zero() {
    let s = stats(&[3.15]);
    assert_eq!(s.std_pop, 0.0);
    assert_eq!(s.std_sam, 0.0);
  }

  #[test]
  fn predicate_check_simple_comparison() {
    let eq = Predicate::parse("x < 1.0", Scope::Check).unwrap();
    let st = stats(&[0.0, 0.5, 0.9]);
    assert!(eq.evaluate(0.5, None, &st, None).passed());
    assert!(!eq.evaluate(1.5, None, &st, None).passed());
  }

  #[test]
  fn predicate_check_case_insensitive() {
    let eq = Predicate::parse("X < MAX AND X > MIN", Scope::Check).unwrap();
    let st = stats(&[0.0, 1.0, 2.0]);
    assert!(eq.evaluate(1.0, None, &st, None).passed());
    assert!(!eq.evaluate(0.0, None, &st, None).passed());
    assert!(!eq.evaluate(2.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_power_operators() {
    // 2**3 == 8 and 2^3 == 8
    let eq = Predicate::parse("x == 2**3", Scope::Check).unwrap();
    let st = stats(&[8.0]);
    assert!(eq.evaluate(8.0, None, &st, None).passed());
    let eq2 = Predicate::parse("x == 2^3", Scope::Check).unwrap();
    assert!(eq2.evaluate(8.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_xor_via_keyword() {
    // (x > 0) xor (x > 5) is true iff 0 < x <= 5
    let eq = Predicate::parse("(x > 0) xor (x > 5)", Scope::Check).unwrap();
    let st = stats(&[3.0]);
    assert!(eq.evaluate(3.0, None, &st, None).passed());
    assert!(!eq.evaluate(10.0, None, &st, None).passed());
    assert!(!eq.evaluate(-1.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_namespace_functions() {
    let eq = Predicate::parse(
      "sqrt(x) > 1 and ln(x) > 0 and exp(0) == 1",
      Scope::Check,
    )
    .unwrap();
    let st = stats(&[4.0]);
    assert!(eq.evaluate(4.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_constants() {
    let eq = Predicate::parse("x > pi and x < pi * 2", Scope::Check).unwrap();
    let st = stats(&[5.0]);
    assert!(eq.evaluate(5.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_three_sigma_check() {
    let eq =
      Predicate::parse("x >= avg - 3*std and x <= avg + 3*std", Scope::Check)
        .unwrap();
    let st = stats(&[10.0, 10.0, 10.0, 10.0]); // std = 0
    assert!(eq.evaluate(10.0, None, &st, None).passed());
    assert!(!eq.evaluate(11.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_comparison_uses_y_and_r() {
    let eq =
      Predicate::parse("abs(x - y) <= 0.01 * rmax", Scope::Comparison).unwrap();
    let x_st = stats(&[1.0, 2.0, 3.0]);
    let y_st = stats(&[1.0, 2.0, 3.0]);
    let pass = eq.evaluate(1.005, Some(1.0), &x_st, Some(&y_st));
    let fail = eq.evaluate(1.5, Some(1.0), &x_st, Some(&y_st));
    assert!(pass.passed());
    assert!(!fail.passed());
  }

  #[test]
  fn predicate_check_rejects_reference_variables() {
    let err = Predicate::parse("x > y", Scope::Check).unwrap_err();
    assert!(matches!(err, PredicateError::UnknownIdentifier { .. }));
    let err2 = Predicate::parse("x > rmax", Scope::Check).unwrap_err();
    assert!(matches!(err2, PredicateError::UnknownIdentifier { .. }));
  }

  #[test]
  fn predicate_comparison_rejects_unprefixed_stats() {
    // In Scope::Comparison, bare `min`/`max` are not in scope; must use
    // `xmin`/`tmin`/`ymin`/`rmin` etc.
    let err = Predicate::parse("x > min", Scope::Comparison).unwrap_err();
    assert!(matches!(err, PredicateError::UnknownIdentifier { .. }));
  }

  #[test]
  fn predicate_rejects_empty_string() {
    let err = Predicate::parse("   ", Scope::Check).unwrap_err();
    assert!(matches!(err, PredicateError::Empty));
  }

  #[test]
  fn predicate_nan_result_is_fail() {
    // ln(-1) == NaN
    let eq = Predicate::parse("ln(x)", Scope::Check).unwrap();
    let st = stats(&[1.0]);
    let out = eq.evaluate(-1.0, None, &st, None);
    assert!(!out.passed());
  }

  #[test]
  fn predicate_zero_result_is_fail() {
    let eq = Predicate::parse("x - x", Scope::Check).unwrap();
    let st = stats(&[1.0]);
    assert!(!eq.evaluate(1.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_symbol_operators_work() {
    let eq =
      Predicate::parse("!(x < 0) && x < 10 || x == 42", Scope::Check).unwrap();
    let st = stats(&[5.0]);
    assert!(eq.evaluate(5.0, None, &st, None).passed());
    assert!(!eq.evaluate(-1.0, None, &st, None).passed());
    assert!(eq.evaluate(42.0, None, &st, None).passed());
  }

  #[test]
  fn stats_mina_maxa_track_absolute_values() {
    let s = stats(&[-5.0, -1.0, 2.0, 3.0]);
    assert_eq!(s.min_abs, 1.0);
    assert_eq!(s.max_abs, 5.0);
  }

  #[test]
  fn predicate_mina_maxa_in_check_scope() {
    let eq = Predicate::parse("abs(x) >= mina and abs(x) <= maxa", Scope::Check)
      .unwrap();
    let st = stats(&[-5.0, -1.0, 2.0, 3.0]);
    assert!(eq.evaluate(2.0, None, &st, None).passed());
    assert!(eq.evaluate(-5.0, None, &st, None).passed());
    // Outside the [|min|, |max|] band on the absolute scale.
    assert!(!eq.evaluate(0.5, None, &st, None).passed());
    assert!(!eq.evaluate(10.0, None, &st, None).passed());
  }

  #[test]
  fn predicate_mina_maxa_in_comparison_scope_need_prefix() {
    // Bare mina/maxa are not in scope for comparisons -- must be xmina /
    // rmaxa / etc.
    let err = Predicate::parse("x >= mina", Scope::Comparison).unwrap_err();
    assert!(matches!(err, PredicateError::UnknownIdentifier { .. }));
    let eq = Predicate::parse(
      "abs(x - y) <= 0.01 * rmaxa or abs(x) <= xmina",
      Scope::Comparison,
    )
    .unwrap();
    let xs = stats(&[1.0, 2.0, 3.0]);
    let ys = stats(&[1.0, 2.0, 3.0]);
    assert!(eq.evaluate(1.005, Some(1.0), &xs, Some(&ys)).passed());
  }

  #[test]
  fn min_max_variable_and_function_coexist() {
    // The parser disambiguates by parentheses: `min`/`max` with no parens
    // is the pool-stats variable, `min(..)` / `max(..)` is the built-in
    // function. Both must work in the same predicate.
    let eq = Predicate::parse(
      "x >= min and x <= max and x == max(min, x)",
      Scope::Check,
    )
    .unwrap();
    let st = stats(&[0.0, 5.0, 10.0]);
    // x = 5: between min (0) and max (10), and max(0, 5) == 5.
    assert!(eq.evaluate(5.0, None, &st, None).passed());
    // x = 11: above pool max -> fails the second clause.
    assert!(!eq.evaluate(11.0, None, &st, None).passed());

    // Spot-check that the built-in functions still compute what they should
    // when used alongside the variables.
    let eq2 = Predicate::parse(
      "min(x, max) == min and max(x, min) == max",
      Scope::Check,
    )
    .unwrap();
    // x below the pool min: min(x, max=10) == x, NOT min=0 -> fails.
    assert!(!eq2.evaluate(-1.0, None, &st, None).passed());
    // x inside the pool: min(x=5, max=10) == 5 != min=0 -> fails.
    assert!(!eq2.evaluate(5.0, None, &st, None).passed());
    // x == min: min(0, 10) == 0 and max(0, 0) == 0 == max? No, max=10 -> fail.
    assert!(!eq2.evaluate(0.0, None, &st, None).passed());
  }
}
