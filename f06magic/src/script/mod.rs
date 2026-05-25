//! This module implements the data structures included in scripts.

pub(crate) mod check;
pub(crate) mod comparison;
pub(crate) mod criteria;
pub(crate) mod equation;
pub(crate) mod errors;
pub(crate) mod extraction;
pub(crate) mod index;

use std::collections::{BTreeMap, BTreeSet};

use f06::prelude::*;
use serde::{Deserialize, Serialize};

use crate::script::check::{Check, CheckResult};
use crate::script::comparison::{
  Comparison, ComparisonResult, FlagReason2, FlaggedDetail,
};
use crate::script::criteria::SimpleCriteria;
use crate::script::equation::{Equation, EvalOutcome, Scope, Stats};
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
    // Parse equations, if any, into Equation objects keyed by check /
    // comparison name. Empty/missing equations stay absent from these maps.
    let mut check_equations: BTreeMap<String, Equation> = BTreeMap::new();
    for c in &self.checks {
      if let Some(raw) = c.equation.as_deref()
        && !raw.trim().is_empty()
      {
        let eq = Equation::parse(raw, Scope::Check).map_err(|e| {
          ScriptValidationError::Equation {
            kind: "check",
            name: c.name.clone(),
            cause: e,
          }
        })?;
        check_equations.insert(c.name.clone(), eq);
      }
    }
    let mut comparison_equations: BTreeMap<String, Equation> = BTreeMap::new();
    for c in &self.comparisons {
      if let Some(raw) = c.equation.as_deref()
        && !raw.trim().is_empty()
      {
        let eq = Equation::parse(raw, Scope::Comparison).map_err(|e| {
          ScriptValidationError::Equation {
            kind: "comparison",
            name: c.name.clone(),
            cause: e,
          }
        })?;
        comparison_equations.insert(c.name.clone(), eq);
      }
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
      check_equations,
      comparison_equations,
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
  /// Parsed equations for checks (keyed by check name). Absent when the
  /// check has no `equation` field.
  pub(crate) check_equations: BTreeMap<String, Equation>,
  /// Parsed equations for comparisons (keyed by comparison name).
  pub(crate) comparison_equations: BTreeMap<String, Equation>,
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
    // If an equation is attached, precompute per-side stats over the
    // union of indices. Missing-in-one-file readings substitute 0.0
    // (matching the same convention as the criteria pass below).
    let equation = self.comparison_equations.get(name);
    let (ref_stats, test_stats): (Option<Stats>, Option<Stats>) =
      if equation.is_some() {
        let ref_vals: Vec<f64> = indices
          .iter()
          .map(|i| i.get_from(ref_file).unwrap_or(F06Number::Real(0.0)).into())
          .collect();
        let test_vals: Vec<f64> = indices
          .iter()
          .map(|i| i.get_from(test_file).unwrap_or(F06Number::Real(0.0)).into())
          .collect();
        let rs = Stats::from_values(ref_vals);
        let ts = Stats::from_values(test_vals);
        if rs.is_none() {
          return Err(ComparisonRunError::EmptyEquationPool {
            name: name.to_owned(),
            side: "reference",
          });
        }
        if ts.is_none() {
          return Err(ComparisonRunError::EmptyEquationPool {
            name: name.to_owned(),
            side: "test",
          });
        }
        (rs, ts)
      } else {
        (None, None)
      };
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
            reason: FlagReason2::Criteria(reason),
          },
        );
        continue;
      }
      if let (Some(eq), Some(rs), Some(ts)) =
        (equation, ref_stats.as_ref(), test_stats.as_ref())
      {
        let xv: f64 = test_val.into();
        let yv: f64 = ref_val.into();
        match eq.evaluate(xv, Some(yv), ts, Some(rs)) {
          EvalOutcome::Pass { .. } => {}
          EvalOutcome::Fail { value } => {
            flagged.insert(
              *i,
              FlaggedDetail {
                ref_val,
                test_val,
                reason: FlagReason2::Equation {
                  raw: eq.raw().to_owned(),
                  value,
                  error: None,
                },
              },
            );
          }
          EvalOutcome::Error { message } => {
            flagged.insert(
              *i,
              FlaggedDetail {
                ref_val,
                test_val,
                reason: FlagReason2::Equation {
                  raw: eq.raw().to_owned(),
                  value: f64::NAN,
                  error: Some(message),
                },
              },
            );
          }
        }
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
    let equation = self.check_equations.get(name);
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
        let collected: Vec<(DatumIndex, F06Number)> = ex
          .lookup(f06)
          .map(|di| (di, di.get_from(f06).unwrap()))
          .collect();
        let stats: Option<Stats> = if equation.is_some() {
          let s =
            Stats::from_values(collected.iter().map(|(_, n)| f64::from(*n)));
          if s.is_none() {
            return Err(CheckRunError::EmptyEquationPool {
              name: name.to_owned(),
              file: f06_name.to_owned(),
              extraction: en.to_owned(),
            });
          }
          s
        } else {
          None
        };
        let pres = check.run_for(collected, equation, stats.as_ref());
        results
          .per_pair
          .insert((f06_name.to_owned(), en.to_owned()), pres);
      }
    }
    return Ok(results);
  }
}
