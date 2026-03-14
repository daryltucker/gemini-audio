// Gemini Audio Core Library
// Portable core for Gemini 2.5 Flash Native Audio — no platform-specific deps.
// Used by the desktop TUI (app/) and the Android app (android/).

uniffi::setup_scaffolding!();

pub mod config;
pub mod error;
pub mod audio;
pub mod capabilities;
pub mod database;
pub mod client;
pub mod logging;
pub mod prompts;
pub mod retry;
pub mod ffi;

// Re-exports for convenience
pub use error::{GeminiAudioError, Result};
pub use config::{RetryConfig, AudioConfig};
pub use audio::{decode_to_pcm_16k, read_wav_pcm, write_wav_pcm, AudioFormat, AudioInfo};
pub use database::{Database, Session, SessionStatus};
pub use client::{GeminiClient, GeminiSender, GeminiReceiver, ServerResponse, GoAway, SessionResumptionUpdate};
pub use logging::init_logging;
pub use prompts::PromptManager;
pub use retry::RetryManager;

