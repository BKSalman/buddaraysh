use std::path::PathBuf;

use buddaraysh::{udev::run_udev, winit::run_winit};
use tracing::Level;
use tracing_subscriber::{filter::LevelFilter, prelude::*, EnvFilter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging();

    match std::env::var("BUD_BACKEND")
        .unwrap_or(String::from("udev"))
        .as_str()
    {
        "winit" => run_winit()?,
        _ => run_udev()?,
    }

    Ok(())
}

pub fn logging() {
    if let Err(_e) = std::env::var("BUD_LOG") {
        tracing::info!(
            "no log level specified, defaulting to debug level for ytdlp_gui crate only"
        );
        std::env::set_var("BUD_LOG", "none,buddaraysh=debug");
    }

    let journald_layer = tracing_journald::layer().expect("journald should be running");

    let home_dir = std::env::var("HOME").expect("HOME should always be set");
    let logs_dir = PathBuf::from(home_dir).join(".cache/buddaraysh/logs/");

    // Log all `tracing` events to files prefixed with `debug`. Since these
    // files will be written to very frequently, roll the log file every minute.
    let debug_file = tracing_appender::rolling::minutely(&logs_dir, "debug");
    // Log warnings and errors to a separate file. Since we expect these events
    // to occur less frequently, roll that file on a daily basis instead.
    let warn_file = tracing_appender::rolling::daily(&logs_dir, "warnings");

    tracing_subscriber::registry()
        .with(
            EnvFilter::builder()
                .with_env_var("BUD_LOG")
                .with_default_directive(LevelFilter::ERROR.into())
                .from_env_lossy(),
        )
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(debug_file.with_max_level(Level::DEBUG))
                .with_ansi(false),
        )
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(warn_file.with_max_level(Level::WARN))
                .with_ansi(false),
        )
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(std::io::stdout.with_max_level(Level::DEBUG)),
        )
        .with(journald_layer)
        .init();
}
