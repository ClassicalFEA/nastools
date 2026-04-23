//! This defines subroutines to run decks and do test runs.

use core::fmt::Display;
use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use f06::prelude::*;
use serde::{Deserialize, Serialize};
use subprocess::{ExitStatus, Popen, PopenConfig, PopenError};
use uuid::Uuid;

use crate::results::{DeckResults, RunState};
use crate::suite::*;

#[cfg(target_os = "macos")]
/// Extensions for binary files.
pub(crate) const BINARY_EXTENSIONS: &[&str] = &[];
#[cfg(target_os = "linux")]
/// Extensions for binary files.
pub(crate) const BINARY_EXTENSIONS: &[&str] = &[];
#[cfg(target_os = "windows")]
/// Extensions for binary files.
pub(crate) const BINARY_EXTENSIONS: &[&str] = &["exe"];

/// Lower-case F06 extension.
const F06_LOWER: &str = "f06";

/// Upper-case F06 extension.
const F06_UPPER: &str = "F06";

/// This is how we run a solver, if at all, to acquire an F06 file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum RunMethod {
  /// The F06 file is directly imported from a directory, containing the .F06
  /// files with the same base name as the decks.
  ImportFromDir(PathBuf),
  /// A solver is run passing the deck as an argument, and the F06 is got from
  /// reading the same
  RunSolver(PathBuf),
}

/// These are the errors that can come up when running a solver to get the F06
/// output.
#[derive(Debug, derive_more::From)]
pub(crate) enum RunError {
  /// The F06 was not found at its supposed location.
  MissingF06(PathBuf),
  /// The F06 was found but could not be read.
  UnreadableF06(PathBuf, Box<dyn Error>),
  /// The solver binary is missing.
  MissingSolver(PathBuf),
  /// The solver subprocess exited with a non-zero code.
  SolverFailed(String, Option<u32>),
  /// Couldn't process a path to get basenames, extensions, etc.
  PathError,
  /// Two matching files exist in the directory.
  ExtensionMixup,
  /// The F06 parser failed for some reason.
  #[from]
  ParserCrash(ParserCrash),
  /// Unidentified I/O error.
  #[from]
  IoError(std::io::Error),
  /// Couldn't create a temp dir.
  TempdirCreationFailed,
  /// Coulndn't spawn a subprocess.
  #[from]
  SubprocessFailed(PopenError),
}

impl Display for RunError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      RunError::MissingF06(p) => {
        write!(f, "missing F06 file at {}", p.display())
      }
      RunError::UnreadableF06(p, e) => write!(
        f,
        "could not read F06 file at {}, reason: {}",
        p.display(),
        e
      ),
      RunError::MissingSolver(p) => {
        write!(f, "missing solver binary at {}", p.display())
      }
      RunError::SolverFailed(s, Some(c)) => {
        write!(f, "solver \"{}\" finished with non-zero exit code {}", s, c)
      }
      RunError::SolverFailed(s, None) => write!(
        f,
        "solver \"{}\" failed to run and didn't even return a code",
        s,
      ),
      RunError::PathError => write!(f, "path processing error"),
      RunError::ExtensionMixup => write!(f, "both .f06 and .F06 exist"),
      RunError::ParserCrash(pc) => write!(f, "Parser failed: {}", pc),
      RunError::IoError(ioe) => write!(f, "I/O error: {}", ioe),
      RunError::TempdirCreationFailed => write!(f, "tempdir creation failed"),
      RunError::SubprocessFailed(e) => write!(f, "subprocess error: {}", e),
    };
  }
}

impl RunError {
  /// Returns a shorter error message.
  pub(crate) fn short_msg(&self) -> &'static str {
    return match self {
      RunError::MissingF06(_) => "missing F06 file",
      RunError::UnreadableF06(_, _) => "couldn't read F06 file",
      RunError::MissingSolver(_) => "missing solver binary",
      RunError::SolverFailed(_, Some(_)) => "solver failed to run",
      RunError::SolverFailed(_, None) => "solver crashed",
      RunError::PathError => "path processing error",
      RunError::ExtensionMixup => "both .f06 and .F06 exist",
      RunError::ParserCrash(_) => "parser failed",
      RunError::IoError(_) => "i/o error",
      RunError::TempdirCreationFailed => "tempdir creation failed",
      RunError::SubprocessFailed(_) => "subprocess error",
    };
  }
}

/// This is a named "F06 acquisition method". A solver, for short.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RunnableSolver {
  /// The kind of solver. Must be supported by the F06 library.
  pub(crate) kind: Solver,
  /// The "nickname" for this solver, so you can tell versions apart.
  pub(crate) nickname: String,
  /// The method through which we actually get an F06.
  pub(crate) method: RunMethod,
}

