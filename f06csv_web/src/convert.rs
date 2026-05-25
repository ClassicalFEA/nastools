//! Pure conversion logic. Takes a byte slice and an [`Options`] value and
//! returns the CSV text — no I/O outside the supplied byte slice.

use std::io::{BufReader, Cursor};

use csv::Terminator;
use f06::prelude::{ElementType, F06File, OnePassParser};
use nas_csv::from_f06::templates::all_converters;
use nas_csv::from_f06::to_records;
use nas_csv::prelude::{Alignment, CsvBlockId, CsvRecord, RowHeader};

use crate::options::Options;

/// All element types we expose as checkboxes in the UI. Kept here so the
/// component layer does not need to import `f06` directly.
pub const KNOWN_ETYPES: &[ElementType] = ElementType::all();

/// All CSV block ids exposed by [`CsvBlockId::all`].
pub fn known_block_ids() -> &'static [CsvBlockId] {
  return CsvBlockId::all();
}

/// Runs the full F06 → CSV conversion. Returns the CSV text or an error
/// message suitable for display. `name`, if supplied, is recorded as the
/// origin filename so the Metadata block can include it.
pub fn run_conversion(
  bytes: &[u8],
  opts: &Options,
  name: Option<&str>,
) -> Result<String, String> {
  // parse
  let cursor = Cursor::new(bytes);
  let mut f06: F06File = OnePassParser::parse_bufread(BufReader::new(cursor))
    .map_err(|e| format!("parser error: {e:?}"))?;
  if let Some(n) = name {
    f06.filename = Some(n.to_owned());
  }
  f06.merge_blocks(true);
  f06.merge_potential_headers();
  f06.sort_all_blocks();

  // resolve delimiter
  let delim_char = if opts.tab { '\t' } else { opts.delim };
  let delim_byte: u8 = u8::try_from(delim_char as u32)
    .map_err(|_| "delimiter must be an ASCII character".to_owned())?;
  let term = if opts.crlf {
    Terminator::CRLF
  } else {
    Terminator::default()
  };

  let mut buf: Vec<u8> = Vec::new();
  {
    let mut wtr = csv::WriterBuilder::new()
      .delimiter(delim_byte)
      .terminator(term)
      .from_writer(&mut buf);

    let should_write = |r: &CsvRecord| -> bool {
      lax_filter(&opts.csv_blocks, &Some(r.block_id))
        && lax_filter(&opts.gids, &r.gid)
        && lax_filter(&opts.eids, &r.eid)
        && lax_filter(&opts.etypes, &r.etype)
        && lax_filter(&opts.subcases, &r.subcase)
    };

    // padding width
    let largest: Option<usize> = if opts.fmtr.align != Alignment::None {
      to_records(&f06, &all_converters())
        .filter_map(|rec| {
          if should_write(&rec) && rec.block_id != CsvBlockId::Metadata {
            let h = if opts.headers {
              col_filter(rec.header_as_iter(), opts)
                .map(|f| f.len())
                .max()
            } else {
              None
            };
            let n = col_filter(rec.to_fields(), opts)
              .map(|f| opts.fmtr.to_string(f).len())
              .max();
            n.max(h)
          } else {
            None
          }
        })
        .max()
    } else {
      None
    };

    let pad = |s: &str| -> String {
      if let Some(w) = largest {
        if s.len() > w {
          return s.to_owned();
        }
        let p1 = w - s.len();
        let ps = p1 / 2;
        let pb = p1 - ps;
        let (lpad, rpad) = match opts.fmtr.align {
          Alignment::None => return s.to_owned(),
          Alignment::Right => (p1, 0),
          Alignment::Left => (0, p1),
          Alignment::Center => (pb, ps),
        };
        return format!("{}{}{}", " ".repeat(lpad), s, " ".repeat(rpad));
      }
      return s.to_owned();
    };

    let mut last_header: Option<(&RowHeader, CsvBlockId)> = None;
    for rec in to_records(&f06, &all_converters()) {
      if !should_write(&rec) {
        continue;
      }
      if opts.headers {
        let cur_header = rec.headers;
        let cur_bid = rec.block_id;
        let was_none = last_header.is_none();
        last_header = last_header.or(Some((cur_header, cur_bid)));
        if last_header != Some((cur_header, cur_bid)) || was_none {
          last_header = Some((cur_header, cur_bid));
          wtr
            .write_record(col_filter(rec.header_as_iter(), opts).map(pad))
            .map_err(|e| format!("csv write error: {e}"))?;
        }
      }
      let flds = col_filter(rec.to_fields(), opts);
      wtr
        .write_record(flds.map(|f| pad(&opts.fmtr.to_string(f))))
        .map_err(|e| format!("csv write error: {e}"))?;
    }
    wtr.flush().map_err(|e| format!("csv flush error: {e}"))?;
  }

  return String::from_utf8(buf).map_err(|e| format!("utf-8 error: {e}"));
}

/// Filter only if there is at least one entry in the filter.
fn lax_filter<T: PartialEq>(v: &[T], x: &Option<T>) -> bool {
  return v.is_empty()
    || x.is_none()
    || x.as_ref().is_some_and(|k| v.contains(k));
}

/// Filter an iterator over columns according to the user's selection.
fn col_filter<'a, T, I: Iterator<Item = T> + 'a>(
  it: I,
  opts: &'a Options,
) -> impl Iterator<Item = T> + 'a {
  return it
    .enumerate()
    .filter(|(i, _)| opts.cols.is_empty() || opts.cols.contains(&(i + 1)))
    .map(|(_, v)| v);
}
