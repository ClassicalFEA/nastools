//! Top-level Yew app: wires file upload, options form, and CSV output.

use gloo_storage::{LocalStorage, Storage};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Event, HtmlInputElement};
use yew::prelude::*;

use crate::components::file_drop::FileDrop;
use crate::components::header_bar::{apply_theme, HeaderBar};
use crate::components::options_form::OptionsForm;
use crate::components::output_view::OutputView;
use crate::convert::run_conversion;
use crate::options::{cli_flags, Options};
use crate::storage::{load_file, save_file};

/// LocalStorage key for persisted options.
const OPTIONS_KEY: &str = "f06csv_web.options.v1";

/// LocalStorage key for persisted theme.
const THEME_KEY: &str = "f06csv_web.theme.v1";

/// LocalStorage key for persisted auto-update preference.
const AUTO_UPDATE_KEY: &str = "f06csv_web.auto_update.v1";

/// Picks the initial theme. The user's explicit choice (in localStorage)
/// always wins; otherwise we honour the OS-level `prefers-color-scheme`
/// media query, defaulting to `"light"` if even that is unavailable.
fn initial_theme() -> String {
  if let Ok(stored) = LocalStorage::get::<String>(THEME_KEY) {
    return stored;
  }
  if let Some(win) = web_sys::window() {
    if let Ok(Some(mql)) = win.match_media("(prefers-color-scheme: dark)") {
      if mql.matches() {
        return "dark".to_owned();
      }
    }
  }
  return "light".to_owned();
}

