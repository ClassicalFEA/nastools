//! This module implements the generic parser for F06 files, and associated
//! structures and enums.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Display;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use log::{debug, error, warn};
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::util::*;

/// A parser might respond this when successfully decoding a line.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ParserResponse {
  /// The line was useless.
  Useless,
  /// The line helped us learn more about the solver.
  Solver(Solver),
  /// This line told us the current subcase.
  Subcase(usize),
  /// The line contained a warning.
  Warning,
  /// The line contained a fatal.
  Fatal,
  /// The line told us whihc kind of solution we're looking at.
  SolType(SolType),
  /// This line contained part of a block header, we're yet to figure out if it
  /// corresponds to a known block or not,
  BlockHeader,
  /// The line told us to start decoding a block.
  BeginBlock(BlockType),
  /// The line was passed to a block decoder.
  PassedToDecoder(BlockType, LineResponse),
  /// The line was a block beginning, but I didn't instantiate a block decoder
  /// because we don't even know the solver yet!
  BeginningWithoutSolver,
  /// This line indicates the beginning of a block we don't even know yet.
  PotentialHeader,
}

/// A cause of crash for the parser.
#[derive(Debug)]
#[non_exhaustive]
pub enum ParserCrash {
  /// A supported solver was not identified by the time a block start was found.
  /// Contains the line where the solver found the block start.
  NoSupportedSolverDetected(usize),
  /// An I/O error was caught.
  IOError(std::io::Error),
}

impl Display for ParserCrash {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      ParserCrash::NoSupportedSolverDetected(i) => write!(
        f,
        "Found a block start on line {i} before knowing the solver!"
      ),
      ParserCrash::IOError(ioe) => ioe.fmt(f),
    };
  }
}

impl From<std::io::Error> for ParserCrash {
  fn from(value: std::io::Error) -> Self {
    return Self::IOError(value);
  }
}

impl Error for ParserCrash {}

/// This is the F06 parser -- it doesn't care how lines are fed into it.
/// It's one-pass, single-thread. There might be a parallel one later.
pub struct OnePassParser {
  /// The current file.
  file: F06File,
  /// The current subcase.
  subcase: usize,
  /// The decoder for block we're currently in.
  current_decoder: Option<Box<dyn OpaqueDecoder>>,
  /// The total number of consumed lines.
  total_lines: usize,
  /// Line of the last block beginning.
  last_block_start: usize,
  /// Accumulator of block header strings.
  header_accumulator: Vec<String>,
  /// Stores last indexes per block type.
  last_indexes: BTreeMap<BlockType, NasIndex>,
}

impl Default for OnePassParser {
  fn default() -> Self {
    return Self::new();
  }
}

impl OnePassParser {
  /// Instantiates a new parser.
  pub fn new() -> Self {
    return Self {
      file: F06File::new(),
      subcase: 1,
      current_decoder: None,
      total_lines: 0,
      last_block_start: 0,
      header_accumulator: Vec::new(),
      last_indexes: BTreeMap::new(),
    };
  }

  /// Hints the parser about the flavour.
  pub fn hint_flavour(&mut self, flavour: Flavour) {
    self.file.flavour.solver = self.file.flavour.solver.or(flavour.solver);
    self.file.flavour.soltype = self.file.flavour.soltype.or(flavour.soltype);
  }

  /// Tries to update the solver in based on a line.
  fn detect_solver(&self, line: &str) -> Option<Solver> {
    if self.file.flavour.solver.is_none() {
      for cand in Solver::all() {
        if line.contains(cand.name()) {
          return Some(*cand);
        }
      }
    }
    return None;
  }

  /// Tries to detect a change in subcase.
  fn detect_subcase(&self, line: &str) -> Option<usize> {
    let bd: Vec<_> = line_breakdown(line).collect();
    if line.contains("OUTPUT FOR SUBCASE")
      || line.contains("OUTPUT FOR EIGENVECTOR")
    {
      return bd
        .into_iter()
        .filter_map(|field| {
          if let LineField::Integer(x) = field {
            return Some(x as usize);
          } else {
            None
          }
        })
        .nth(0);
    }
    if let Some(LineField::Integer(sc)) = bd.last() {
      if let Some(LineField::NoIdea("SUBCASE")) = bd.iter().rev().nth(1) {
        return Some(*sc as usize);
      }
    }
    return None;
  }

