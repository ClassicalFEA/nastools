//! A free-form text input whose value is held in internal state so the
//! user's keystrokes aren't clobbered by re-renders from the parent.
//!
//! The parent only sees `on_change` callbacks; it does **not** drive the
//! input's value after the initial mount. This lets the user type partial
//! tokens like "1, " without the parser collapsing them away on each
//! keystroke.

use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, InputEvent};
use yew::prelude::*;

/// Properties for [`RawTextInput`].
#[derive(Properties, PartialEq)]
pub struct RawTextInputProps {
  /// Initial value, only consulted on the first render.
  #[prop_or_default]
  pub initial: AttrValue,
  /// Placeholder text.
  #[prop_or_default]
  pub placeholder: AttrValue,
  /// Maximum number of characters (use 0 for no limit).
  #[prop_or(0)]
  pub maxlength: u32,
  /// Whether the input is disabled.
  #[prop_or(false)]
  pub disabled: bool,
  /// Called with the new raw text on every keystroke.
  pub on_change: Callback<String>,
}

/// An uncontrolled text input.
#[function_component(RawTextInput)]
pub fn raw_text_input(props: &RawTextInputProps) -> Html {
  let text = use_state(|| props.initial.to_string());

  let on_change = props.on_change.clone();
  let text_state = text.clone();
  let oninput = Callback::from(move |e: InputEvent| {
    let raw = e
      .target()
      .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
      .map(|i| i.value())
      .unwrap_or_default();
    text_state.set(raw.clone());
    on_change.emit(raw);
  });

  let maxlength_attr = if props.maxlength == 0 {
    None
  } else {
    Some(AttrValue::from(props.maxlength.to_string()))
  };

  return html! {
    <input
      type="text"
      value={(*text).clone()}
      placeholder={props.placeholder.clone()}
      maxlength={maxlength_attr}
      disabled={props.disabled}
      {oninput}
    />
  };
}
