// Configuration constants and settings for Gemini Audio

/// WebSocket endpoint for Gemini Live API
pub const WEBSOCKET_ENDPOINT: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

/// Model ID for Gemini Live API
pub const MODEL_ID: &str = "models/gemini-2.5-flash-native-audio-preview-12-2025";

/// Audio input specifications
pub const INPUT_SAMPLE_RATE: u32 = 16000;
pub const OUTPUT_SAMPLE_RATE: u32 = 24000;
pub const AUDIO_CHANNELS: u16 = 1;
pub const AUDIO_BIT_DEPTH: u16 = 16;

/// Retry configuration
pub struct RetryConfig {
    pub max_retries_5xx: usize,
    pub retry_429: bool,
    pub retry_401: bool,
    pub retry_delay_ms: u64,
    pub backoff_factor: f64,
    pub max_backoff_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries_5xx: 3,
            retry_429: true,
            retry_401: false,
            retry_delay_ms: 0,
            backoff_factor: 2.0,
            max_backoff_ms: 10000,
        }
    }
}

/// Audio chunking configuration
pub struct AudioConfig {
    pub chunk_size_ms: u64,
    pub buffer_size_ms: u64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            chunk_size_ms: 3000, // 3 seconds for single file mode
            buffer_size_ms: 500,  // 500ms buffer for playback
        }
    }
}

/// Default bucket name for Minio storage
pub const DEFAULT_MINIO_BUCKET: &str = "gemini-audio-results";

/// Default database name
pub const DEFAULT_DATABASE_NAME: &str = "gemini-audio.db";

/// All available Gemini Live API voices
pub const VOICES: &[&str] = &[
    "Fenrir",
    "Puck",
    "Charon",
    "Kore",
    "Aoede",
    "Leda",
    "Orus",
    "Zephyr",
    "Laomedeia",
    "Enceladus",
    "Iapetus",
    "Umbriel",
    "Achernar",
    "Rasalgethi",
    "Algieba",
    "Despina",
    "Erinome",
    "Autonoe",
    "Callirrhoe",
    "Achird",
    "Zubenelgenubi",
    "Vindemiatrix",
    "Sadachbia",
    "Sulafat",
    "Gacrux",
    "Pulcherrima",
    "Algenib",
    "Alnilam",
    "Sadaltager",
    "Schedar",
];
