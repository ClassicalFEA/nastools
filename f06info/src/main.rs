//! Dumps information on an F06 file, such as its blocks, etc.

#![allow(clippy::needless_return)] // i'll never forgive rust for this
#![allow(dead_code)] // temporary

use std::collections::BTreeMap;
use std::io::{self, BufReader};
use std::path::PathBuf;

use clap::Parser;
use f06::prelude::*;
use f06::util::PotentialHeader;
use log::{error, info, LevelFilter};

#[derive(Parser)]
#[command(author, version)]
struct Cli {
  /// Disable block merging.
  #[arg(short = 'M', long)]
  no_merge: bool,
  /// Output extra/debug info while parsing.
  #[arg(short, long)]
  verbose: bool,
  /// File path (set to "-" to read from standard input).
  file: PathBuf,
}

const INDENT: &str = "  ";

fn main() -> Result<(), ParserCrash> {
  // init cli stuff
  let args = Cli::parse();
  let log_level = if args.verbose {
    LevelFilter::Debug
  } else {
    LevelFilter::Info
  };
  env_logger::builder().filter_level(log_level).init();
  // parse the file
  let mut f06: F06File = if args.file.as_os_str().eq_ignore_ascii_case("-") {
    OnePassParser::parse_bufread(BufReader::new(io::stdin()))?
  } else if args.file.is_file() {
    if let Some(bn) = args.file.file_name() {
      if let Some(sbn) = bn.to_str() {
        info!("Parsing {sbn}...");
      }
    } else {
      info!("Parsing...");
    }
    OnePassParser::parse_file(&args.file)?
  } else {
    error!("Provided path either does not exist or is not a file!");
    std::process::exit(1);
  };
  // print block & merge info
  info!("Done parsing.");
  let solver_name = f06.flavour.solver.map_or("unknown", |s| s.name());
  let soltype = f06.flavour.soltype.map_or("unknown", |st| st.name());
  info!("Solver is {solver_name}.");
  info!("Analysis type is {soltype}.");
  // print warnings
  if f06.warnings.is_empty() {
    info!("No warnings found.");
  } else {
    info!("The following warnings were found:");
    for (line, text) in f06.warnings.iter() {
      info!("{INDENT}- Line {line}: {text}");
    }
  }
  // print fatals
  if f06.fatal_errors.is_empty() {
    info!("No fatal errors found.");
  } else {
    info!("The following fatal errors were found:");
    for (line, text) in f06.fatal_errors.iter() {
      info!("{INDENT}- Line {line}: {text}");
    }
  }
  // print merge/block info
  if f06.blocks.is_empty() {
    info!("No supported blocks were found.");
  } else {
    let nmerges = if args.no_merge {
      0
    } else {
      info!("Merging blocks...");
      f06.merge_blocks(true)
    };
    if nmerges == 0 {
      info!("Merged no blocks, stayed with {}.", f06.blocks.len());
    } else {
      info!(
        "Did {} block merges, now there are {}.",
        nmerges,
        f06.blocks.len()
      );
    };
    info!("Supported blocks found:");
    for subcase in f06.subcases() {
      info!("{INDENT}- Subcase {subcase}:");
      for block in f06.block_search(None, Some(subcase), false) {
        info!(
          "{}{}- {}: {} rows, {} columns",
          INDENT,
          INDENT,
          block.block_type,
          block.row_indices.len(),
          block.col_indices.len()
        );
      }
    }
  }
  if f06.potential_headers.is_empty() {
    info!("No potential headers for unsupported blocks were found.");
  } else {
    f06.merge_potential_headers();
    info!("Some potential headers for unsupported blocks were found:");
    let mut headers = f06
      .potential_headers
      .iter()
      .map(|ph| (ph.text.as_str(), Vec::new()))
      .collect::<BTreeMap<&str, Vec<&PotentialHeader>>>();
    f06.potential_headers.iter().for_each(|ph| {
      if let Some(v) = headers.get_mut(ph.text.as_str()) {
        v.push(ph)
      }
    });
    for (txt, occurrences) in headers {
      let ntimes = occurrences.len();
      let ph = occurrences.first().unwrap();
      let countlines = match ph.span {
        0 => panic!("header spanning 0 lines?!"),
        1 => format!("ine {}", ph.start),
        2 => format!("ines {} and {}", ph.start, ph.lines().last().unwrap()),
        _ => format!("ines {}-{}", ph.start, ph.lines().last().unwrap()),
      };
      info!("{INDENT}- L{countlines}: \"{txt}\"");
      if ntimes > 1 {
        info!(
          "{}{}- (other {} occurences omitted)",
          INDENT,
          INDENT,
          ntimes - 1
        );
      }
    }
  }
  return Ok(());
}
