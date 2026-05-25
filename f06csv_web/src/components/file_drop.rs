//! File-upload component with drag-and-drop and click-to-browse.

use gloo_file::futures::read_as_bytes;
use gloo_file::Blob;
use wasm_bindgen_futures::spawn_local;
use web_sys::{DragEvent, Event, HtmlInputElement};
use yew::prelude::*;

/// Properties for [`FileDrop`].
#[derive(Properties, PartialEq)]
pub struct FileDropProps {
  /// Most recently selected filename, if any (for display).
  pub filename: Option<AttrValue>,
  /// Called with `(filename, bytes)` once the file is fully read.
  pub on_file: Callback<(String, Vec<u8>)>,
  /// Called with a user-facing error message if reading fails.
  pub on_error: Callback<String>,
}

/// Drag-and-drop + click-to-browse file input.
#[function_component(FileDrop)]
pub fn file_drop(props: &FileDropProps) -> Html {
  let dragging = use_state(|| false);
  let input_ref = use_node_ref();

  let read_file = {
    let on_file = props.on_file.clone();
    let on_error = props.on_error.clone();
    move |file: web_sys::File| {
      let name = file.name();
      let blob: Blob = file.into();
      let on_file = on_file.clone();
      let on_error = on_error.clone();
      spawn_local(async move {
        match read_as_bytes(&blob).await {
          Ok(bytes) => on_file.emit((name, bytes)),
          Err(e) => on_error.emit(format!("could not read file: {e}")),
        }
      });
    }
  };

  let onchange = {
    let read_file = read_file.clone();
    let input_ref = input_ref.clone();
    Callback::from(move |_: Event| {
      if let Some(input) = input_ref.cast::<HtmlInputElement>() {
        if let Some(files) = input.files() {
          if let Some(file) = files.get(0) {
            read_file(file);
          }
        }
      }
    })
  };

  let ondrop = {
    let read_file = read_file.clone();
    let dragging = dragging.clone();
    Callback::from(move |e: DragEvent| {
      e.prevent_default();
      dragging.set(false);
      if let Some(dt) = e.data_transfer() {
        if let Some(files) = dt.files() {
          if let Some(file) = files.get(0) {
            read_file(file);
          }
        }
      }
    })
  };

  let ondragover = {
    let dragging = dragging.clone();
    Callback::from(move |e: DragEvent| {
      e.prevent_default();
      dragging.set(true);
    })
  };

  let ondragleave = {
    let dragging = dragging.clone();
    Callback::from(move |_: DragEvent| {
      dragging.set(false);
    })
  };

  let onclick = {
    let input_ref = input_ref.clone();
    Callback::from(move |_: MouseEvent| {
      if let Some(input) = input_ref.cast::<HtmlInputElement>() {
        input.click();
      }
    })
  };

  let mut classes = classes!("drop-zone");
  if *dragging {
    classes.push("dragging");
  }

  return html! {
    <div
      class={classes}
      {onclick}
      {ondrop}
      {ondragover}
      {ondragleave}
    >
      <input
        ref={input_ref}
        type="file"
        accept=".f06,.F06,text/plain"
        {onchange}
      />
      <div>
        {"drop an F06 file here, or click to browse"}
      </div>
      {
        if let Some(name) = &props.filename {
          html! { <div class="filename">{name.clone()}</div> }
        } else {
          Html::default()
        }
      }
    </div>
  };
}
