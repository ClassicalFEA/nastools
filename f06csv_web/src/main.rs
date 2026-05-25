//! Trunk entry point.

fn main() {
  wasm_logger::init(wasm_logger::Config::default());
  yew::Renderer::<f06csv_web::app::App>::new().render();
}
