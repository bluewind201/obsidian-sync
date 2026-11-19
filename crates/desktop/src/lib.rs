//! Shared Onyx Studio (Obsidian Sync) backend & native desktop GUI library.
//! - Windows desktop binary uses 100% pure native Rust (`eframe`/`egui` + `glow`, Zero WebView2 / Zero local ports).
//! - Android JNI (`obsidian-sync-android`) shares `api`, `crypto`, `sync_controller`, `vault`, and `web_assets`.

pub mod api;
pub mod crypto;
pub mod platform;
pub mod sync_controller;
pub mod updater;
pub mod vault;

#[cfg(not(target_os = "android"))]
pub mod flowchart;
#[cfg(not(target_os = "android"))]
pub mod graph_view;
#[cfg(not(target_os = "android"))]
pub mod print_pdf;
#[cfg(not(target_os = "android"))]
pub mod studio_ui;
#[cfg(not(target_os = "android"))]
pub mod sync_ui;
#[cfg(not(target_os = "android"))]
pub mod ui;
#[cfg(not(target_os = "android"))]
pub mod win_clipboard;

#[derive(Debug)]
pub enum AppUserEvent {
    ShowMainWindow,
    ExportPdf {
        output_path: std::path::PathBuf,
        landscape: bool,
        margin_top_cm: f64,
        margin_bottom_cm: f64,
        margin_left_cm: f64,
        margin_right_cm: f64,
        reply: std::sync::mpsc::Sender<Result<(), String>>,
    },
}

#[allow(dead_code)]
mod web_assets {
    include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));
}
pub use web_assets::get_web_asset;

pub use sync_controller::App;

#[cfg(test)]
use obsidian_sync_core::{AppConfig, BackendConfig, Profile};
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
mod lifecycle_tests;
