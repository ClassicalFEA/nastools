//! User-configurable options for the conversion, mirroring the CLI flags
//! of the `f06csv` binary.

use std::str::FromStr;

use f06::prelude::ElementType;
use nas_csv::prelude::{
  Alignment, BlankDisplay, CsvBlockId, CsvFormatting, FloatFormat,
};
use serde::{Deserialize, Serialize};

/// Conversion options mirroring the CLI flags of `f06csv`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Options {
  /// CSV blocks to include in the output. Empty means "all".
  pub csv_blocks: Vec<CsvBlockId>,
  /// Grid point ID filter. Empty means "no filter".
  pub gids: Vec<usize>,
  /// Element ID filter. Empty means "no filter".
  pub eids: Vec<usize>,
  /// Element type filter. Empty means "no filter".
  pub etypes: Vec<ElementType>,
  /// Subcase filter. Empty means "no filter".
  pub subcases: Vec<usize>,
  /// Column filter (1..=11). Empty means "all columns".
  pub cols: Vec<usize>,
  /// Whether to write CSV headers (re-emitted on header changes).
  pub headers: bool,
  /// Field delimiter character.
  pub delim: char,
  /// Force tab as delimiter (overrides `delim`).
  pub tab: bool,
  /// Use CRLF line terminators (else LF).
  pub crlf: bool,
  /// Detailed formatting (floats, blanks, alignment).
  pub fmtr: CsvFormatting,
}

impl Default for Options {
  fn default() -> Self {
    return Self {
      csv_blocks: Vec::new(),
      gids: Vec::new(),
      eids: Vec::new(),
      etypes: Vec::new(),
      subcases: Vec::new(),
      cols: Vec::new(),
      headers: false,
      delim: ',',
      tab: false,
      crlf: false,
      fmtr: CsvFormatting {
        reals: FloatFormat {
          dec_places: Some(6),
          no_scientific: false,
          no_superfluous_plus: false,
          small_e: false,
        },
        blanks: BlankDisplay::Dashes,
        align: Alignment::None,
      },
    };
  }
}

/// Parses a comma/space-separated list into a vector, ignoring blanks.
///
/// Returns `Err` with the first offending token on a parse failure.
pub fn parse_list<T: FromStr>(input: &str) -> Result<Vec<T>, String>
where
  T::Err: std::fmt::Display,
{
  let mut out: Vec<T> = Vec::new();
  for tok in input.split(|c: char| c == ',' || c.is_whitespace()) {
    let t = tok.trim();
    if t.is_empty() {
      continue;
    }
    match t.parse::<T>() {
      Ok(v) => out.push(v),
      Err(e) => return Err(format!("could not parse \"{t}\": {e}")),
    }
  }
  return Ok(out);
}

/// Formats a list of `Display`able values into a comma-separated string.
pub fn format_list<T: std::fmt::Display>(items: &[T]) -> String {
  let mut s = String::new();
  for (i, item) in items.iter().enumerate() {
    if i > 0 {
      s.push_str(", ");
    }
    s.push_str(&item.to_string());
  }
  return s;
}

/// Renders the variant of [`BlankDisplay`] as the kebab-case string clap
/// expects on the CLI.
fn blanks_cli(b: &BlankDisplay) -> &'static str {
  return match b {
    BlankDisplay::Zero => "zero",
    BlankDisplay::Space => "space",
    BlankDisplay::Dash => "dash",
    BlankDisplay::Dashes => "dashes",
    BlankDisplay::Empty => "empty",
  };
}

/// Renders the variant of [`Alignment`] as the kebab-case string clap
/// expects on the CLI.
fn align_cli(a: &Alignment) -> &'static str {
  return match a {
    Alignment::None => "none",
    Alignment::Right => "right",
    Alignment::Left => "left",
    Alignment::Center => "center",
  };
}

/// Quotes a token for shell-safe display if it contains spaces or
/// metacharacters; otherwise returns it as-is.
fn shell_quote(s: &str) -> String {
  let needs = s.is_empty()
    || s.chars().any(|c| {
      matches!(
        c,
        ' '
          | '\t'
          | '"'
          | '\''
          | '\\'
          | '$'
          | '`'
          | '|'
          | '&'
          | ';'
          | '('
          | ')'
          | '<'
          | '>'
          | '*'
          | '?'
          | '#'
          | '!'
      )
    });
  if !needs {
    return s.to_owned();
  }
  // Single-quote and escape any embedded single-quotes.
  let mut out = String::with_capacity(s.len() + 2);
  out.push('\'');
  for c in s.chars() {
    if c == '\'' {
      out.push_str("'\\''");
    } else {
      out.push(c);
    }
  }
  out.push('\'');
  return out;
}

