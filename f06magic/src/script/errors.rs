//! Contains error types for scripts and their runnings.

use std::error::Error;
use std::fmt::Display;

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
