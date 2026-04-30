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
  OnelinerOutcome, error_exit_code, parse_oneliner, run_oneliner,
};
use crate::script::Script;

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
  ///
  /// `to` is an inclusive range; `delta` is `[A - B, A + B]` (B >= 0).
  ///
  /// Prints PASS/FAIL/ERROR on stdout and the value (or error) on stderr.
  /// Exit codes: 0 PASS, 1 FAIL, 2 extraction error, 3 spec parse error,
  /// 4 F06 parse error.
  #[arg(long, value_name = "SPEC", conflicts_with = "indices")]
  oneliner: Option<String>,
}

/// Runs a script in a given path and outputs results.
fn run_script<P: AsRef<Path>>(path: P) -> Result<(), Box<dyn Error>> {
  let contents = std::fs::read_to_string(path)?;
  let try_script: Result<Script, TomlError> = toml::from_str(&contents);
  let script = try_script?.prepare()?;
  for comp in script.comparisons.keys() {
    let res = script.run_comparison(comp)?;
    let pass = if res.flagged.is_empty() {
      "PASSED"
    } else {
      "FAILED"
    };
    println!("==> {comp}: {pass}");
    println!("  => checked: {}", res.checked.len());
    println!("  => flagged: {}", res.flagged.len());
  }
  for ck in script.checks.keys() {
    let res = script.run_check(ck)?;
    println!("==> {ck}:");
    for ((f, ex), rp) in res.per_pair.iter() {
      let pass = if rp.flagged.is_empty() {
        "PASSED"
      } else {
        "FAILED"
      };
      let a = rp.flagged.len();
      let b = rp.checked.len();
      println!("  => {f}, {ex}: {pass} ({a}/{b} flagged)")
    }
  }
  if script.comparisons.is_empty() {
    println!("no comparisons in script");
  }
  if script.checks.is_empty() {
    println!("no checks in script");
  }
  return Ok(());
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
      Ok((outcome, value)) => {
        eprintln!("{value}");
        match outcome {
          OnelinerOutcome::Pass => {
            println!("PASS");
            ExitCode::SUCCESS
          }
          OnelinerOutcome::Fail => {
            println!("FAIL");
            ExitCode::from(1)
          }
        }
      }
      Err(e) => {
        eprintln!("{e}");
        println!("ERROR");
        ExitCode::from(error_exit_code(&e) as u8)
      }
    };
  }
  match cli.path {
    Some(p) => {
      if let Err(e) = run_script(p) {
        eprintln!("{e}");
        return ExitCode::from(1);
      }
    }
    None => {
      eprintln!("No script supplied!");
      return ExitCode::from(1);
    }
  }
  return ExitCode::SUCCESS;
}
