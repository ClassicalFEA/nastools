//! The options form: every CLI flag of `f06csv` exposed as form controls.

use f06::prelude::ElementType;
use nas_csv::prelude::{Alignment, BlankDisplay, CsvBlockId};
use wasm_bindgen::JsCast;
use web_sys::{Event, HtmlInputElement, HtmlSelectElement};
use yew::prelude::*;

use crate::components::raw_text_input::RawTextInput;
use crate::convert::{known_block_ids, KNOWN_ETYPES};
use crate::options::{format_list, parse_list, Options};

/// Properties for [`OptionsForm`].
#[derive(Properties, PartialEq)]
pub struct OptionsFormProps {
  /// Current options value.
  pub options: Options,
  /// Emitted whenever the user edits the form.
  pub on_change: Callback<Options>,
}

/// Renders all options controls and emits the updated [`Options`] on every
/// change.
#[function_component(OptionsForm)]
pub fn options_form(props: &OptionsFormProps) -> Html {
  let opts = &props.options;
  let on_change = props.on_change.clone();
  let emit = move |new_opts: Options| on_change.emit(new_opts);

  // -- block checkboxes ----------------------------------------------------
  let blocks_view = {
    let selected = opts.csv_blocks.clone();
    let opts_clone = opts.clone();
    let emit = emit.clone();
    known_block_ids()
      .iter()
      .copied()
      .map(|bid| {
        let checked = selected.is_empty() || selected.contains(&bid);
        let opts_clone = opts_clone.clone();
        let emit = emit.clone();
        let onchange = Callback::from(move |e: Event| {
          let input = e
            .target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
          let want = input.map(|i| i.checked()).unwrap_or(false);
          let mut new_opts = opts_clone.clone();
          // Treat an empty selection as "all on". Editing the first
          // checkbox transitions to an explicit list.
          if new_opts.csv_blocks.is_empty() {
            new_opts.csv_blocks = known_block_ids().to_vec();
          }
          if want {
            if !new_opts.csv_blocks.contains(&bid) {
              new_opts.csv_blocks.push(bid);
            }
          } else {
            new_opts.csv_blocks.retain(|b| *b != bid);
          }
          // If the user re-checked everything, collapse back to "all".
          if new_opts.csv_blocks.len() == known_block_ids().len() {
            new_opts.csv_blocks.clear();
          }
          emit(new_opts);
        });
        html! {
          <label class="inline">
            <input type="checkbox" {checked} {onchange} />
            {bid.display_name()}
          </label>
        }
      })
      .collect::<Html>()
  };

  // -- element-type checkboxes --------------------------------------------
  let etypes_view = {
    let selected = opts.etypes.clone();
    let opts_clone = opts.clone();
    let emit = emit.clone();
    KNOWN_ETYPES
      .iter()
      .copied()
      .map(|et| {
        let checked = selected.contains(&et);
        let opts_clone = opts_clone.clone();
        let emit = emit.clone();
        let onchange = Callback::from(move |e: Event| {
          let input = e
            .target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
          let want = input.map(|i| i.checked()).unwrap_or(false);
          let mut new_opts = opts_clone.clone();
          if want {
            if !new_opts.etypes.contains(&et) {
              new_opts.etypes.push(et);
            }
          } else {
            new_opts.etypes.retain(|x| *x != et);
          }
          emit(new_opts);
        });
        html! {
          <label class="inline">
            <input type="checkbox" {checked} {onchange} />
            {format!("{et}")}
          </label>
        }
      })
      .collect::<Html>()
  };

  // -- numeric list inputs (gids, eids, subcases, cols) -------------------
  //
  // We use an *uncontrolled* `RawTextInput` so the user's keystrokes (commas,
  // trailing spaces, etc.) survive the round-trip through the parser. The
  // parent only sees parsed `Vec<usize>` values; the displayed text is owned
  // by the input itself.
  let num_list = |label: &'static str,
                  help: &'static str,
                  current: &[usize],
                  field: NumField|
   -> Html {
    let initial = AttrValue::from(format_list(current));
    let placeholder = AttrValue::from(help);
    let opts_clone = opts.clone();
    let emit = emit.clone();
    let on_change = Callback::from(move |raw: String| {
      let parsed = parse_list::<usize>(&raw).unwrap_or_default();
      let mut new_opts = opts_clone.clone();
      match field {
        NumField::Gids => new_opts.gids = parsed,
        NumField::Eids => new_opts.eids = parsed,
        NumField::Subcases => new_opts.subcases = parsed,
        NumField::Cols => new_opts.cols = parsed,
      }
      emit(new_opts);
    });
    html! {
      <div>
        <label>{label}</label>
        <RawTextInput {initial} {placeholder} {on_change} />
      </div>
    }
  };

  // -- delimiter ---------------------------------------------------------
  let delim_initial = AttrValue::from(opts.delim.to_string());
  let delim_on_change = {
    let opts_clone = opts.clone();
    let emit = emit.clone();
    Callback::from(move |raw: String| {
      let ch = raw.chars().next().unwrap_or(',');
      let mut new_opts = opts_clone.clone();
      new_opts.delim = ch;
      emit(new_opts);
    })
  };

  // -- bool toggles ------------------------------------------------------
  let bool_toggle =
    |label: &'static str, value: bool, field: BoolField| -> Html {
      let opts_clone = opts.clone();
      let emit = emit.clone();
      let onchange = Callback::from(move |e: Event| {
        let input = e
          .target()
          .and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
        let checked = input.map(|i| i.checked()).unwrap_or(false);
        let mut new_opts = opts_clone.clone();
        match field {
          BoolField::Headers => new_opts.headers = checked,
          BoolField::Tab => new_opts.tab = checked,
          BoolField::Crlf => new_opts.crlf = checked,
          BoolField::NoSci => new_opts.fmtr.reals.no_scientific = checked,
          BoolField::OmitPlus => {
            new_opts.fmtr.reals.no_superfluous_plus = checked
          }
          BoolField::SmallE => new_opts.fmtr.reals.small_e = checked,
        }
        emit(new_opts);
      });
      html! {
        <label class="inline">
          <input type="checkbox" checked={value} {onchange} />
          {label}
        </label>
      }
    };

  // -- alignment select --------------------------------------------------
  let align_value = match opts.fmtr.align {
    Alignment::None => "none",
    Alignment::Left => "left",
    Alignment::Right => "right",
    Alignment::Center => "center",
  };
  let align_onchange = {
    let opts_clone = opts.clone();
    let emit = emit.clone();
    Callback::from(move |e: Event| {
      let sel = e
        .target()
        .and_then(|t| t.dyn_into::<HtmlSelectElement>().ok());
      let raw = sel.map(|s| s.value()).unwrap_or_else(|| "none".into());
      let mut new_opts = opts_clone.clone();
      new_opts.fmtr.align = match raw.as_str() {
        "left" => Alignment::Left,
        "right" => Alignment::Right,
        "center" => Alignment::Center,
        _ => Alignment::None,
      };
      emit(new_opts);
    })
  };

  // -- blanks select -----------------------------------------------------
  let blanks_value = match opts.fmtr.blanks {
    BlankDisplay::Zero => "zero",
    BlankDisplay::Space => "space",
    BlankDisplay::Dash => "dash",
    BlankDisplay::Dashes => "dashes",
    BlankDisplay::Empty => "empty",
  };
  let blanks_onchange = {
    let opts_clone = opts.clone();
    let emit = emit.clone();
    Callback::from(move |e: Event| {
      let sel = e
        .target()
        .and_then(|t| t.dyn_into::<HtmlSelectElement>().ok());
      let raw = sel.map(|s| s.value()).unwrap_or_else(|| "dashes".into());
      let mut new_opts = opts_clone.clone();
      new_opts.fmtr.blanks = match raw.as_str() {
        "zero" => BlankDisplay::Zero,
        "space" => BlankDisplay::Space,
        "dash" => BlankDisplay::Dash,
        "empty" => BlankDisplay::Empty,
        _ => BlankDisplay::Dashes,
      };
      emit(new_opts);
    })
  };

  // -- decimals input ----------------------------------------------------
  let dec_initial = AttrValue::from(
    opts
      .fmtr
      .reals
      .dec_places
      .map(|n| n.to_string())
      .unwrap_or_default(),
  );
  let dec_on_change = {
    let opts_clone = opts.clone();
    let emit = emit.clone();
    Callback::from(move |raw: String| {
      let mut new_opts = opts_clone.clone();
      new_opts.fmtr.reals.dec_places = if raw.trim().is_empty() {
        None
      } else {
        raw.trim().parse::<usize>().ok()
      };
      emit(new_opts);
    })
  };

  return html! {
    <div>
      <h3>{"CSV blocks"}</h3>
      <div class="checkbox-grid">{ blocks_view }</div>

      <h3>{"Filters"}</h3>
      <p class="section-hint">
        {"Leave any filter empty to include everything in that category."}
      </p>
      <div class="field-row">
        { num_list("Grid IDs", "e.g. 1, 2, 3", &opts.gids, NumField::Gids) }
        { num_list("Element IDs", "e.g. 10, 20", &opts.eids, NumField::Eids) }
        { num_list("Subcases", "e.g. 1, 2", &opts.subcases, NumField::Subcases) }
        { num_list("Columns (1–11)", "e.g. 1, 2, 5", &opts.cols, NumField::Cols) }
      </div>
      <label>{"Element types"}</label>
      <div class="checkbox-grid">{ etypes_view }</div>

      <h3>{"Output"}</h3>
      <div class="field-row">
        <div>
          <label>{"Delimiter"}</label>
          <RawTextInput
            initial={delim_initial}
            maxlength={1u32}
            disabled={opts.tab}
            on_change={delim_on_change}
          />
        </div>
        <div>
          <label>{"Alignment"}</label>
          <select onchange={align_onchange}>
            { vec!["none", "left", "right", "center"].into_iter().map(|v| html! {
                <option value={v} selected={align_value == v}>{v}</option>
            }).collect::<Html>() }
          </select>
        </div>
        <div>
          <label>{"Blanks"}</label>
          <select onchange={blanks_onchange}>
            { vec!["dashes", "dash", "zero", "space", "empty"].into_iter().map(|v| html! {
                <option value={v} selected={blanks_value == v}>{v}</option>
            }).collect::<Html>() }
          </select>
        </div>
        <div>
          <label>{"Decimals"}</label>
          <RawTextInput
            initial={dec_initial}
            placeholder="free-form"
            on_change={dec_on_change}
          />
        </div>
      </div>

      <div class="checkbox-grid" style="margin-top: 0.5rem;">
        { bool_toggle("Write headers", opts.headers, BoolField::Headers) }
        { bool_toggle("Use tab delimiter", opts.tab, BoolField::Tab) }
        { bool_toggle("CRLF line endings", opts.crlf, BoolField::Crlf) }
        { bool_toggle("Decimal (no scientific)", opts.fmtr.reals.no_scientific, BoolField::NoSci) }
        { bool_toggle("Omit '+' on positives", opts.fmtr.reals.no_superfluous_plus, BoolField::OmitPlus) }
        { bool_toggle("Small 'e' exponent", opts.fmtr.reals.small_e, BoolField::SmallE) }
      </div>
    </div>
  };
}

/// Which numeric list field a callback should update.
#[derive(Copy, Clone)]
enum NumField {
  /// Grid point IDs.
  Gids,
  /// Element IDs.
  Eids,
  /// Subcase IDs.
  Subcases,
  /// Column indices.
  Cols,
}

/// Which boolean toggle a callback should update.
#[derive(Copy, Clone)]
enum BoolField {
  /// Write CSV headers on header change.
  Headers,
  /// Use a tab as the delimiter.
  Tab,
  /// Use CRLF line endings.
  Crlf,
  /// Disable scientific notation for floats.
  NoSci,
  /// Omit the leading `+` on non-negative floats.
  OmitPlus,
  /// Use lowercase `e` in exponents.
  SmallE,
}

// Keep imports referenced in case clippy strips unused use statements.
#[allow(dead_code)]
fn _force_imports(_: ElementType, _: CsvBlockId) {}
