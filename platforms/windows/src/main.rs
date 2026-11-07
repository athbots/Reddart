//! Reddart Video Editor — Windows Application
//!
//! This is the main entry point for the Windows desktop editor.
//! It creates a wgpu-powered window with egui UI and the custom
//! GPU-rendered timeline canvas.

mod app;
mod gpu;
pub mod ui;

use anyhow::Result;

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    log::info!("Reddart Video Editor v{}", env!("CARGO_PKG_VERSION"));
    log::info!("Starting application...");

    // Run the application
    app::run()
}
