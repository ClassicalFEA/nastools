//! This program is meant as a successor to f06diff and a command-line based
//! replacement for nastester. It consumes a "script", which is just a TOML file
//! containing a series of tests to do on one or more F06 files, and generates
//! a report.

#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]
#![allow(clippy::needless_return)]
#![allow(dead_code)]

pub(crate) mod oneliner;
pub(crate) mod script;
pub(crate) mod utils;

use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use f06::prelude::*;
use toml::de::Error as TomlError;

use crate::oneliner::{
  CellOutcome, CellResult, error_exit_code, parse_oneliner, run_oneliner,
};
use crate::script::Script;
use crate::script::check::{CheckFailure, CheckRule};
use crate::script::comparison::{FlagReason2, FlaggedDetail};

/// f06magic command-line interface.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
  /// Path to a TOML script (default mode), or to an F06 file when
  /// `--oneliner` is supplied.
  path: Option<String>,
  /// List the row/column index types accepted by every block (or only the
  /// requested block type) and exit. Useful when authoring a script.
  #[arg(
    long,
    value_name = "BLOCK",
    num_args = 0..=1,
    default_missing_value = ""
  )]
  indices: Option<String>,
  /// Run a single PASS/FAIL check against the F06 file at <PATH>.
  ///
  /// Spec format (must be quoted on the shell):
  ///
  ///   subcase <N> <block> <row> <col> <A> <to|delta> <B>
  ///   subcase <N> <block> <row> <col> <A> <percent|pct> <P> [floor <E>]
  ///   subcase <N> <block> <row> <col> <A> ± <B>        (alias: +-)
  ///   subcase <N> <block> <row> <col> <A> ± <P>% [floor <E>]
  ///   subcase <N> <block> <row> <col> satisfies <predicate ...>
  ///
  /// `to` is an inclusive range; `delta` / `±` is `[A - B, A + B]` (B >= 0);
  /// `percent` / `pct` matches `100*|test/A - 1| <= P`; `floor <E>` lets near-
  /// zero pairs pass when both `|A|` and `|test|` fall below `E`.
  ///
  /// `satisfies <expr>` evaluates a user-supplied boolean predicate per cell;
  /// the cell's value is bound to `x` (or `t`). Available operators include
  /// `+ - * / % ** ^`, comparisons `== != < <= > >=`, and boolean
  /// `! && ||` (or `not`/`and`/`or`/`xor`). Functions: `abs`, `sqrt`,
  /// `exp`, `ln`, `log(base, val)`, `sin/cos/tan`, `ceil`, `floor`,
  /// `round`, `sign`, `int`, `min(a, b, ...)`, `max(a, b, ...)`. Constants:
  /// `pi`, `e`. The cartesian-product pool also exposes magic stat
  /// variables: `min`, `max`, `mina`, `maxa` (min / max of absolute value),
  /// `avg`, `sum`, `std` (= `stdp`), `stds`, `n`. PASS iff the expression
  /// evaluates to a finite, non-zero value.
  ///
  /// Subcase, row and col tokens accept comma-separated lists; the spec
  /// is then run on the cartesian product (one PASS/FAIL per cell).
  ///
  /// Prints PASS/FAIL/ERROR on stdout and the value (or error) on stderr.
  /// Exit codes (oneliner): 0 PASS, 1 FAIL, 2 extraction error,
  /// 3 spec parse error, 4 F06 parse error.
  ///
  /// Exit codes (script mode): total number of flagged values across all
  /// comparisons and checks (0 means all passed, capped at 254), or 255
  /// (-1) on a general failure such as a TOML parse error or F06 parse
  /// error.
  #[arg(long, value_name = "SPEC", conflicts_with = "indices")]
  oneliner: Option<String>,
  /// Show one line per flagged datum on stderr (both script and oneliner
  /// modes). The stdout summary is unchanged.
  #[arg(long, short = 'v')]
  verbose: bool,
}

