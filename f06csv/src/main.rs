//! A command-line application to convert Nastran F06 output to CSV.

#![allow(clippy::needless_return)]
#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;

use clap::Parser;
use clap::ValueEnum;
use csv::Terminator;
use f06::prelude::*;
use log::*;
use nas_csv::from_f06::templates::all_converters;
use nas_csv::prelude::*;
use serde::{Deserialize, Serialize};

/// help template for clap args parser
const HELP_TEMPLATE: &str = "{name} - {version}
{about-section}
{usage-heading}
{tab}{usage}

{all-args}
{after-help}";

/// A command-line utility to convert NASTRAN F06 output to CSV.
#[derive(Clone, Debug, Parser)]
#[command(
  name = "f06csv",
  author,
  version,
  help_template = HELP_TEMPLATE,
)]
struct Cli {
  /// Path to write output to.
  ///
  /// If absent, writes to standard output.
  #[arg(short = 'o')]
  output: Option<PathBuf>,
  /// CSV blocks to write. Can be specified more than once, or comma-separated.
  ///
  /// You can also write the numerical IDs for shorthand.
  ///
  /// If absent, all blocks are written.
  #[arg(short = 'b', long = "blocks", num_args = 0.., value_delimiter = ',')]
  csv_blocks: Vec<CsvBlockId>,
  /// Grid point ID filter.
  ///
  /// If a record has a grid point ID, only output those that contain the
  /// specified IDs.
  ///
  /// Can be specified more than once, or comma-separated.
  ///
  /// If absent, no grid point ID filter is applied.
  #[arg(short = 'g', long = "gids", num_args = 0.., value_delimiter = ',')]
  gids: Vec<usize>,
  /// Element ID filter.
  ///
  /// If a record has an element ID, only output those that contain the
  /// specified IDs.
  ///
  /// Can be specified more than once, or comma-separated.
  ///
  /// If absent, no element ID filter is applied.
  #[arg(short = 'e', long = "eids", num_args = 0.., value_delimiter = ',')]
  eids: Vec<usize>,
  /// Element type filter.
  ///
  /// If a record has an element type, only output those that contain the
  /// specified types.
  ///
  /// Can be specified more than once, or comma-separated.
  ///
  /// If absent, no element type filter is applied.
  #[arg(short = 't', long = "etypes", num_args = 0.., value_delimiter = ',')]
  etypes: Vec<ElementType>,
  /// Subcase filter.
  ///
  /// If a record has subcase ID, only output those that contain the
  /// specified IDs.
  ///
  /// Can be specified more than once, or comma-separated.
  ///
  /// If absent, no subcase filter is applied.
  #[arg(short = 's', long = "subcases", num_args = 0.., value_delimiter = ',')]
  subcases: Vec<usize>,
  /// Enable writing CSV headers.
  ///
  /// Be warned, they're written every time there's a change.
  #[arg(short = 'H', long = "headers")]
  headers: bool,
  /// The delimiter used in the CSV.
  #[arg(short = 'd', long, default_value = ",", verbatim_doc_comment)]
  delim: char,
  /// Use a tab as delimiter. Overrides --delim.
  #[arg(long = "tab", verbatim_doc_comment)]
  tab: bool,
  /// Use CRLF (Windows) line breaks. Default is LF (Unix).
  #[arg(long = "crlf", verbatim_doc_comment)]
  crlf: bool,
  /// Formatting options.
  #[command(flatten)]
  fmtr: CsvFormatting,
  /// Limit output to specific columns (numbers 1 thru 11)
  ///
  /// Can be specified more than once, or comma-separated.
  #[arg(short = 'c', long = "cols", num_args = 0.., value_delimiter = ',')]
  cols: Vec<usize>,
  /// Output extra/debug info while parsing and converting.
  #[arg(short = 'v', long = "verbose", verbatim_doc_comment)]
  verbose: bool,
  /// Dump a CSV containing all blocks' headers and column numbers and exit.
  headers_csv: bool,
  /// The name of the input F06 file.
  ///
  /// If - or absent, reads from standard input.
  #[arg(default_value = "-")]
  input: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
  // init cli stuff
  let mut args = Cli::parse();
  let log_level = if args.verbose {
    LevelFilter::Debug
  } else {
    LevelFilter::Info
  };
  env_logger::builder().filter_level(log_level).init();
  // check for debug options
  if args.headers_csv {
    println!("block_name,row_num,1,2,3,4,5,6,7,8,9,10,11");
    all_converters().iter().for_each(|(bt, ct)| {
      ct.headers.iter().enumerate().for_each(|(row, header)| {
        println!("{},{},{}", bt.snake_case_name(), row + 1, header.join(","));
      })
    });
    return Ok(());
  }
  // parse the file
  let mut f06: F06File = if args.input.as_os_str().eq_ignore_ascii_case("-") {
    OnePassParser::parse_bufread(BufReader::new(io::stdin()))?
  } else if args.input.is_file() {
    if let Some(bn) = args.input.file_name() {
      if let Some(sbn) = bn.to_str() {
        info!("Parsing {sbn}...");
      }
    } else {
      info!("Parsing...");
    }
    OnePassParser::parse_file(&args.input)?
  } else {
    error!("Provided path either does not exist or is not a file!");
    std::process::exit(1);
  };
  f06.merge_blocks(true);
  f06.merge_potential_headers();
  f06.sort_all_blocks();
  info!("Done parsing.");
  // init the csv writer
  let output: BufWriter<Box<dyn Write>> =
    BufWriter::new(if let Some(ref op) = args.output {
      Box::new(File::create(op)?)
    } else {
      Box::new(io::stdout())
    });
  if args.tab {
    args.delim = '\t';
  }
  let delim_byte: u8 = args
    .delim
    .try_into()
    .expect("Delimiter must be an ASCII character!");
  let term = if args.crlf {
    Terminator::CRLF
  } else {
    Terminator::default()
  };
  let mut wtr = csv::WriterBuilder::new()
    .delimiter(delim_byte)
    .terminator(term)
    .from_writer(output);
  /// Filter only if there is at least one in the filter.
  fn lax_filter<T: PartialEq>(v: &[T], x: &Option<T>) -> bool {
    return v.is_empty()
      || x.is_none()
      || x.as_ref().is_some_and(|k| v.contains(k));
  }
  /// Filter an iterator over columns.
  fn col_filter<T, I: Iterator<Item = T>>(
    it: I,
    a: &Cli,
  ) -> impl Iterator<Item = T> + use<'_, T, I> {
    return it
      .enumerate()
      .filter(|(i, _x)| a.cols.is_empty() || a.cols.contains(&(i + 1)))
      .map(|t| t.1);
  }
  // should we write a record?
  let should_write = |r: &CsvRecord, a: &Cli| -> bool {
    let f_blocks = lax_filter(&a.csv_blocks, &Some(r.block_id));
    let f_gids = lax_filter(&a.gids, &r.gid);
    let f_eids = lax_filter(&a.eids, &r.eid);
    let f_etypes = lax_filter(&a.etypes, &r.etype);
    let f_subcases = lax_filter(&a.subcases, &r.subcase);
    return f_gids && f_eids && f_etypes && f_subcases && f_blocks;
  };
  // determine padding
  let largest: Option<usize> = if args.fmtr.align != Alignment::None {
    to_records(&f06, &all_converters())
      .filter_map(|rec| {
        if should_write(&rec, &args) && rec.block_id != CsvBlockId::Metadata {
          let h = if args.headers {
            col_filter(rec.header_as_iter(), &args)
              .map(|f| f.len())
              .max()
          } else {
            None
          };
          let n = col_filter(rec.to_fields(), &args)
            .map(|f| args.fmtr.to_string(f).len())
            .max();
          return n.max(h);
        } else {
          return None;
        }
      })
      .max()
  } else {
    None
  };
  // padding fn
  let pad = |s: &str| -> String {
    if let Some(w) = largest {
      if s.len() > w {
        return s.to_owned();
      }
      let p1 = w - s.len();
      let ps = p1 / 2;
      let pb = p1 - ps;
      let (lpad, rpad) = match args.fmtr.align {
        Alignment::None => return s.to_owned(),
        Alignment::Right => (p1, 0),
        Alignment::Left => (0, p1),
        Alignment::Center => (pb, ps),
      };
      return format!("{}{}{}", " ".repeat(lpad), s, " ".repeat(rpad),);
    } else {
      return s.to_owned();
    }
  };
  // write blocks
  info!("Writing CSV records...");
  let mut last_header: Option<(&RowHeader, CsvBlockId)> = None;
  for rec in to_records(&f06, &all_converters()) {
    if should_write(&rec, &args) {
      if args.headers {
        let cur_header = &rec.headers;
        let cur_bid = rec.block_id;
        let was_none = last_header.is_none();
        last_header = last_header.or(Some((cur_header, cur_bid)));
        if last_header != Some((cur_header, cur_bid)) || was_none {
          // header change
          last_header = Some((cur_header, cur_bid));
          wtr.write_record(col_filter(rec.header_as_iter(), &args).map(pad))?;
        }
      }
      let flds = col_filter(rec.to_fields(), &args);
      wtr.write_record(flds.map(|f| pad(&args.fmtr.to_string(f))))?;
    }
  }
  info!("All done.");
  // done
  return Ok(());
}
