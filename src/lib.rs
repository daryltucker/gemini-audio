// Gemini Audio Library
// A Rust client for Gemini 2.5 Flash Native Audio with support for both
// single file processing and real-time streaming

pub mod config;
pub mod error;
pub mod audio;
pub mod capabilities;
pub mod database;
pub mod client;
pub mod logging;
pub mod prompts;
pub mod retry;
pub mod tui;

// Re-exports for convenience
pub use error::{GeminiAudioError, Result};
pub use config::{RetryConfig, AudioConfig};
pub use audio::{decode_to_pcm_16k, play_pcm_pulseaudio, play_pcm_pulseaudio_cancellable, read_wav_pcm, write_wav_pcm, AudioFormat, AudioInfo};
pub use database::{Database, Session, SessionStatus};
pub use client::{GeminiClient, GeminiSender, GeminiReceiver, ServerResponse, GoAway, SessionResumptionUpdate};
pub use logging::init_logging;
pub use prompts::PromptManager;
pub use retry::RetryManager;
