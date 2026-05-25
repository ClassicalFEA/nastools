//! Top-of-page header with title and dark-mode toggle.

use gloo_utils::document;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;
use yew::prelude::*;

/// Properties for [`HeaderBar`].
#[derive(Properties, PartialEq)]
pub struct HeaderBarProps {
  /// Current theme: `"light"` or `"dark"`.
  pub theme: AttrValue,
  /// Callback fired with the new theme after a toggle.
  pub on_toggle: Callback<String>,
}

/// Renders the title bar and dark-mode toggle button.
#[function_component(HeaderBar)]
pub fn header_bar(props: &HeaderBarProps) -> Html {
  let theme = props.theme.clone();
  let on_toggle = props.on_toggle.clone();
  let onclick = Callback::from(move |_: MouseEvent| {
    let next = if theme == "dark" { "light" } else { "dark" };
    apply_theme(next);
    on_toggle.emit(next.to_owned());
  });

  let icon = if props.theme == "dark" { "☀" } else { "☾" };
  let label = if props.theme == "dark" {
    "light mode"
  } else {
    "dark mode"
  };

  return html! {
    <header class="app-header">
      <div class="app-header-title">
        <h1>{"f06csv"}<span class="app-header-tagline">
          {" — convert Nastran F06 to CSV"}
        </span></h1>
        <div class="app-header-byline">
          {"by "}
          <span class="app-header-project">
            {"Bruno Borges Paschoalinoto"}
          </span>
          {", for the "}
          <span class="app-header-project">{"ClassicalFEA"}</span>
          {" project"}
        </div>
        <div class="app-header-repo-line">
          <a
            class="app-header-repo"
            href="https://github.com/ClassicalFEA/nastools"
            target="_blank"
            rel="noopener noreferrer"
          >
            {"github.com/ClassicalFEA/nastools"}
          </a>
          <span class="app-header-repo-note">
            {"— prebuilt binaries and other tools available"}
          </span>
        </div>
      </div>
      <button class="theme-toggle" {onclick}>
        {format!("{icon}  {label}")}
      </button>
    </header>
  };
}

/// Apply the given theme name to the document `<html>` element's
/// `data-theme` attribute.
pub fn apply_theme(theme: &str) {
  if let Some(root) = document()
    .document_element()
    .and_then(|e| e.dyn_into::<HtmlElement>().ok())
  {
    let _ = root.set_attribute("data-theme", theme);
  }
}
