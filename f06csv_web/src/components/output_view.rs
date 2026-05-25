//! Output view: CSV preview + download button.

use js_sys::Array;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
  Blob, BlobPropertyBag, HtmlAnchorElement, Url,
};
use yew::prelude::*;

/// Maximum number of preview lines rendered into the `<pre>` block.
pub const PREVIEW_LINES: usize = 500;

/// Properties for [`OutputView`].
#[derive(Properties, PartialEq)]
pub struct OutputViewProps {
  /// The most recent conversion result, if any.
  pub csv: Option<AttrValue>,
  /// Suggested download filename (without extension).
  pub base_name: AttrValue,
  /// Whether a conversion is currently in-flight.
  pub running: bool,
  /// Last error message, if any.
  pub error: Option<AttrValue>,
}

/// Renders the preview pane plus a download button.
#[function_component(OutputView)]
pub fn output_view(props: &OutputViewProps) -> Html {
  if let Some(err) = &props.error {
    return html! {
      <div class="banner error">{err.clone()}</div>
    };
  }

  let csv = match &props.csv {
    Some(c) => c.clone(),
    None => {
      let msg = if props.running {
        html! { <><span class="spinner" />{"Converting…"}</> }
      } else {
        html! { {"No output yet. Choose a file and click \"Convert\"."} }
      };
      return html! { <div>{msg}</div> };
    }
  };

  let total_lines = csv.lines().count();
  let preview: String = csv
    .lines()
    .take(PREVIEW_LINES)
    .collect::<Vec<_>>()
    .join("\n");

  let base = props.base_name.clone();
  let csv_for_download = csv.clone();
  let on_download = Callback::from(move |_: MouseEvent| {
    if let Err(e) = trigger_download(&base, &csv_for_download) {
      log::error!("download failed: {e}");
    }
  });

  let line_count_text = if total_lines > PREVIEW_LINES {
    format!(
      "Showing first {PREVIEW_LINES} of {total_lines} lines. Download for full output."
    )
  } else {
    format!("{total_lines} line(s).")
  };

  return html! {
    <div>
      <div class="actions" style="margin-bottom: 0.75rem;">
        <button class="button-primary" onclick={on_download}>
          {"Download CSV"}
        </button>
        <span class="preview-meta">{line_count_text}</span>
      </div>
      <pre class="preview">{preview}</pre>
    </div>
  };
}

/// Triggers a browser download for `csv` using `<filename>.csv`.
fn trigger_download(filename: &str, csv: &str) -> Result<(), String> {
  let parts = Array::new();
  parts.push(&JsValue::from_str(csv));
  let opts = BlobPropertyBag::new();
  opts.set_type("text/csv");
  let blob =
    Blob::new_with_str_sequence_and_options(&parts, &opts)
      .map_err(|e| format!("blob error: {e:?}"))?;
  let url = Url::create_object_url_with_blob(&blob)
    .map_err(|e| format!("url error: {e:?}"))?;
  let document = gloo_utils::document();
  let anchor: HtmlAnchorElement = document
    .create_element("a")
    .map_err(|e| format!("create error: {e:?}"))?
    .dyn_into()
    .map_err(|_| "could not cast to HtmlAnchorElement".to_owned())?;
  anchor.set_href(&url);
  anchor.set_download(&format!("{filename}.csv"));
  let body = document.body().ok_or_else(|| "no body".to_owned())?;
  body
    .append_child(&anchor)
    .map_err(|e| format!("append error: {e:?}"))?;
  anchor.click();
  body
    .remove_child(&anchor)
    .map_err(|e| format!("remove error: {e:?}"))?;
  let _ = Url::revoke_object_url(&url);
  return Ok(());
}
