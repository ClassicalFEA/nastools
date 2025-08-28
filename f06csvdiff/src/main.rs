use clap::Parser;
use csv::ReaderBuilder;
use regex::Regex;
use std::path::PathBuf;
use std::process;

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
  csv1: String,
  csv2: String,
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

  let float_re = Regex::new(r"[-+]?[0-9]*\.?[0-9]+E[-+]?[0-9]+").unwrap();

  // Track maxima for reporting
  let mut max_abs_diff = 0.0;
  let mut max_abs_vals = (0.0, 0.0);
  let mut max_diff_line = 0;
  let mut max_ratio = 0.0;
  let mut max_ratio_vals = (0.0, 0.0);
  let mut max_ratio_line = 0;

  let mut cols: Option<usize> = None;
  let mut iter1 = rdr1.records();
  let mut iter2 = rdr2.records();
  let mut line_num = 1;

  while let (Some(r1), Some(r2)) = (iter1.next(), iter2.next()) {
    let rec1 = r1.unwrap_or_else(|e| {
      eprintln!("Error reading {} at line {}: {}", &args.csv1, line_num, e);
      process::exit(1)
    });
    let rec2 = r2.unwrap_or_else(|e| {
      eprintln!("Error reading {} at line {}: {}", &args.csv2, line_num, e);
      process::exit(1)
    });

    // Column count check
    let len1 = rec1.len();
    let len2 = rec2.len();
    if cols.is_none() {
      if len1 != len2 {
        eprintln!(
          "Error: column count differs at line {}: {} has {}, {} has {}",
          line_num, &args.csv1, len1, &args.csv2, len2
        );
        process::exit(1);
      }
      cols = Some(len1);
    } else if Some(len1) != cols || Some(len2) != cols {
      eprintln!(
        "Error: inconsistent column count at line {} (expected {}), got {} and {}",
        line_num,
        cols.unwrap(),
        len1,
        len2
      );
      process::exit(1);
    }

    // Extract floats
    let f1: Vec<(usize, f64)> = rec1
      .iter()
      .enumerate()
      .filter_map(|(i, f)| {
        if float_re.is_match(f) {
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
        if float_re.is_match(f) {
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
      line_num += 1;
      continue;
    }
    if f1.len() != f2.len() || f1.is_empty() || f2.is_empty() {
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

    line_num += 1;
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
      println!(
        "maximum percent difference seen: {:.2}%",
        (max_ratio - 1.0) * 100.0,
      );
      println!(
        "the values: {:+.6E} and {:+.6E} (line {})",
        max_ratio_vals.0, max_ratio_vals.1, max_ratio_line
      );
      let status = if max_ratio > mr { "FAILED" } else { "PASSED" };
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
  } else {
    print!("{bn1} {bn2} ");
    if let Some(mr) = args.max_ratio {
      print!("{:.2}", (max_ratio - 1.0) * 100.0,);
      print!(
        " {:+.6E} {:+.6E} {}",
        max_ratio_vals.0, max_ratio_vals.1, max_ratio_line
      );
      let status = if max_ratio > mr { "FAILED" } else { "PASSED" };
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
