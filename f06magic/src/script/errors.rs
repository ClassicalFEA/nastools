//! Contains error types for scripts and their runnings.

use std::error::Error;
use std::fmt::Display;

use f06::prelude::ParseLenientError;

use crate::script::equation::EquationError;
use crate::script::index::IndexAxis;

/// Errors raised when a script is being prepared (i.e. simple extractions
/// resolved into real ones).
#[derive(Debug)]
pub(crate) enum ScriptValidationError {
  /// A row/col entry could not be parsed into any known index value.
  IndexParse {
    /// The name of the offending extraction.
    extraction: String,
    /// Which axis (row or column) the entry came from.
    axis: IndexAxis,
    /// The raw script string.
    raw: String,
    /// The underlying parse error.
    cause: ParseLenientError,
  },
  /// A row/col entry parsed fine but its type does not match any of the
  /// configured block(s).
  IndexKindMismatch {
    /// The name of the offending extraction.
    extraction: String,
    /// Which axis (row or column) the entry came from.
    axis: IndexAxis,
    /// The raw script string.
    raw: String,
    /// The all-caps type name that was actually parsed.
    got: &'static str,
    /// The all-caps type names that the configured blocks accept.
    expected: Vec<String>,
  },
  /// A `[[check]]` or `[[comparison]]` table contains an `equation` field
  /// that could not be parsed (or referenced an out-of-scope variable).
  Equation {
    /// Whether the equation came from a check or a comparison.
    kind: &'static str,
    /// Name of the offending check/comparison.
    name: String,
    /// Underlying equation error.
    cause: EquationError,
  },
}

impl Display for ScriptValidationError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::IndexParse {
        extraction,
        axis,
        raw,
        cause,
      } => write!(
        f,
        "extraction \"{extraction}\": cannot parse {axis:?} index \"{raw}\": {cause}",
      ),
      Self::IndexKindMismatch {
        extraction,
        axis,
        raw,
        got,
        expected,
      } => write!(
        f,
        "extraction \"{extraction}\": {axis:?} index \"{raw}\" is a {got}, \
         but the configured block(s) expect one of: {}",
        expected.join(", ")
      ),
      Self::Equation { kind, name, cause } => write!(
        f,
        "{kind} \"{name}\": {cause}",
      ),
    };
  }
}

impl Error for ScriptValidationError {}

/// Errors when running comparisons.
#[derive(Debug)]
pub(crate) enum ComparisonRunError {
  /// Could not find an extraction with a given name.
  ExtractionNotFound(String),
  /// Could not find a comparison criteria set with a given name.
  CriteriaNotFound(String),
  /// Could not find a file with the given name.
  FileNotFound(String),
  /// Could not find a comparison with a given name.
  ComparisonNotFound(String),
  /// Some other error
  AnotherError(Box<dyn Error>),
}

/// Errors when running checks.
#[derive(Debug)]
pub(crate) enum CheckRunError {
  /// Could not find an extraction with a given name.
  ExtractionNotFound(String),
  /// Could not find an extraction with a given name.
  CheckNotFound(String),
  /// Could not find a file with the given name.
  FileNotFound(String),
  /// Some other error
  AnotherError(Box<dyn Error>),
}

impl Display for ComparisonRunError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Self::AnotherError(e) = self {
      return e.fmt(f);
    } else {
      return write!(f, "{self:?}");
    }
  }
}

impl Display for CheckRunError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Self::AnotherError(e) = self {
      return e.fmt(f);
    } else {
      return write!(f, "{self:?}");
    }
  }
}

impl Error for ComparisonRunError {}

impl Error for CheckRunError {}
