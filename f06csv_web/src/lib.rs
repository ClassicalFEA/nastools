//! Yew-based web UI that converts Nastran F06 output to CSV in the browser.
//!
//! See [`app::App`] for the top-level component.

#![allow(clippy::needless_return)]

pub mod app;
pub mod components;
pub mod convert;
pub mod options;
pub mod storage;
