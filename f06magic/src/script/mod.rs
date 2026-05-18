//! This module implements the data structures included in scripts.

pub(crate) mod check;
pub(crate) mod comparison;
pub(crate) mod criteria;
pub(crate) mod errors;
pub(crate) mod extraction;
pub(crate) mod index;

use std::collections::{BTreeMap, BTreeSet};

use f06::prelude::*;
use serde::{Deserialize, Serialize};

use crate::script::check::{Check, CheckResult};
use crate::script::comparison::{Comparison, ComparisonResult, FlaggedDetail};
use crate::script::criteria::SimpleCriteria;
use crate::script::errors::{
  CheckRunError, ComparisonRunError, ScriptValidationError,
};
use crate::script::extraction::SimpleExtraction;

/// An f06magic script. Contains decks, extractions, criteria, and tests.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Script {
  /// The files used in this script.
  pub(crate) files: BTreeMap<String, String>,
  /// The extractions within this script.
  #[serde(alias = "extraction")]
  pub(crate) extractions: Vec<SimpleExtraction>,
  /// The comparison criteria within this script.
  #[serde(alias = "criterion")]
  pub(crate) criteria: Vec<SimpleCriteria>,
  /// The comparisons within this script.
  #[serde(alias = "comparison")]
  pub(crate) comparisons: Vec<Comparison>,
  /// The checks within this script.
  #[serde(alias = "check")]
  pub(crate) checks: Vec<Check>,
}

impl Script {
  /// Prepares a script for running: parses F06s and resolves names.
  pub(crate) fn prepare(self) -> Result<ReadyScript, ScriptPrepareError> {
    let mut files: BTreeMap<String, F06File> = BTreeMap::new();
    for (n, p) in self.files {
      let read = OnePassParser::parse_file(&p)?;
      files.insert(n, read);
    }
    let mut extractions: BTreeMap<String, Extraction> = BTreeMap::new();
    for simple in self.extractions {
      let name = simple.name.clone();
      let resolved = simple.resolve()?;
      extractions.insert(name, resolved);
    }
    return Ok(ReadyScript {
      files,
      extractions,
      criteria: self
        .criteria
        .into_iter()
        .map(|c| (c.name.clone(), c))
        .collect(),
      comparisons: self
        .comparisons
        .into_iter()
        .map(|c| (c.name.clone(), c))
        .collect(),
      checks: self
        .checks
        .into_iter()
        .map(|c| (c.name.clone(), c))
        .collect(),
    });
  }
}

/// Errors during [`Script::prepare`].
#[derive(Debug)]
pub(crate) enum ScriptPrepareError {
  /// One of the F06 files failed to parse.
  Parse(ParserCrash),
  /// One of the simple extractions could not be resolved.
  Validation(ScriptValidationError),
}

impl From<ParserCrash> for ScriptPrepareError {
  fn from(e: ParserCrash) -> Self {
    return Self::Parse(e);
  }
}

impl From<ScriptValidationError> for ScriptPrepareError {
  fn from(e: ScriptValidationError) -> Self {
    return Self::Validation(e);
  }
}

impl std::fmt::Display for ScriptPrepareError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return match self {
      Self::Parse(e) => write!(f, "{e}"),
      Self::Validation(e) => write!(f, "{e}"),
    };
  }
}

impl std::error::Error for ScriptPrepareError {}

/// A script that is ready to run after names having been resolved and F06 files
/// having been parsed.
pub(crate) struct ReadyScript {
  /// The files used in this script.
  pub(crate) files: BTreeMap<String, F06File>,
  /// The extractions within this script.
  pub(crate) extractions: BTreeMap<String, Extraction>,
  /// The comparison criteria within this script.
  pub(crate) criteria: BTreeMap<String, SimpleCriteria>,
  /// The comparisons within this script.
  pub(crate) comparisons: BTreeMap<String, Comparison>,
  /// The checks within this script.
  pub(crate) checks: BTreeMap<String, Check>,
}

impl ReadyScript {
  /// Runs a single comparison.
  pub(crate) fn run_comparison(
    &self,
    name: &str,
  ) -> Result<ComparisonResult, ComparisonRunError> {
    // get the comparison
    let comparison = self
      .comparisons
      .get(name)
      .ok_or(ComparisonRunError::ComparisonNotFound(name.to_string()))?;
    // get the reference f06
    let ref_name = &comparison.reference_f06;
    let ref_file = self
      .files
      .get(ref_name)
      .ok_or(ComparisonRunError::FileNotFound(ref_name.to_string()))?;
    // get the test f06
    let test_name = &comparison.test_f06;
    let test_file = self
      .files
      .get(test_name)
      .ok_or(ComparisonRunError::FileNotFound(test_name.to_string()))?;
    // get the criteria
    let crit_name = &comparison.criteria;
    let criteria: Criteria = self
      .criteria
      .get(crit_name)
      .ok_or(ComparisonRunError::CriteriaNotFound(crit_name.clone()))?
      .clone()
      .into();
    let mut indices: BTreeSet<DatumIndex> = BTreeSet::new();
    for en in comparison.extractions.clone().into_iter() {
      let ex = self
        .extractions
        .get(&en)
        .ok_or(ComparisonRunError::ExtractionNotFound(en.clone()))?;
      indices.extend(ex.lookup(ref_file));
      indices.extend(ex.lookup(test_file));
    }
    let mut flagged: BTreeMap<DatumIndex, FlaggedDetail> = BTreeMap::new();
    for i in indices.iter() {
      let ref_val = i.get_from(ref_file).unwrap_or(F06Number::Real(0.0));
      let test_val = i.get_from(test_file).unwrap_or(F06Number::Real(0.0));
      if let Some(reason) = criteria.check(ref_val.into(), test_val.into()) {
        flagged.insert(
          *i,
          FlaggedDetail {
            ref_val,
            test_val,
            reason,
          },
        );
      }
    }
    return Ok(ComparisonResult {
      checked: indices,
      flagged,
    });
  }

  /// Runs a single check.
  pub(crate) fn run_check(
    &self,
    name: &str,
  ) -> Result<CheckResult, CheckRunError> {
    let mut results = CheckResult::default();
    let check = self
      .checks
      .get(name)
      .ok_or(CheckRunError::CheckNotFound(name.to_string()))?;
    // get the files and extractions
    let f06_names = &check.files;
    let extractions_names = &check.extractions;
    for f06_name in f06_names.into_iter() {
      for en in extractions_names.into_iter() {
        let f06 = self
          .files
          .get(f06_name)
          .ok_or(CheckRunError::FileNotFound(f06_name.to_string()))?;
        let ex = self
          .extractions
          .get(en)
          .ok_or(CheckRunError::ExtractionNotFound(en.clone()))?;
        let nums = ex.lookup(f06).map(|di| (di, di.get_from(f06).unwrap()));
        let pres = check.run_for(nums);
        results
          .per_pair
          .insert((f06_name.to_owned(), en.to_owned()), pres);
      }
    }
    return Ok(results);
  }
}
