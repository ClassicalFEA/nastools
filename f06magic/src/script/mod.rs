//! This module implements the data structures included in scripts.

pub(crate) mod comparison;
pub(crate) mod criteria;
pub(crate) mod errors;
pub(crate) mod extraction;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Result as IoResult;

use f06::prelude::*;
use serde::{Deserialize, Serialize};

use crate::script::comparison::Comparison;
use crate::script::criteria::SimpleCriteria;
use crate::script::errors::ComparisonRunError;
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
}

impl Script {
  /// Prepares a script for running: parses F06s and resolves names.
  pub(crate) fn prepare(self) -> Result<ReadyScript, ParserCrash> {
    let mut files: BTreeMap<String, F06File> = BTreeMap::new();
    for (n, p) in self.files {
      let read = OnePassParser::parse_file(&p)?;
      files.insert(n, read);
    }
    return Ok(ReadyScript {
      files,
      extractions: self
        .extractions
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect(),
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
    });
  }
}

/// A script that is ready to run after names having been resolved and F06 files
/// having been parsed.
pub(crate) struct ReadyScript {
  /// The files used in this script.
  pub(crate) files: BTreeMap<String, F06File>,
  /// The extractions within this script.
  pub(crate) extractions: BTreeMap<String, SimpleExtraction>,
  /// The comparison criteria within this script.
  pub(crate) criteria: BTreeMap<String, SimpleCriteria>,
  /// The comparisons within this script.
  pub(crate) comparisons: BTreeMap<String, Comparison>,
}

/// The results from a run.
pub(crate) struct ComparisonResult {
  /// Indices checked.
  pub(crate) checked: BTreeSet<DatumIndex>,
  /// Indices flagged.
  pub(crate) flagged: BTreeSet<DatumIndex>,
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
      let ex: Extraction = self
        .extractions
        .get(&en)
        .ok_or(ComparisonRunError::ExtractionNotFound(en.clone()))?
        .clone()
        .into();
      indices.extend(ex.lookup(ref_file));
      indices.extend(ex.lookup(test_file));
    }
    let mut flagged: BTreeSet<DatumIndex> = BTreeSet::new();
    for i in indices.iter() {
      let ref_val = i.get_from(ref_file).unwrap_or(F06Number::Real(0.0));
      let test_val = i.get_from(test_file).unwrap_or(F06Number::Real(0.0));
      if criteria.check(ref_val.into(), test_val.into()).is_some() {
        flagged.insert(*i);
      }
    }
    return Ok(ComparisonResult {
      checked: indices,
      flagged,
    });
  }
}
