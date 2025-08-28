use clap::Parser;
use csv::ReaderBuilder;
use regex::Regex;
use std::path::PathBuf;
use std::process;

#[derive(Clone, Debug)]
enum Alignment {
  Left,
  Right,
  Center,
}

impl std::str::FromStr for Alignment {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "left" => Ok(Alignment::Left),
      "right" => Ok(Alignment::Right),
      "center" => Ok(Alignment::Center),
      _ => Err(format!(
        "Invalid alignment: {s}. Must be left, right, or center"
      )),
    }
  }
}

/// Diffs floating-point numbers at corresponding positions within two CSVs.
///
/// Made for usage alongside f06csv.
///
/// Author: Bruno Borges Paschoalinoto <bruno@paschoalinoto.com>
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
  #[arg(short = 'd', long, value_name = "REAL")]
  max_diff: Option<f64>,
  #[arg(short = 'r', long, value_name = "REAL")]
  max_ratio: Option<f64>,
  #[arg(short = 't', long, value_name = "REAL", default_value = "0")]
  threshold: f64,
  #[arg(long, value_name = "CHAR", default_value = ",")]
  delim: char,
  #[arg(long)]
  explain: bool,
  #[arg(long, value_name = "ALIGNMENT")]
  align: Option<Alignment>,
  csv1: String,
  csv2: String,
}

fn align_text(text: &str, width: usize, alignment: &Alignment) -> String {
  if text.len() >= width {
    return text.to_string();
  }

  let padding = width - text.len();
  match alignment {
    Alignment::Left => format!("{text}{}", " ".repeat(padding)),
    Alignment::Right => format!("{}{text}", " ".repeat(padding)),
    Alignment::Center => {
      let left_pad = padding / 2;
      let right_pad = padding - left_pad;
      format!("{}{text}{}", " ".repeat(left_pad), " ".repeat(right_pad))
    }
  }
}

fn format_aligned_output(
  filenames: (&str, &str),
  max_ratio_info: Option<(f64, (f64, f64), usize, bool)>,
  max_diff_info: Option<(f64, (f64, f64), usize, bool)>,
  alignment: &Alignment,
) {
  let mut rows = Vec::new();
  let mut headers = vec![filenames.0.to_string(), filenames.1.to_string()];

  let mut first_row = vec![];
  if let Some((ratio, (v1, v2), line, passed)) = max_ratio_info {
    let percent = ((ratio - 1.0) * 100.0).abs();
    first_row.extend([
      format!("{percent:.2}"),
      format!("{v1:+.6E}"),
      format!("{v2:+.6E}"),
      line.to_string(),
      if passed {
        "PASSED".to_string()
      } else {
        "FAILED".to_string()
      },
    ]);
    headers.extend(
      ["ratio_%", "val1_r", "val2_r", "line_r", "status_r"]
        .iter()
        .map(|s| s.to_string()),
    );
  }

  if let Some((diff, (v1, v2), line, passed)) = max_diff_info {
    first_row.extend([
      format!("{diff:.2E}"),
      format!("{v1:+.6E}"),
      format!("{v2:+.6E}"),
      line.to_string(),
      if passed {
        "PASSED".to_string()
      } else {
        "FAILED".to_string()
      },
    ]);
    headers.extend(
      ["abs_diff", "val1_d", "val2_d", "line_d", "status_d"]
        .iter()
        .map(|s| s.to_string()),
    );
  }

  rows.push(first_row);

  // Calculate column widths
  let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
  for row in &rows {
    for (i, cell) in row.iter().enumerate() {
      if i < col_widths.len() {
        col_widths[i] = col_widths[i].max(cell.len());
      }
    }
  }

  // Print aligned output
  let aligned_headers: Vec<String> = headers
    .iter()
    .zip(&col_widths)
    .map(|(header, &width)| align_text(header, width, alignment))
    .collect();
  println!("{}", aligned_headers.join(" "));

  for row in &rows {
    let aligned_row: Vec<String> = row
      .iter()
      .zip(&col_widths)
      .map(|(cell, &width)| align_text(cell, width, alignment))
      .collect();
    println!("{}", aligned_row.join(" "));
  }
}