/// Joins a list of stringly values with commas, for `--flag a,b,c`.
fn comma_join<T: std::fmt::Display>(xs: &[T]) -> String {
  let mut s = String::new();
  for (i, x) in xs.iter().enumerate() {
    if i > 0 {
      s.push(',');
    }
    s.push_str(&x.to_string());
  }
  return s;
}

/// Builds the equivalent `f06csv` CLI invocation for the given options,
/// using `<input.f06>` as a placeholder for the user's file. Only
/// non-default flags are emitted.
pub fn cli_flags(opts: &Options) -> String {
  let def = Options::default();
  let mut parts: Vec<String> = vec!["f06csv".to_owned()];

  if !opts.csv_blocks.is_empty() {
    let shorts: Vec<&'static str> =
      opts.csv_blocks.iter().map(|b| b.shorthand()).collect();
    parts.push("-b".into());
    parts.push(comma_join(&shorts));
  }
  if !opts.gids.is_empty() {
    parts.push("-g".into());
    parts.push(comma_join(&opts.gids));
  }
  if !opts.eids.is_empty() {
    parts.push("-e".into());
    parts.push(comma_join(&opts.eids));
  }
  if !opts.etypes.is_empty() {
    parts.push("-t".into());
    parts.push(comma_join(&opts.etypes));
  }
  if !opts.subcases.is_empty() {
    parts.push("-s".into());
    parts.push(comma_join(&opts.subcases));
  }
  if !opts.cols.is_empty() {
    parts.push("-c".into());
    parts.push(comma_join(&opts.cols));
  }
  if opts.headers {
    parts.push("-H".into());
  }
  if opts.tab {
    parts.push("--tab".into());
  } else if opts.delim != def.delim {
    parts.push("-d".into());
    parts.push(shell_quote(&opts.delim.to_string()));
  }
  if opts.crlf {
    parts.push("--crlf".into());
  }
  // Float formatting.
  if opts.fmtr.reals.dec_places != def.fmtr.reals.dec_places {
    match opts.fmtr.reals.dec_places {
      Some(n) => {
        parts.push("--decimals".into());
        parts.push(n.to_string());
      }
      None => {
        // No CLI flag clears it; closest is leaving it as default. Skip.
      }
    }
  }
  if opts.fmtr.reals.no_scientific {
    parts.push("--no-sci".into());
  }
  if opts.fmtr.reals.no_superfluous_plus {
    parts.push("--omit-plus".into());
  }
  if opts.fmtr.reals.small_e {
    parts.push("--small-e".into());
  }
  if std::mem::discriminant(&opts.fmtr.blanks)
    != std::mem::discriminant(&def.fmtr.blanks)
  {
    parts.push("-B".into());
    parts.push(blanks_cli(&opts.fmtr.blanks).to_owned());
  }
  if std::mem::discriminant(&opts.fmtr.align)
    != std::mem::discriminant(&def.fmtr.align)
  {
    parts.push("--align".into());
    parts.push(align_cli(&opts.fmtr.align).to_owned());
  }

  parts.push("<input.f06>".to_owned());
  return parts.join(" ");
}

// `CsvFormatting` (from `nas_csv`) does not implement `PartialEq`, so we
// compare its fields by hand here. This is required for Yew's `Properties`.
impl PartialEq for Options {
  fn eq(&self, other: &Self) -> bool {
    return self.csv_blocks == other.csv_blocks
      && self.gids == other.gids
      && self.eids == other.eids
      && self.etypes == other.etypes
      && self.subcases == other.subcases
      && self.cols == other.cols
      && self.headers == other.headers
      && self.delim == other.delim
      && self.tab == other.tab
      && self.crlf == other.crlf
      && self.fmtr.reals.dec_places == other.fmtr.reals.dec_places
      && self.fmtr.reals.no_scientific == other.fmtr.reals.no_scientific
      && self.fmtr.reals.no_superfluous_plus
        == other.fmtr.reals.no_superfluous_plus
      && self.fmtr.reals.small_e == other.fmtr.reals.small_e
      && std::mem::discriminant(&self.fmtr.blanks)
        == std::mem::discriminant(&other.fmtr.blanks)
      && std::mem::discriminant(&self.fmtr.align)
        == std::mem::discriminant(&other.fmtr.align);
  }
}
