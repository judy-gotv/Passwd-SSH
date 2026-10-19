//! OpsDeck — local-first SSH workstation.
//!
//! Entry point: logging, directories, tokio runtime, eframe window.
//!
//! Author: Passwd Word

mod app;
mod update;

use app::OpsDeckApp;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("opsdeck=info,opsdeck_ssh=info")),
        )
        .init();

    // A multi-threaded runtime the whole app shares; eframe runs on the main
    // thread while background tasks use this runtime.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("opsdeck-worker")
        .build()?;
    let _guard = rt.enter();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1024.0, 640.0])
            .with_title("OpsDeck"),
        ..Default::default()
    };

    // Build the app (async init: DB, settings) before eframe takes the thread.
    let app = rt.block_on(OpsDeckApp::new_async())?;

    eframe::run_native(
        "OpsDeck",
        native_options,
        Box::new(|_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )?;
    Ok(())
}
