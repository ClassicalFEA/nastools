//! This module implements the data structures included in scripts.

pub(crate) mod check;
pub(crate) mod comparison;
pub(crate) mod criteria;
pub(crate) mod predicate;
pub(crate) mod errors;
pub(crate) mod extraction;
pub(crate) mod index;
pub(crate) mod printout;

use std::collections::{BTreeMap, BTreeSet};

use f06::prelude::*;
use serde::{Deserialize, Serialize};

use crate::script::check::{Check, CheckResult};
use crate::script::comparison::{
  Comparison, ComparisonResult, EmptySide, FlagReason2, FlaggedDetail,
};
use crate::script::criteria::SimpleCriteria;
use crate::script::predicate::{Predicate, EvalOutcome, Scope, Stats};
use crate::script::errors::{
  CheckRunError, ComparisonRunError, ScriptValidationError,
};
use crate::script::extraction::{ExtractionFlags, SimpleExtraction};
use crate::script::printout::{
  PrintoutValue, ResolvedPrintout, SimplePrintout,
};

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
  /// Named bags of debug expressions referenced by checks/comparisons.
  #[serde(alias = "printout")]
  pub(crate) printouts: Vec<SimplePrintout>,
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
    let mut extraction_flags: BTreeMap<String, ExtractionFlags> =
      BTreeMap::new();
    for simple in self.extractions {
      let name = simple.name.clone();
      let flags = simple.flags();
      let resolved = simple.resolve()?;
      extractions.insert(name.clone(), resolved);
      extraction_flags.insert(name, flags);
    }
    // Parse predicates, if any, into Predicate objects keyed by check /
    // comparison name. Empty/missing predicates stay absent from these maps.
    let mut check_predicates: BTreeMap<String, Predicate> = BTreeMap::new();
    for c in &self.checks {
      if let Some(raw) = c.predicate.as_deref()
        && !raw.trim().is_empty()
      {
        let eq = Predicate::parse(raw, Scope::Check).map_err(|e| {
          ScriptValidationError::Predicate {
            kind: "check",
            name: c.name.clone(),
            cause: Box::new(e),
          }
        })?;
        check_predicates.insert(c.name.clone(), eq);
      }
    }
    let mut comparison_predicates: BTreeMap<String, Predicate> = BTreeMap::new();
    for c in &self.comparisons {
      if let Some(raw) = c.predicate.as_deref()
        && !raw.trim().is_empty()
      {
        let eq = Predicate::parse(raw, Scope::Comparison).map_err(|e| {
          ScriptValidationError::Predicate {
            kind: "comparison",
            name: c.name.clone(),
            cause: Box::new(e),
          }
        })?;
        comparison_predicates.insert(c.name.clone(), eq);
      }
    }
    // Build the printout-definitions map, refusing duplicate names.
    let mut printout_defs: BTreeMap<String, BTreeMap<String, String>> =
      BTreeMap::new();
    for p in &self.printouts {
      if printout_defs.contains_key(&p.name) {
        return Err(
          ScriptValidationError::PrintoutDuplicateName {
            name: p.name.clone(),
          }
          .into(),
        );
      }
      printout_defs.insert(p.name.clone(), p.vars.clone());
    }
    // Resolve printouts referenced by each check / comparison under that
    // site's scope. Reject collisions when two referenced printouts
    // share a label.
    let mut check_printouts: BTreeMap<String, Vec<ResolvedPrintout>> =
      BTreeMap::new();
    for c in &self.checks {
      let refs = match &c.printouts {
        Some(r) => r,
        None => continue,
      };
      let mut resolved: Vec<ResolvedPrintout> = Vec::new();
      let mut seen_labels: BTreeSet<String> = BTreeSet::new();
      for pname in refs.into_iter() {
        let defs = printout_defs.get(pname).ok_or_else(|| {
          ScriptValidationError::PrintoutNotFound {
            kind: "check",
            site: c.name.clone(),
            printout: pname.clone(),
          }
        })?;
        let mut vars: Vec<(String, Predicate)> = Vec::new();
        for (label, raw) in defs {
          if !seen_labels.insert(label.clone()) {
            return Err(
              ScriptValidationError::PrintoutLabelCollision {
                kind: "check",
                site: c.name.clone(),
                label: label.clone(),
              }
              .into(),
            );
          }
          let eq = Predicate::parse(raw, Scope::Check).map_err(|e| {
            ScriptValidationError::PrintoutPredicate {
              kind: "check",
              site: c.name.clone(),
              printout: pname.clone(),
              label: label.clone(),
              cause: Box::new(e),
            }
          })?;
          vars.push((label.clone(), eq));
        }
        resolved.push(ResolvedPrintout {
          name: pname.clone(),
          vars,
        });
      }
      if !resolved.is_empty() {
        check_printouts.insert(c.name.clone(), resolved);
      }
    }
    let mut comparison_printouts: BTreeMap<String, Vec<ResolvedPrintout>> =
      BTreeMap::new();
    for c in &self.comparisons {
      let refs = match &c.printouts {
        Some(r) => r,
        None => continue,
      };
      let mut resolved: Vec<ResolvedPrintout> = Vec::new();
      let mut seen_labels: BTreeSet<String> = BTreeSet::new();
      for pname in refs.into_iter() {
        let defs = printout_defs.get(pname).ok_or_else(|| {
          ScriptValidationError::PrintoutNotFound {
            kind: "comparison",
            site: c.name.clone(),
            printout: pname.clone(),
          }
        })?;
        let mut vars: Vec<(String, Predicate)> = Vec::new();
        for (label, raw) in defs {
          if !seen_labels.insert(label.clone()) {
            return Err(
              ScriptValidationError::PrintoutLabelCollision {
                kind: "comparison",
                site: c.name.clone(),
                label: label.clone(),
              }
              .into(),
            );
          }
          let eq = Predicate::parse(raw, Scope::Comparison).map_err(|e| {
            ScriptValidationError::PrintoutPredicate {
              kind: "comparison",
              site: c.name.clone(),
              printout: pname.clone(),
              label: label.clone(),
              cause: Box::new(e),
            }
          })?;
          vars.push((label.clone(), eq));
        }
        resolved.push(ResolvedPrintout {
          name: pname.clone(),
          vars,
        });
      }
      if !resolved.is_empty() {
        comparison_printouts.insert(c.name.clone(), resolved);
      }
    }
    return Ok(ReadyScript {
      files,
      extractions,
      extraction_flags,
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
      check_predicates,
      comparison_predicates,
      check_printouts,
      comparison_printouts,
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
  /// Per-extraction empty-match flags (`allow_empty`,
  /// `allow_reference_empty`, `allow_test_empty`).
  pub(crate) extraction_flags: BTreeMap<String, ExtractionFlags>,
  /// The comparison criteria within this script.
  pub(crate) criteria: BTreeMap<String, SimpleCriteria>,
  /// The comparisons within this script.
  pub(crate) comparisons: BTreeMap<String, Comparison>,
  /// The checks within this script.
  pub(crate) checks: BTreeMap<String, Check>,
  /// Parsed predicates for checks (keyed by check name). Absent when the
  /// check has no `predicate` field.
  pub(crate) check_predicates: BTreeMap<String, Predicate>,
  /// Parsed predicates for comparisons (keyed by comparison name).
  pub(crate) comparison_predicates: BTreeMap<String, Predicate>,
  /// Parsed printouts referenced by each check (keyed by check name).
  pub(crate) check_printouts: BTreeMap<String, Vec<ResolvedPrintout>>,
  /// Parsed printouts referenced by each comparison (keyed by
  /// comparison name).
  pub(crate) comparison_printouts: BTreeMap<String, Vec<ResolvedPrintout>>,
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
    let mut empty_extractions: Vec<(String, EmptySide)> = Vec::new();
    for en in comparison.extractions.clone().into_iter() {
      let ex = self
        .extractions
        .get(&en)
        .ok_or(ComparisonRunError::ExtractionNotFound(en.clone()))?;
      let ref_hits: Vec<DatumIndex> = ex.lookup(ref_file).collect();
      let test_hits: Vec<DatumIndex> = ex.lookup(test_file).collect();
      let flags = self.extraction_flags.get(&en).copied().unwrap_or_default();
      // Each flag is checked independently so the user gets one
      // violation per condition that actually fires.
      if !flags.allow_reference_empty && ref_hits.is_empty() {
        empty_extractions.push((en.clone(), EmptySide::Reference));
      }
      if !flags.allow_test_empty && test_hits.is_empty() {
        empty_extractions.push((en.clone(), EmptySide::Test));
      }
      if !flags.allow_empty && ref_hits.is_empty() && test_hits.is_empty() {
        empty_extractions.push((en.clone(), EmptySide::Both));
      }
      indices.extend(ref_hits);
      indices.extend(test_hits);
    }
    // If an predicate or printout is attached, precompute per-side stats
    // over the union of indices. Missing-in-one-file readings substitute
    // 0.0 (matching the same convention as the criteria pass below).
    // When the pool is empty the predicate is silently skipped: there are
    // no values to evaluate against, so nothing can fail it.
    let predicate = self.comparison_predicates.get(name);
    let printouts = self.comparison_printouts.get(name);
    let want_stats =
      (predicate.is_some() || printouts.is_some()) && !indices.is_empty();
    let (ref_stats, test_stats): (Option<Stats>, Option<Stats>) = if want_stats
    {
      let ref_vals: Vec<f64> = indices
        .iter()
        .map(|i| i.get_from(ref_file).unwrap_or(F06Number::Real(0.0)).into())
        .collect();
      let test_vals: Vec<f64> = indices
        .iter()
        .map(|i| i.get_from(test_file).unwrap_or(F06Number::Real(0.0)).into())
        .collect();
      (Stats::from_values(ref_vals), Stats::from_values(test_vals))
    } else {
      (None, None)
    };
    let mut flagged: BTreeMap<DatumIndex, FlaggedDetail> = BTreeMap::new();
    for i in indices.iter() {
      let ref_val = i.get_from(ref_file).unwrap_or(F06Number::Real(0.0));
      let test_val = i.get_from(test_file).unwrap_or(F06Number::Real(0.0));
      let mut maybe_reason: Option<FlagReason2> = None;
      if let Some(reason) = criteria.check(ref_val.into(), test_val.into()) {
        maybe_reason = Some(FlagReason2::Criteria(reason));
      } else if let (Some(eq), Some(rs), Some(ts)) =
        (predicate, ref_stats.as_ref(), test_stats.as_ref())
      {
        let xv: f64 = test_val.into();
        let yv: f64 = ref_val.into();
        match eq.evaluate(xv, Some(yv), ts, Some(rs)) {
          EvalOutcome::Pass { .. } => {}
          EvalOutcome::Fail { value } => {
            maybe_reason = Some(FlagReason2::Predicate {
              raw: eq.raw().to_owned(),
              value,
              error: None,
            });
          }
          EvalOutcome::Error { message } => {
            maybe_reason = Some(FlagReason2::Predicate {
              raw: eq.raw().to_owned(),
              value: f64::NAN,
              error: Some(message),
            });
          }
        }
      }
      if let Some(reason) = maybe_reason {
        let printout_values =
          match (printouts, ref_stats.as_ref(), test_stats.as_ref()) {
            (Some(ps), Some(rs), Some(ts)) if !ps.is_empty() => {
              evaluate_printouts_comparison(
                ps,
                test_val.into(),
                ref_val.into(),
                ts,
                rs,
              )
            }
            _ => Vec::new(),
          };
        flagged.insert(
          *i,
          FlaggedDetail {
            ref_val,
            test_val,
            reason,
            printouts: printout_values,
          },
        );
      }
    }
    return Ok(ComparisonResult {
      checked: indices,
      flagged,
      empty_extractions,
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
    let predicate = self.check_predicates.get(name);
    let printouts = self.check_printouts.get(name);
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
        let allow_empty = self
          .extraction_flags
          .get(en)
          .map(|f| f.allow_empty)
          .unwrap_or(true);
        // An empty pool silently skips the predicate (nothing to evaluate
        // against). The pair as a whole only fails if `allow_empty` is
        // false, in which case we flag it via `empty_violation`.
        let stats: Option<Stats> = if (predicate.is_some()
          || printouts.is_some())
          && !collected.is_empty()
        {
          Stats::from_values(collected.iter().map(|(_, n)| f64::from(*n)))
        } else {
          None
        };
        let mut pres = check.run_for(collected, predicate, stats.as_ref());
        if !allow_empty && pres.checked.is_empty() {
          pres.empty_violation = true;
        }
        // Decorate flagged failures with printout evaluations.
        if let (Some(ps), Some(st)) = (printouts, stats.as_ref())
          && !ps.is_empty()
        {
          for (_di, fail) in pres.flagged.iter_mut() {
            let x: f64 = fail.value.into();
            fail.printouts = evaluate_printouts_check(ps, x, st);
          }
        }
        results
          .per_pair
          .insert((f06_name.to_owned(), en.to_owned()), pres);
      }
    }
    return Ok(results);
  }
}

