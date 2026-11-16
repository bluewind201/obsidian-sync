//! Shared Obsidian Sync Studio backend library (used by Windows desktop binary and Android JNI).

pub mod api;
pub mod crypto;
pub mod platform;
pub mod sync_controller;
pub mod vault;

#[derive(Debug)]
pub enum AppUserEvent {
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