fn main() {
  let args = Args::parse();
  if args.max_diff.is_none() && args.max_ratio.is_none() {
    eprintln!("Error: at least one of -d or -r must be specified.");
    process::exit(1);
  }

  let delim = args.delim.try_into().unwrap();
  let mut rdr1 = ReaderBuilder::new()
    .has_headers(false)
    .delimiter(delim)
    .from_path(&args.csv1)
    .unwrap_or_else(|e| {
      eprintln!("Error opening {}: {}", &args.csv1, e);
      process::exit(1)
    });
  let mut rdr2 = ReaderBuilder::new()
    .has_headers(false)
    .delimiter(delim)
    .from_path(&args.csv2)
    .unwrap_or_else(|e| {
      eprintln!("Error opening {}: {}", &args.csv2, e);
      process::exit(1)
    });

  let float_re = Regex::new(r"[-+]?[0-9]*\.?[0-9]+[Ee][-+]?[0-9]+").unwrap();

  // First pass: determine which columns contain only floats in both files
  let mut float_columns: Option<Vec<bool>> = None;

  // Read all records to determine float columns
  let records1: Vec<_> = rdr1
    .records()
    .collect::<Result<Vec<_>, _>>()
    .unwrap_or_else(|e| {
      eprintln!("Error reading {}: {}", &args.csv1, e);
      process::exit(1);
    });
  let records2: Vec<_> = rdr2
    .records()
    .collect::<Result<Vec<_>, _>>()
    .unwrap_or_else(|e| {
      eprintln!("Error reading {}: {}", &args.csv2, e);
      process::exit(1);
    });

  if records1.len() != records2.len() {
    eprintln!(
      "Error: files have different number of rows ({} vs {})",
      records1.len(),
      records2.len()
    );
    process::exit(1);
  }

  for (line_num, (rec1, rec2)) in records1.iter().zip(&records2).enumerate() {
    let line_num = line_num + 1;

    // Column count check
    let len1 = rec1.len();
    let len2 = rec2.len();
    if len1 != len2 {
      eprintln!(
        "Error: column count differs at line {}: {} has {}, {} has {}",
        line_num, &args.csv1, len1, &args.csv2, len2
      );
      process::exit(1);
    }

    // Initialize float_columns on first row
    if float_columns.is_none() {
      float_columns = Some(vec![true; len1]);
    }

    let float_cols = float_columns.as_mut().unwrap();

    // Check each column to see if it's a float in both files
    for (i, (cell1, cell2)) in rec1.iter().zip(rec2.iter()).enumerate() {
      if float_cols[i] {
        let is_float1 =
          float_re.is_match(cell1) && cell1.parse::<f64>().is_ok();
        let is_float2 =
          float_re.is_match(cell2) && cell2.parse::<f64>().is_ok();
        if !is_float1 || !is_float2 {
          float_cols[i] = false;
        }
      }
    }
  }

  let float_cols = float_columns.unwrap_or_default();

  // Track maxima for reporting
  let mut max_abs_diff = 0.0;
  let mut max_abs_vals = (0.0, 0.0);
  let mut max_diff_line = 0;
  let mut max_ratio = 1.0; // Initialize to 1.0 (no difference)
  let mut max_ratio_vals = (0.0, 0.0);
  let mut max_ratio_line = 0;

  // Second pass: compare float values
  for (line_num, (rec1, rec2)) in records1.iter().zip(&records2).enumerate() {
    let line_num = line_num + 1;

    // Extract floats from float columns only
    let f1: Vec<(usize, f64)> = rec1
      .iter()
      .enumerate()
      .filter_map(|(i, f)| {
        if float_cols[i] && float_re.is_match(f) {
          match f.parse() {
            Ok(v) => Some((i, v)),
            Err(_) => {
              eprintln!(
                "Error parsing '{}' in {} at line {}",
                f, &args.csv1, line_num
              );
              process::exit(1);
            }
          }
        } else {
          None
        }
      })
      .collect();
    let f2: Vec<(usize, f64)> = rec2
      .iter()
      .enumerate()
      .filter_map(|(i, f)| {
        if float_cols[i] && float_re.is_match(f) {
          match f.parse() {
            Ok(v) => Some((i, v)),
            Err(_) => {
              eprintln!(
                "Error parsing '{}' in {} at line {}",
                f, &args.csv2, line_num
              );
              process::exit(1);
            }
          }
        } else {
          None
        }
      })
      .collect();

    if f1.is_empty() && f2.is_empty() {
      continue;
    }
    if f1.len() != f2.len() {
      eprintln!("Error: float layout differs at line {line_num}");
      process::exit(1);
    }

    // Compare
    for ((_, v1), (_, v2)) in f1.iter().zip(&f2) {
      let a1 = *v1;
      let a2 = *v2;
      if a1 == 0.0 && a2 == 0.0 {
        continue;
      }
      if a1.abs() < args.threshold && a2.abs() < args.threshold {
        continue;
      }

      // Check abs diff
      let diff = (a1 - a2).abs();
      if diff > max_abs_diff {
        max_abs_diff = diff;
        max_abs_vals = (a1, a2);
        max_diff_line = line_num;
      }

      // Check ratio
      let ratio = if a1 == 0.0 || a2 == 0.0 {
        f64::INFINITY
      } else {
        a1.abs().max(a2.abs()) / a1.abs().min(a2.abs())
      };
      if ratio > max_ratio {
        max_ratio = ratio;
        max_ratio_vals = (a1, a2);
        max_ratio_line = line_num;
      }
    }
  }

  let pb1 = PathBuf::from(&args.csv1);
  let pb2 = PathBuf::from(&args.csv2);
  let bn1 = pb1
    .file_name()
    .map(|s| s.to_string_lossy())
    .unwrap_or(std::borrow::Cow::Borrowed("<?>"));
  let bn2 = pb2
    .file_name()
    .map(|s| s.to_string_lossy())
    .unwrap_or(std::borrow::Cow::Borrowed("<?>"));

  // Report
  if args.explain {
    println!("files: {bn1} and {bn2}\n");
    if let Some(mr) = args.max_ratio {
      let percentage_diff = ((max_ratio - 1.0) * 100.0).abs();
      println!(
        "maximum percent difference seen: {percentage_diff:.2}%",
      );
      println!(
        "the values: {:+.6E} and {:+.6E} (line {})",
        max_ratio_vals.0, max_ratio_vals.1, max_ratio_line
      );
      let status = if percentage_diff > mr * 100.0 { "FAILED" } else { "PASSED" };
      println!("result: {status}");
    }

    if args.max_diff.is_some() && args.max_ratio.is_some() {
      println!();
    }

    if let Some(md) = args.max_diff {
      println!("maximum absolute difference seen: {max_abs_diff:.2E}");
      println!(
        "the values: {:+.6E} and {:+.6E} (line {})",
        max_abs_vals.0, max_abs_vals.1, max_diff_line
      );
      let status = if max_abs_diff > md {
        "FAILED"
      } else {
        "PASSED"
      };
      println!("result: {status}");
    }
  } else if let Some(align) = &args.align {
    // Use aligned output format
    let max_ratio_info = args.max_ratio.map(|mr| {
      let percentage_diff = ((max_ratio - 1.0) * 100.0).abs();
      let passed = percentage_diff <= mr * 100.0;
      (max_ratio, max_ratio_vals, max_ratio_line, passed)
    });

    let max_diff_info = args.max_diff.map(|md| {
      let passed = max_abs_diff <= md;
      (max_abs_diff, max_abs_vals, max_diff_line, passed)
    });

    format_aligned_output((&bn1, &bn2), max_ratio_info, max_diff_info, align);
  } else {
    print!("{bn1} {bn2} ");
    if let Some(mr) = args.max_ratio {
      let percentage_diff = ((max_ratio - 1.0) * 100.0).abs();
      print!("{percentage_diff:.2}");
      print!(
        " {:+.6E} {:+.6E} {}",
        max_ratio_vals.0, max_ratio_vals.1, max_ratio_line
      );
      let status = if percentage_diff > mr * 100.0 { "FAILED" } else { "PASSED" };
      print!(" {status}");
    }

    if args.max_diff.is_some() && args.max_ratio.is_some() {
      print!(" ");
    }

    if let Some(md) = args.max_diff {
      print!("{max_abs_diff:.2E} ");
      print!(
        "{:+.6E} {:+.6E} {}",
        max_abs_vals.0, max_abs_vals.1, max_diff_line
      );
      let status = if max_abs_diff > md {
        "FAILED"
      } else {
        "PASSED"
      };
      print!(" {status}");
    }
    println!();
  }
}
