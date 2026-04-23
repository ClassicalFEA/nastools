//! This program is meant as a successor to f06diff and a command-line based
//! replacement for nastester. It consumes a "script", which is just a TOML file
//! containing a series of tests to do on one or more F06 files, and generates
//! a report.

#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]
#![allow(clippy::needless_return)]
#![allow(dead_code)]

pub(crate) mod script;
pub(crate) mod utils;

use std::error::Error;
use std::path::Path;

use toml::de::Error as TomlError;

use crate::script::Script;

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
  if script.comparisons.is_empty() {
    println!("no comparisons in script");
  }
  return Ok(());
}

fn main() {
  if let Some(p) = std::env::args().nth(1) {
    if let Err(e) = run_script(p) {
      eprintln!("{e}");
    }
  } else {
    eprintln!("No script supplied!");
  }
}