  /// Flushes the current block decoder into the file.
  fn flush_decoder(&mut self) {
    if let Some(dec) = self.current_decoder.take() {
      debug!(
        "Finishing up a \"{}\" block on line {}.",
        dec.block_type(),
        self.total_lines
      );
      let line_range = Some((self.last_block_start, self.total_lines + 1));
      if let Some(li) = dec.last_index() {
        self.last_indexes.insert(dec.block_type(), li);
      }

      // if eigen solution and on SC NASTRAN
      let subcase = if self.file.flavour.solver == Some(Solver::Simcenter)
        && self.file.blocks.iter().any(|b| {
          b.0.block_type == BlockType::Eigenvector
            || b.0.block_type == BlockType::RealEigenvalues
        }) {
        // not an eigen-specific block, so order is their subcase
        if dec.block_type() != BlockType::Eigenvector
          || dec.block_type() != BlockType::RealEigenvalues
        {
          let type_count = self
            .file
            .blocks
            .iter()
            .filter(|b| b.0.block_type == dec.block_type())
            .count();
          type_count + 1
        } else {
          self.subcase
        }
      } else {
        self.subcase
      };

      let fb = dec.finalise(subcase, line_range);
      if !fb.row_indexes.is_empty() {
        self.file.insert_block(fb);
      }
    }
  }

  /// Flushes the current block header accumulator.
  fn flush_header(&mut self) -> Option<(String, usize)> {
    if self.header_accumulator.is_empty() {
      return None;
    } else {
      let num = self.header_accumulator.len();
      let full_name = self.header_accumulator.join(" ");
      self.header_accumulator.clear();
      return Some((full_name, num));
    }
  }