/// Evaluates each label of every printout for one comparison-flagged
/// datum. Returns `(label, PrintoutValue)` in definition order.
fn evaluate_printouts_comparison(
  printouts: &[ResolvedPrintout],
  x_val: f64,
  y_val: f64,
  x_stats: &Stats,
  y_stats: &Stats,
) -> Vec<(String, PrintoutValue)> {
  let mut out = Vec::new();
  for p in printouts {
    for (label, eq) in &p.vars {
      let pv = match eq.evaluate(x_val, Some(y_val), x_stats, Some(y_stats)) {
        EvalOutcome::Pass { value } | EvalOutcome::Fail { value } => {
          PrintoutValue::Number(value)
        }
        EvalOutcome::Error { message } => PrintoutValue::Error(message),
      };
      out.push((label.clone(), pv));
    }
  }
  return out;
}

/// Evaluates each label of every printout for one check-flagged datum.
fn evaluate_printouts_check(
  printouts: &[ResolvedPrintout],
  x_val: f64,
  x_stats: &Stats,
) -> Vec<(String, PrintoutValue)> {
  let mut out = Vec::new();
  for p in printouts {
    for (label, eq) in &p.vars {
      let pv = match eq.evaluate(x_val, None, x_stats, None) {
        EvalOutcome::Pass { value } | EvalOutcome::Fail { value } => {
          PrintoutValue::Number(value)
        }
        EvalOutcome::Error { message } => PrintoutValue::Error(message),
      };
      out.push((label.clone(), pv));
    }
  }
  return out;
}
