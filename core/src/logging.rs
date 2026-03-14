// Dual logging system: console + systemd-journald

use crate::error::{GeminiAudioError, Result};
use tracing::{info, warn, error, debug, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use std::path::PathBuf;

/// Initialize logging system with dual output
pub fn init_logging(log_level: &str, log_to_console: bool, log_to_journald: bool, data_dir: &PathBuf) -> Result<()> {
    let level = parse_log_level(log_level);
    
    let mut layers = Vec::new();

    // Console layer
    if log_to_console {
        let console_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .with_filter(EnvFilter::from_default_env().add_directive(level.into()));
        
        layers.push(console_layer.boxed());
    }

    // File layer for application logs
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to create log directory: {}", e)))?;
    
    let file_appender = tracing_appender::rolling::never(&log_dir, "gemini-audio.log");
    
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_appender)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .json()
        .with_filter(EnvFilter::from_default_env().add_directive(level.into()));
    
    layers.push(file_layer.boxed());

    // Journald layer (if available and requested)
    if log_to_journald {
        // Note: systemd-journald integration would require additional setup
        // For now, we'll just use console and file logging
        info!("Journald logging requested but not yet implemented");
    }

    tracing_subscriber::registry()
        .with(layers)
        .try_init()
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to initialize logging: {}", e)))?;

    Ok(())
}

/// Parse log level string to tracing Level
fn parse_log_level(level_str: &str) -> Level {
    match level_str.to_lowercase().as_str() {
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        "trace" => Level::TRACE,
        _ => Level::INFO,
    }
}

/// Log session event
pub fn log_session_event(session_id: i64, event: &str, details: Option<&str>) {
    match details {
        Some(detail) => info!(session_id = session_id, event = event, details = detail),
        None => info!(session_id = session_id, event = event),
    }
}

/// Log error event
pub fn log_error_event(session_id: Option<i64>, error: &str, details: Option<&str>) {
    match (session_id, details) {
        (Some(id), Some(detail)) => error!(session_id = id, error = error, details = detail),
        (Some(id), None) => error!(session_id = id, error = error),
        (None, Some(detail)) => error!(error = error, details = detail),
        (None, None) => error!(error = error),
    }
}

/// Log warning event
pub fn log_warning_event(session_id: Option<i64>, warning: &str, details: Option<&str>) {
    match (session_id, details) {
        (Some(id), Some(detail)) => warn!(session_id = id, warning = warning, details = detail),
        (Some(id), None) => warn!(session_id = id, warning = warning),
        (None, Some(detail)) => warn!(warning = warning, details = detail),
        (None, None) => warn!(warning = warning),
    }
}

/// Log debug event
pub fn log_debug_event(session_id: Option<i64>, debug_msg: &str, details: Option<&str>) {
    match (session_id, details) {
        (Some(id), Some(detail)) => debug!(session_id = id, debug = debug_msg, details = detail),
        (Some(id), None) => debug!(session_id = id, debug = debug_msg),
        (None, Some(detail)) => debug!(debug = debug_msg, details = detail),
        (None, None) => debug!(debug = debug_msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_level() {
        assert_eq!(parse_log_level("debug"), Level::DEBUG);
        assert_eq!(parse_log_level("INFO"), Level::INFO);
        assert_eq!(parse_log_level("warn"), Level::WARN);
        assert_eq!(parse_log_level("ERROR"), Level::ERROR);
        assert_eq!(parse_log_level("trace"), Level::TRACE);
        assert_eq!(parse_log_level("invalid"), Level::INFO);
    }
}
