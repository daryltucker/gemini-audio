// Custom error types for Gemini Audio

use thiserror::Error;

#[derive(Error, Debug)]
pub enum GeminiAudioError {
    #[error("Audio conversion error: {0}")]
    AudioConversion(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("File I/O error: {0}")]
    FileIO(String),

    #[error("API error: {0}")]
    API(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Audio device error: {0}")]
    AudioDevice(String),

    #[error("Minio storage error: {0}")]
    MinioStorage(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Processing error: {0}")]
    Processing(String),
}

pub type Result<T> = std::result::Result<T, GeminiAudioError>;