/// Top-level application component.
#[function_component(App)]
pub fn app() -> Html {
  // -------- state --------------------------------------------------------
  let options =
    use_state(|| LocalStorage::get::<Options>(OPTIONS_KEY).unwrap_or_default());
  let theme = use_state(initial_theme);
  let auto_update =
    use_state(|| LocalStorage::get::<bool>(AUTO_UPDATE_KEY).unwrap_or(true));
  // Try to restore a previously-cached upload so the app is useful right
  // after a reload.
  let restored = load_file();
  let file_bytes: UseStateHandle<Option<Vec<u8>>> =
    use_state(|| restored.as_ref().map(|(_, b)| b.clone()));
  let filename: UseStateHandle<Option<AttrValue>> =
    use_state(|| restored.as_ref().map(|(n, _)| AttrValue::from(n.clone())));
  let csv_output: UseStateHandle<Option<AttrValue>> = use_state(|| None);
  let error: UseStateHandle<Option<AttrValue>> = use_state(|| None);
  let running = use_state(|| false);

  // -------- apply theme on first paint -----------------------------------
  {
    let theme = theme.clone();
    use_effect_with(theme.clone(), move |theme| {
      apply_theme(theme.as_str());
      || ()
    });
  }

  // -------- callbacks ----------------------------------------------------
  let on_options_change = {
    let options = options.clone();
    Callback::from(move |new_opts: Options| {
      let _ = LocalStorage::set(OPTIONS_KEY, &new_opts);
      options.set(new_opts);
    })
  };

  let on_theme_toggle = {
    let theme = theme.clone();
    Callback::from(move |new_theme: String| {
      let _ = LocalStorage::set(THEME_KEY, &new_theme);
      theme.set(new_theme);
    })
  };

  let on_file = {
    let file_bytes = file_bytes.clone();
    let filename = filename.clone();
    let csv_output = csv_output.clone();
    let error = error.clone();
    Callback::from(move |(name, bytes): (String, Vec<u8>)| {
      // Best-effort cache for next reload; we just log on failure.
      match save_file(&name, &bytes) {
        Ok(true) => log::info!("cached {name} for next reload"),
        Ok(false) => log::info!(
          "{name} is too large to cache (>3 MiB compressed), skipping"
        ),
        Err(e) => log::warn!("could not cache {name}: {e}"),
      }
      filename.set(Some(AttrValue::from(name)));
      file_bytes.set(Some(bytes));
      csv_output.set(None);
      error.set(None);
    })
  };

  let on_file_error = {
    let error = error.clone();
    Callback::from(move |msg: String| error.set(Some(AttrValue::from(msg))))
  };

  let on_convert = {
    let file_bytes = file_bytes.clone();
    let options = options.clone();
    let filename = filename.clone();
    let csv_output = csv_output.clone();
    let error = error.clone();
    let running = running.clone();
    Callback::from(move |_: MouseEvent| {
      let Some(bytes) = (*file_bytes).clone() else {
        error.set(Some(AttrValue::from(
          "Please choose an F06 file first.".to_owned(),
        )));
        return;
      };
      let opts = (*options).clone();
      let name: Option<String> = (*filename).as_ref().map(|n| n.to_string());
      let csv_output = csv_output.clone();
      let error = error.clone();
      let running = running.clone();
      running.set(true);
      error.set(None);
      csv_output.set(None);
      // Yield to the browser so the spinner can paint, then convert.
      spawn_local(async move {
        let result = run_conversion(&bytes, &opts, name.as_deref());
        match result {
          Ok(csv) => {
            csv_output.set(Some(AttrValue::from(csv)));
            error.set(None);
          }
          Err(e) => {
            csv_output.set(None);
            error.set(Some(AttrValue::from(e)));
          }
        }
        running.set(false);
      });
    })
  };

  let on_auto_update_toggle = {
    let auto_update = auto_update.clone();
    Callback::from(move |e: Event| {
      let checked = e
        .target()
        .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
        .map(|i| i.checked())
        .unwrap_or(false);
      let _ = LocalStorage::set(AUTO_UPDATE_KEY, checked);
      auto_update.set(checked);
    })
  };

  // -------- auto-update effect ------------------------------------------
  //
  // Whenever the options, the loaded file, or the auto-update preference
  // changes, re-run the conversion in the background — but only if the
  // user has the toggle on and there's actually a file loaded.
  {
    let options_dep = (*options).clone();
    let file_bytes = file_bytes.clone();
    let filename = filename.clone();
    let csv_output = csv_output.clone();
    let error = error.clone();
    let running = running.clone();
    let auto_update = auto_update.clone();
    use_effect_with(
      (options_dep, *auto_update, (*file_bytes).is_some()),
      move |(opts, on, has_file)| {
        if *on && *has_file {
          if let Some(bytes) = (*file_bytes).clone() {
            let opts = opts.clone();
            let name: Option<String> =
              (*filename).as_ref().map(|n| n.to_string());
            let csv_output = csv_output.clone();
            let error = error.clone();
            let running = running.clone();
            running.set(true);
            spawn_local(async move {
              match run_conversion(&bytes, &opts, name.as_deref()) {
                Ok(csv) => {
                  csv_output.set(Some(AttrValue::from(csv)));
                  error.set(None);
                }
                Err(e) => {
                  csv_output.set(None);
                  error.set(Some(AttrValue::from(e)));
                }
              }
              running.set(false);
            });
          }
        }
        || ()
      },
    );
  }

  // -------- derived ------------------------------------------------------
  let base_name = filename
    .as_ref()
    .map(|n| {
      // strip extension if any
      let s = n.as_str();
      match s.rfind('.') {
        Some(i) => s[..i].to_owned(),
        None => s.to_owned(),
      }
    })
    .unwrap_or_else(|| "output".to_owned());

  let convert_disabled = file_bytes.is_none() || *running;
  let convert_label = if *running {
    html! { <><span class="spinner" />{"Converting…"}</> }
  } else {
    html! { {"Convert to CSV"} }
  };

  return html! {
    <div class="container">
      <HeaderBar
        theme={AttrValue::from((*theme).clone())}
        on_toggle={on_theme_toggle}
      />

      <div class="panel">
        <FileDrop
          filename={(*filename).clone()}
          on_file={on_file}
          on_error={on_file_error}
        />
      </div>

      <div class="layout">
        <div class="panel">
          <div class="toolbar">
            <button
              class="button-primary"
              disabled={convert_disabled}
              onclick={on_convert}
            >
              { convert_label }
            </button>
            <label class="inline">
              <input
                type="checkbox"
                checked={*auto_update}
                onchange={on_auto_update_toggle}
              />
              {"Auto-update on change"}
            </label>
            <span class="grow" />
            if *running {
              <span class="preview-meta">{"Converting…"}</span>
            }
          </div>
          <label class="cli-flags-row">
            <span class="cli-flags-label">
              {"Equivalent CLI:"}
            </span>
            <input
              class="cli-flags"
              type="text"
              readonly=true
              spellcheck="false"
              value={cli_flags(&options)}
              onclick={Callback::from(|e: MouseEvent| {
                if let Some(t) = e.target() {
                  if let Ok(i) = t.dyn_into::<web_sys::HtmlInputElement>() {
                    i.select();
                  }
                }
              })}
            />
          </label>
          <OptionsForm
            options={(*options).clone()}
            on_change={on_options_change}
          />
        </div>

        <div class="panel">
          <h3>{"Output"}</h3>
          <OutputView
            csv={(*csv_output).clone()}
            base_name={AttrValue::from(base_name)}
            running={*running}
            error={(*error).clone()}
          />
        </div>
      </div>
    </div>
  };
}
