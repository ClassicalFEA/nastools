//! This module implements utility types for f06magic.

use f06::prelude::*;
use num::PrimInt;
use serde::{Deserialize, Serialize};

/// A simple, inclusive range.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub(crate) struct InclusiveRange<T> {
  /// Start of the inclusive range.
  from: T,
  /// End of the inclusive range.
  to: T,
}

impl<T: PrimInt> IntoIterator for InclusiveRange<T> {
  type Item = T;

  type IntoIter = num::iter::RangeInclusive<T>;

  fn into_iter(self) -> Self::IntoIter {
    return num::range_inclusive(self.from, self.to);
  }
}

/// For inputs that can take a number, a list of numbers, or a min/max.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum NumListRange<T> {
  /// No numbers.
  #[default]
  None,
  /// A single number.
  Single(T),
  /// A list of numbers.
  List(Vec<T>),
  /// A minimum and a maximum (inclusive).
  Range(InclusiveRange<T>),
  /// Several inclusive ranges.
  Ranges(Vec<InclusiveRange<T>>),
}

impl<T: PrimInt + 'static> IntoIterator for NumListRange<T> {
  type Item = T;

  type IntoIter = Box<dyn Iterator<Item = T>>;

  fn into_iter(self) -> Self::IntoIter {
    return match self {
      NumListRange::None => Box::new([].into_iter()),
      NumListRange::Single(x) => Box::new([x].into_iter()),
      NumListRange::List(v) => Box::new(v.into_iter()),
      NumListRange::Range(r) => Box::new(r.into_iter()),
      NumListRange::Ranges(vec) => Box::new(vec.into_iter().flatten()),
    };
  }
}

impl<T: PrimInt + 'static> From<NumListRange<T>> for Specifier<T> {
  fn from(value: NumListRange<T>) -> Self {
    return match value {
      NumListRange::None => Self::All,
      NumListRange::Single(x) => Self::List(Vec::from([x])),
      NumListRange::List(v) => Self::List(v),
      _ => Self::List(value.into_iter().collect()),
    };
  }
}

/// One or many of anything.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum OneOrMany<T> {
  /// One of the thing
  One(T),
  /// Many of the thing
  Many(Vec<T>),
}

impl<T> From<OneOrMany<T>> for AnyAmount<T> {
  fn from(value: OneOrMany<T>) -> Self {
    return match value {
      OneOrMany::One(x) => Self::One(x),
      OneOrMany::Many(v) => Self::Many(v),
    };
  }
}

impl<T> IntoIterator for OneOrMany<T> {
  type Item = T;

  type IntoIter = <Vec<T> as IntoIterator>::IntoIter;

  fn into_iter(self) -> Self::IntoIter {
    return match self {
      Self::One(x) => Vec::from([x]).into_iter(),
      Self::Many(v) => v.into_iter(),
    };
  }
}

impl<'a, T> IntoIterator for &'a OneOrMany<T> {
  type Item = &'a T;

  type IntoIter = Box<dyn Iterator<Item = &'a T> + 'a>;

  fn into_iter(self) -> Self::IntoIter {
    return match self {
      OneOrMany::One(x) => Box::new(std::iter::once(x)),
      OneOrMany::Many(v) => Box::new(v.iter()),
    };
  }
}

/// None, one or many of anything.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum AnyAmount<T> {
  /// None of the thing
  #[default]
  None,
  /// One of the thing
  One(T),
  /// Many of the thing
  Many(Vec<T>),
}

impl<T> IntoIterator for AnyAmount<T> {
  type Item = T;

  type IntoIter = <Vec<T> as IntoIterator>::IntoIter;

  fn into_iter(self) -> Self::IntoIter {
    return match self {
      Self::None => Vec::new().into_iter(),
      Self::One(x) => Vec::from([x]).into_iter(),
      Self::Many(v) => v.into_iter(),
    };
  }
}

impl<'a, T> IntoIterator for &'a AnyAmount<T> {
  type Item = &'a T;

  type IntoIter = Box<dyn Iterator<Item = &'a T> + 'a>;

  fn into_iter(self) -> Self::IntoIter {
    return match self {
      AnyAmount::None => Box::new(std::iter::empty()),
      AnyAmount::One(x) => Box::new(std::iter::once(x)),
      AnyAmount::Many(v) => Box::new(v.iter()),
    };
  }
}

impl<T> From<AnyAmount<T>> for Specifier<T> {
  fn from(value: AnyAmount<T>) -> Self {
    return match value {
      AnyAmount::None => Self::All,
      AnyAmount::One(x) => Self::List(Vec::from([x])),
      AnyAmount::Many(v) => Self::List(v),
    };
  }
}