/// Runs a script in a given path and outputs results.
///
/// Returns the total number of flagged values across all comparisons and
/// checks (0 when everything passed).
fn run_script<P: AsRef<Path>>(
  path: P,
  verbose: bool,
) -> Result<usize, Box<dyn Error>> {
  let contents = std::fs::read_to_string(path)?;
  let try_script: Result<Script, TomlError> = toml::from_str(&contents);
  let script = try_script?.prepare()?;
  let mut flagged_total: usize = 0;
  for comp in script.comparisons.keys() {
    let res = script.run_comparison(comp)?;
    let total_failures = res.flagged.len() + res.empty_extractions.len();
    let pass = if total_failures == 0 {
      "PASSED"
    } else {
      "FAILED"
    };
    flagged_total += total_failures;
    println!("==> {comp}: {pass}");
    println!("  => checked: {}", res.checked.len());
    println!("  => flagged: {}", res.flagged.len());
    if !res.empty_extractions.is_empty() {
      let listed: Vec<String> = res
        .empty_extractions
        .iter()
        .map(|(n, s)| format!("{n} ({})", s.label()))
        .collect();
      println!(
        "  => empty extractions (allow_*_empty=false): {}",
        listed.join(", ")
      );
    }
    if verbose {
      for (en, side) in &res.empty_extractions {
        eprintln!(
          "  - extraction \"{en}\": matched zero datums on {} side",
          side.label()
        );
      }
      for (di, det) in res.flagged.iter() {
        eprintln!("  - {}", fmt_comparison_failure(di, det));
      }
    }
  }
  for ck in script.checks.keys() {
    let res = script.run_check(ck)?;
    println!("==> {ck}:");
    for ((f, ex), rp) in res.per_pair.iter() {
      let extra = if rp.empty_violation { 1 } else { 0 };
      let total_failures = rp.flagged.len() + extra;
      let pass = if total_failures == 0 {
        "PASSED"
      } else {
        "FAILED"
      };
      let a = rp.flagged.len();
      let b = rp.checked.len();
      flagged_total += total_failures;
      if rp.empty_violation {
        println!("  => {f}, {ex}: {pass} ({a}/{b} flagged, extraction empty)");
      } else {
        println!("  => {f}, {ex}: {pass} ({a}/{b} flagged)");
      }
      if verbose {
        if rp.empty_violation {
          eprintln!(
            "  - extraction \"{ex}\" on file \"{f}\": matched zero datums"
          );
        }
        for (di, fail) in rp.flagged.iter() {
          eprintln!("  - {}", fmt_check_failure(di, fail));
        }
      }
    }
  }
  if script.comparisons.is_empty() {
    println!("no comparisons in script");
  }
  if script.checks.is_empty() {
    println!("no checks in script");
  }
  return Ok(flagged_total);
}

/// Formats a flagged comparison datum as a single verbose line.
fn fmt_comparison_failure(di: &DatumIndex, det: &FlaggedDetail) -> String {
  let head = format!(
    "subcase={} block={} row={} col={}: ref={} test={}  [{}]",
    di.block_ref.subcase,
    di.block_ref.block_type.short_name(),
    di.row,
    di.col,
    det.ref_val,
    det.test_val,
    fmt_reason2(&det.reason),
  );
  return append_printouts(head, &det.printouts);
}

/// Formats a magic-side [`FlagReason2`] for verbose output.
fn fmt_reason2(reason: &FlagReason2) -> String {
  return match reason {
    FlagReason2::Criteria(r) => fmt_reason(r),
    FlagReason2::Predicate { raw, value, error } => match error {
      Some(msg) => format!("predicate \"{raw}\" error: {msg}"),
      None => format!("predicate \"{raw}\" = {value}"),
    },
  };
}

/// Formats a libf06 [`FlagReason`] into its bracketed metric, e.g.
/// `difference=1.0e-3 > 1.0e-6` or `percent=5.2 > 1.0`.
fn fmt_reason(reason: &FlagReason) -> String {
  return match reason {
    FlagReason::Difference {
      abs_difference,
      max_epsilon,
    } => format!("difference={abs_difference} > {max_epsilon}"),
    FlagReason::Ratio {
      big_to_small,
      max_ratio,
    } => format!("ratio={big_to_small:.3} > {max_ratio:.3}"),
    FlagReason::Percent {
      percent,
      max_percent,
    } => format!("percent={percent:.3} > {max_percent:.3}"),
    FlagReason::FloorAsymmetry {
      ref_val,
      test_val,
      floor,
    } => {
      format!("floor_asymmetry: ref={ref_val} test={test_val} floor={floor}")
    }
    FlagReason::NaN => "nan".to_owned(),
    FlagReason::Infinity => "inf".to_owned(),
    FlagReason::Signs => "signs differ".to_owned(),
    FlagReason::Disjunction => "missing in one file".to_owned(),
  };
}

/// Formats a flagged check datum as a single verbose line.
fn fmt_check_failure(di: &DatumIndex, fail: &CheckFailure) -> String {
  let rule = match &fail.rule {
    CheckRule::AllEqual { expected } => {
      format!("all_equal: expected {expected}")
    }
    CheckRule::AllInRange { lo, hi } => {
      format!("all_in_range: outside [{lo}, {hi}]")
    }
    CheckRule::ExactValues { idx, expected } => {
      format!("exact_values[{idx}]: expected {expected}")
    }
    CheckRule::Ranges { idx, lo, hi } => {
      format!("ranges[{idx}]: outside [{lo}, {hi}]")
    }
    CheckRule::Predicate { raw, value, error } => match error {
      Some(msg) => format!("predicate \"{raw}\" error: {msg}"),
      None => format!("predicate \"{raw}\" = {value}"),
    },
  };
  let head = format!(
    "subcase={} block={} row={} col={}: value={}  [{rule}]",
    di.block_ref.subcase,
    di.block_ref.block_type.short_name(),
    di.row,
    di.col,
    fail.value,
  );
  return append_printouts(head, &fail.printouts);
}