impl RunnableSolver {
  /// Runs this solver and returns an F06 output.
  pub(crate) fn make_f06(&self, deck: &Deck) -> Result<F06File, RunError> {
    let basename = deck.in_file.file_name().ok_or(RunError::PathError)?;
    /// This function extracts the F06 file from a directory.
    fn do_dir(d: &Path, basename: &OsStr) -> Result<F06File, RunError> {
      let mut lower = d.join(basename);
      lower.set_extension(F06_LOWER);
      let mut upper = d.join(basename);
      upper.set_extension(F06_UPPER);
      let f06path = match (lower.exists(), upper.exists()) {
        (true, true) => {
          // are we on a stupid system with stupid case-insensitive files?
          if cfg!(windows) {
            // sure why not, return the upper-case
            upper
          } else {
            // ehh, if both exist and this isn't windows, something went badly
            return Err(RunError::ExtensionMixup);
          }
        }
        (false, false) => return Err(RunError::MissingF06(d.to_path_buf())),
        (true, false) => lower,
        (false, true) => upper,
      };
      let mut file = f06::parser::OnePassParser::parse_file(f06path)?;
      file.merge_blocks(true);
      file.sort_all_blocks();
      return Ok(file);
    }
    match &self.method {
      RunMethod::ImportFromDir(d) => return do_dir(d, basename),
      RunMethod::RunSolver(bin) => {
        let tmp = tempfile::TempDir::with_prefix("nastester_run_")
          .map_err(|_| RunError::TempdirCreationFailed)?;
        let file_in_tmp = |name: &Path| -> PathBuf {
          let mut subfile = tmp.path().to_path_buf();
          subfile.push(name);
          return subfile;
        };
        let stdout = File::create(file_in_tmp("stdout.log".as_ref()))?;
        let stderr = File::create(file_in_tmp("stderr.log".as_ref()))?;
        let pc = PopenConfig {
          stdin: subprocess::Redirection::Pipe,
          stdout: subprocess::Redirection::File(stdout),
          stderr: subprocess::Redirection::File(stderr),
          executable: Some(bin.clone().into_os_string()),
          cwd: Some(tmp.path().as_os_str().to_owned()),
          ..Default::default()
        };
        let tmp_deck = file_in_tmp(deck.name().as_ref());
        std::fs::copy(&deck.in_file, &tmp_deck)?;
        let mut proc = Popen::create(&[bin, &tmp_deck], pc)?;
        proc.detach();
        //let code = proc.wait_timeout(Duration::from_secs(60));
        let code = proc.wait();
        let res = match code {
          Ok(ExitStatus::Exited(0)) => do_dir(tmp.path(), basename),
          Ok(ExitStatus::Exited(i)) => {
            return Err(RunError::SolverFailed(self.nickname.clone(), Some(i)))
          }
          _ => return Err(RunError::SolverFailed(self.nickname.clone(), None)),
        };
        if res.is_err() {
          dbg!(&res);
        }
        std::mem::drop(tmp);
        return res;
      }
    };
  }
}

/// A pick of solver for a job. Sugar.
#[derive(
  Copy,
  Clone,
  Debug,
  Serialize,
  Deserialize,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
)]
pub(crate) enum SolverPick {
  /// The reference solver.
  Reference,
  /// The solver under test.
  Testing,
}

impl SolverPick {
  /// Returns all variants.
  pub(crate) fn all() -> &'static [Self] {
    return &[Self::Reference, Self::Testing];
  }
}

/// This specifies a run job.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Job {
  /// The deck this is for.
  pub(crate) deck: Deck,
  /// The solver to use.
  pub(crate) solver: RunnableSolver,
  /// The pick of solver for the job.
  pub(crate) pick: SolverPick,
  /// The target to write results to.
  pub(crate) target: Arc<Mutex<DeckResults>>,
  /// A copy of the crit-sets at the instant of job creation.
  pub(crate) crit_sets: BTreeMap<Uuid, NamedCriteria>,
}

impl Job {
  /// Runs this job. This blocks! Careful.
  pub(crate) fn run(&self) {
    let mut h = self.target.lock().expect("mutex poisoned");
    *h.get_mut(self.pick) = RunState::Running;
    let res = self.solver.make_f06(&self.deck).map_err(|e| e.to_string());
    *h.get_mut(self.pick) = res.into();
    h.recompute_extractions(&self.deck, &self.crit_sets)
  }
}

/// This contains everything needed to run decks, and locks stuff.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Runner {
  /// The currently-selected reference solver, if any.
  pub(crate) ref_solver: Option<Uuid>,
  /// The currently-selected solver under test, if any.
  pub(crate) test_solver: Option<Uuid>,
  /// The results currently loaded for the decks.
  pub(crate) results: BTreeMap<Uuid, Arc<Mutex<DeckResults>>>,
  /// Runs in queue.
  pub(crate) job_queue: Arc<Mutex<VecDeque<Job>>>,
  /// Max concurrent jobs. If zero, auto-detect.
  pub(crate) max_jobs: usize,
  /// Current number of jobs running.
  pub(crate) current_jobs: Arc<AtomicUsize>,
}

impl Runner {
  /// Returns the UUID of a solver pick.
  pub(crate) fn get_solver(&self, pick: SolverPick) -> Option<Uuid> {
    return match pick {
      SolverPick::Reference => self.ref_solver,
      SolverPick::Testing => self.test_solver,
    };
  }
}