  /// Consumes a line into the parser.
  pub fn consume(&mut self, line: &str) -> ParserResponse {
    self.total_lines += 1;
    // first, try and enhance our knowledge of the flavour from the line.
    if let Some(solver) = self.detect_solver(line) {
      self.file.flavour.solver = Some(solver);
      debug!(
        "Line {} told us the solver is {}!",
        self.total_lines, solver
      );
      return ParserResponse::Solver(solver);
    }
    // check for a subcase change
    if let Some(subcase) = self.detect_subcase(line) {
      if self.subcase != subcase {
        // a subcase change definitely means we should stop the block
        self.flush_decoder();
        debug!(
          "Switched from subcase {} to {} on line {}!",
          self.subcase, subcase, self.total_lines
        );
        self.subcase = subcase;
      }
      self.flush_header();
      return ParserResponse::Subcase(subcase);
    }
    // check for warning
    if line.contains("WARNING") {
      debug!("Found warning on line {}: {}", self.total_lines, line);
      self
        .file
        .warnings
        .insert(self.total_lines, line.to_string());
      self.flush_header();
      return ParserResponse::Warning;
    }
    // check for fatal
    if line.contains("FATAL") {
      debug!("Found fatal on line {}: {}", self.total_lines, line);
      self
        .file
        .fatal_errors
        .insert(self.total_lines, line.to_string());
      self.flush_header();
      return ParserResponse::Fatal;
    }
    // check for a block header part.
    if let Some(unspaced) = check_header(line) {
      self.header_accumulator.push(unspaced);
      return ParserResponse::BlockHeader;
    } else if let Some((full_name, num_lines)) = self.flush_header() {
      // not a block header, but we were accumulating one.
      // first, flush the current decoder.
      self.flush_decoder();
      // is it the header of a known block?
      let mut candidates = BlockType::all()
        .iter()
        .copied()
        .filter(|bt| bt.headers().iter().any(|s| full_name.contains(s)))
        .collect::<BTreeSet<_>>();
      match candidates.len() {
        0 => {
          // not a known block. push a potential header.
          // ensure no bad words
          if BAD_WORDS.iter().any(|w| full_name.contains(w)) {
            return ParserResponse::Useless;
          }
          self.file.potential_headers.insert(PotentialHeader {
            start: self.total_lines - num_lines,
            span: num_lines,
            text: full_name,
          });
          debug!(
            "Found a potential header ending in line {}! Flushing.",
            self.total_lines
          );
          return ParserResponse::PotentialHeader;
        }
        1 => {
          let bt = candidates.pop_first().unwrap();
          // do we know the solver?
          if self.file.flavour.solver.is_none() {
            // nope
            error!(
              "Found a block start on line {} before knowing the solver!",
              self.total_lines
            );
            return ParserResponse::BeginningWithoutSolver;
          } else {
            // subcase is on this line for this specific scenario
            if bt == BlockType::Eigenvector
              && self
                .file
                .flavour
                .solver
                .is_some_and(|s| s == Solver::Simcenter)
            {
              // TODO: find a way to propagate both this and PotentialHeader back to the caller
              self.subcase = last_natural(&full_name).unwrap_or(self.subcase);
            }

            // ok, begin the block then.
            let mut dec = bt.init_decoder(self.file.flavour);
            if dec.good_header(&full_name) {
              debug!(
                "Started a \"{}\" block on line {}!",
                bt, self.total_lines
              );
              if let Some(li) = self.last_indexes.remove(&dec.block_type()) {
                dec.hint_last(li);
              }
              self.last_block_start = self.total_lines;
              self.current_decoder = Some(dec);
            } else if !BAD_WORDS.iter().any(|w| full_name.contains(w)) {
              // bad header, whoops.
              self.file.potential_headers.insert(PotentialHeader {
                start: self.total_lines - num_lines,
                span: num_lines,
                text: full_name,
              });
              debug!(
                "Found a potential header ending in line {}! Flushing.",
                self.total_lines
              );
              return ParserResponse::PotentialHeader;
            }
          }
        }
        _ => warn!(
          "Line {} matches more than one block type!",
          self.total_lines
        ),
      }
    }
    // if we got here, the line NOT a block header, and if there was a header
    // being accumulated, it was flushed and the decoder is active.
    // well, is there a current block decoder? if so, pass it the line.
    if let Some(ref mut dec) = self.current_decoder {
      // check for a block-ender
      let resp = if let Some(solver) = self.file.flavour.solver {
        if solver.block_enders().iter().any(|s| line.contains(s))
          && !solver.ender_exceptions().contains(&dec.block_type())
        {
          // line has block ender and block is not exempt from ender
          LineResponse::Done
        } else {
          // no block ender, pass to decoder
          dec.consume(line)
        }
      } else {
        // no solver but we're in a block?!
        self.flush_decoder();
        return ParserResponse::BeginningWithoutSolver;
      };
      let bt = dec.block_type();
      if resp.abnormal() || resp == LineResponse::Done {
        self.flush_decoder();
      }
      return ParserResponse::PassedToDecoder(bt, resp);
    }
    // well, the line was useless then.
    return ParserResponse::Useless;
  }

  /// Finishes up and returns the file struct.
  pub fn finish(mut self) -> F06File {
    self.flush_decoder();
    return self.file;
  }

  /// Parses from a BufRead instance.
  pub fn parse_bufread<R: BufRead>(
    mut reader: R,
  ) -> Result<F06File, ParserCrash> {
    let mut parser = Self::new();
    let mut buf = vec![];
    while reader.read_until(b'\n', &mut buf).is_ok() {
      if buf.is_empty() {
        break;
      }
      buf.pop();
      if buf.ends_with(b"\r") {
        buf.pop();
      }
      let line = String::from_utf8_lossy(&buf);
      let res = parser.consume(&line);
      match res {
        ParserResponse::PassedToDecoder(bt, lr) if lr.abnormal() => warn!(
          "Got abnormal response {:?} from {} while parsing line {}!",
          lr, bt, parser.total_lines
        ),
        ParserResponse::BeginningWithoutSolver => {
          return Err(ParserCrash::NoSupportedSolverDetected(
            parser.total_lines,
          ))
        }
        _ => {}
      }
      buf.clear();
    }
    return Ok(parser.finish());
  }

  /// Utility method -- reads and parses a file.
  pub fn parse_file<S: AsRef<Path>>(p: S) -> Result<F06File, ParserCrash> {
    let file = File::open(p.as_ref())?;
    let mut f06 = Self::parse_bufread(BufReader::new(file))?;
    f06.filename = p
      .as_ref()
      .file_name()
      .and_then(|s| s.to_str())
      .map(String::from);
    return Ok(f06);
  }
}