/// If any printout values are present, appends them as a single
/// space-separated `printouts: label=value ...` segment.
fn append_printouts(
  head: String,
  values: &[(String, crate::script::printout::PrintoutValue)],
) -> String {
  if values.is_empty() {
    return head;
  }
  let mut s = head;
  s.push_str("  printouts:");
  for (label, val) in values {
    s.push(' ');
    s.push_str(label);
    s.push('=');
    s.push_str(&val.render());
  }
  return s;
}

/// Reports the outcome of a one-liner run on stdout/stderr and computes
/// the process exit code.
///
/// Single-cell runs preserve the legacy contract byte-for-byte: the value
/// (or per-cell error) goes to stderr and exactly one of `PASS`/`FAIL`/
/// `ERROR` goes to stdout. Multi-cell runs print the same single-token
/// stdout (`PASS` iff all cells pass, otherwise `FAIL`), an aggregate
/// `flagged=<k>/<n>` line on stderr, and -- with `--verbose` -- one line
/// per cell on stderr.
fn report_oneliner(cells: &[CellResult], verbose: bool) -> ExitCode {
  if cells.len() == 1 {
    let c = &cells[0];
    return match &c.outcome {
      CellOutcome::Pass(v) => {
        eprintln!("{v}");
        println!("PASS");
        ExitCode::SUCCESS
      }
      CellOutcome::Fail(v) => {
        eprintln!("{v}");
        println!("FAIL");
        ExitCode::from(1)
      }
      CellOutcome::Error(e) => {
        eprintln!("{e}");
        println!("ERROR");
        ExitCode::from(error_exit_code(e) as u8)
      }
    };
  }
  // Multi-cell.
  let mut failed = 0usize;
  let mut errored = 0usize;
  for c in cells {
    match &c.outcome {
      CellOutcome::Pass(_) => {}
      CellOutcome::Fail(_) => failed += 1,
      CellOutcome::Error(_) => errored += 1,
    }
    if verbose {
      eprintln!("  - {}", fmt_oneliner_cell(c));
    }
  }
  let total = cells.len();
  let flagged = failed + errored;
  eprintln!("flagged={flagged}/{total}");
  if flagged == 0 {
    println!("PASS");
    return ExitCode::SUCCESS;
  }
  println!("FAIL");
  if errored > 0 {
    return ExitCode::from(2);
  }
  return ExitCode::from(failed.min(254) as u8);
}

/// Formats one [`CellResult`] as a single verbose stderr line.
fn fmt_oneliner_cell(c: &CellResult) -> String {
  let header = format!(
    "subcase={} block={} row={} col={}",
    c.subcase,
    c.block.short_name(),
    c.row,
    c.col,
  );
  return match &c.outcome {
    CellOutcome::Pass(v) => format!("{header} value={v} verdict=PASS"),
    CellOutcome::Fail(v) => format!("{header} value={v} verdict=FAIL"),
    CellOutcome::Error(e) => format!("{header} error=\"{e}\" verdict=ERROR"),
  };
}

/// Prints the row/column index reference for one or all block types.
fn print_indices(filter: &str) {
  let mut printed = 0usize;
  for bt in BlockType::all() {
    if !filter.is_empty()
      && !bt.snake_case_name().eq_ignore_ascii_case(filter)
      && !bt.short_name().eq_ignore_ascii_case(filter)
    {
      continue;
    }
    print!("{}", bt.describe_indices());
    println!();
    printed += 1;
  }
  if printed == 0 {
    eprintln!("no block type matched \"{filter}\"");
  }
}

fn main() -> ExitCode {
  let cli = Cli::parse();
  if let Some(filter) = cli.indices.as_deref() {
    print_indices(filter);
    return ExitCode::SUCCESS;
  }
  if let Some(spec_str) = cli.oneliner.as_deref() {
    let path = match cli.path.as_deref() {
      Some(p) => p,
      None => {
        eprintln!("--oneliner requires an F06 file path");
        println!("ERROR");
        return ExitCode::from(3);
      }
    };
    let spec = match parse_oneliner(spec_str) {
      Ok(s) => s,
      Err(e) => {
        eprintln!("{e}");
        println!("ERROR");
        return ExitCode::from(error_exit_code(&e) as u8);
      }
    };
    return match run_oneliner(&spec, path) {
      Ok(cells) => report_oneliner(&cells, cli.verbose),
      Err(e) => {
        eprintln!("{e}");
        println!("ERROR");
        ExitCode::from(error_exit_code(&e) as u8)
      }
    };
  }
  match cli.path {
    Some(p) => match run_script(p, cli.verbose) {
      Ok(flagged) => {
        // Exit code is the total number of flagged values, capped at 254 so
        // it does not collide with the general-failure code (255 / -1).
        return ExitCode::from(flagged.min(254) as u8);
      }
      Err(e) => {
        eprintln!("{e}");
        // General failure: TOML parse error, F06 parse error, missing file,
        // etc. Exits with -1 (255 as u8).
        return ExitCode::from(255);
      }
    },
    None => {
      eprintln!("No script supplied!");
      return ExitCode::from(255);
    }
  }
}
