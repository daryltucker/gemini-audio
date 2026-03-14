/// FFI interface for Android (and future platforms).
///
/// This module exposes a minimal, FFI-safe surface using UniFFI proc-macros.
/// All types here are simple records/enums; Kotlin calls these functions via JNA.
///
/// The async Tokio runtime is lazily initialized inside the Rust library. Kotlin
/// doesn't need to know about Rust async — every exported function blocks until
/// the async work completes, then returns the result.

use std::sync::OnceLock;

// ── Runtime ──────────────────────────────────────────────────────────────────

/// Global Tokio runtime for async operations called from FFI.
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn rt() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

// ── Exported types ───────────────────────────────────────────────────────────

/// A conversation summary for the list screen.
#[derive(uniffi::Record)]
pub struct ConversationSummary {
    pub id: u64,
    pub timestamp: String,
    pub preview: String,
    pub turn_count: u32,
}

/// A single turn in a conversation.
#[derive(uniffi::Record)]
pub struct ConversationTurn {
    pub turn: u64,
    pub timestamp: String,
    pub voice: String,
    pub user_text: String,
    pub assistant_text: String,
    pub thinking: String,
    pub has_recording: bool,
}

/// Available prompt info.
#[derive(uniffi::Record)]
pub struct PromptInfo {
    pub name: String,
}

// ── Prompt functions ─────────────────────────────────────────────────────────

/// List available prompts (both bundled and user-created).
#[uniffi::export]
pub fn list_prompts(user_dir: String, bundled_dir: String) -> Vec<PromptInfo> {
    let pm = match crate::prompts::PromptManager::new(
        std::path::PathBuf::from(&user_dir),
        std::path::PathBuf::from(&bundled_dir),
    ) {
        Ok(pm) => pm,
        Err(_) => return Vec::new(),
    };

    match pm.list_prompts() {
        Ok(list) => list
            .into_iter()
            .map(|name| PromptInfo { name })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Load a prompt by name. Returns the markdown content, or empty string on error.
#[uniffi::export]
pub fn load_prompt(user_dir: String, bundled_dir: String, name: String) -> String {
    let pm = match crate::prompts::PromptManager::new(
        std::path::PathBuf::from(&user_dir),
        std::path::PathBuf::from(&bundled_dir),
    ) {
        Ok(pm) => pm,
        Err(_) => return String::new(),
    };

    pm.load_prompt(&name).unwrap_or_default()
}

// ── Conversation log functions ───────────────────────────────────────────────

/// List conversations from the JSONL files in the conversations directory.
/// Returns newest-first.
#[uniffi::export]
pub fn list_conversations(data_dir: String) -> Vec<ConversationSummary> {
    let conv_dir = std::path::PathBuf::from(&data_dir).join("conversations");
    let Ok(entries) = std::fs::read_dir(&conv_dir) else {
        return Vec::new();
    };

    let mut summaries: Vec<ConversationSummary> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let id: u64 = filename.parse().unwrap_or(0);
        if id == 0 { continue; }

        // Read the JSONL and extract summary info
        let Ok(content) = std::fs::read_to_string(&path) else { continue };
        let lines: Vec<&str> = content.lines().collect();
        let turn_count = lines.len() as u32;

        // First turn timestamp, last turn for preview
        let mut timestamp = String::new();
        let mut preview = String::new();

        if let Some(first) = lines.first() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(first) {
                timestamp = v["timestamp"].as_str().unwrap_or("").to_string();
            }
        }
        if let Some(last) = lines.last() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(last) {
                let asst = v["assistantText"].as_str().unwrap_or("");
                preview = if asst.len() > 80 {
                    format!("{}…", &asst[..80])
                } else {
                    asst.to_string()
                };
            }
        }
        summaries.push(ConversationSummary {
            id,
            timestamp,
            preview,
            turn_count,
        });
    }

    // Sort newest-first
    summaries.sort_by(|a, b| b.id.cmp(&a.id));
    summaries
}

/// Load all turns for a specific conversation.
#[uniffi::export]
pub fn load_conversation(data_dir: String, conversation_id: u64) -> Vec<ConversationTurn> {
    let path = std::path::PathBuf::from(&data_dir)
        .join("conversations")
        .join(format!("{}.jsonl", conversation_id));

    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    content
        .lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            Some(ConversationTurn {
                turn: v["turn"].as_u64().unwrap_or(0),
                timestamp: v["timestamp"].as_str().unwrap_or("").to_string(),
                voice: v["voice"].as_str().unwrap_or("").to_string(),
                user_text: v["userText"].as_str().unwrap_or("").to_string(),
                assistant_text: v["assistantText"].as_str().unwrap_or("").to_string(),
                thinking: v["thinking"].as_str().unwrap_or("").to_string(),
                has_recording: false, // No recordings in JSONL format
            })
        })
        .collect()
}


/// Create a new conversation file.
#[uniffi::export]
pub fn create_conversation(data_dir: String, conversation_id: u64) -> bool {
    let base = std::path::PathBuf::from(&data_dir);
    let conv_dir = base.join("conversations");
    
    if let Err(e) = std::fs::create_dir_all(&conv_dir) {
        eprintln!("Failed to create conversations directory: {}", e);
        return false;
    }
    
    let jsonl = conv_dir.join(format!("{}.jsonl", conversation_id));
    
    match std::fs::write(&jsonl, "") {
        Ok(_) => true,
        Err(e) => {
            eprintln!("Failed to create conversation file: {}", e);
            false
        }
    }
}

/// Delete a conversation and its recordings.
#[uniffi::export]
pub fn delete_conversation(data_dir: String, conversation_id: u64) -> bool {
    let base = std::path::PathBuf::from(&data_dir);

    // Delete JSONL
    let jsonl = base.join("conversations").join(format!("{}.jsonl", conversation_id));
    let _ = std::fs::remove_file(&jsonl);

    // Delete associated recordings
    let rec_dir = base.join("recordings");
    if let Ok(entries) = std::fs::read_dir(&rec_dir) {
        let prefix = format!("conv_{}_", conversation_id);
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&prefix) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    true
}

/// Add a turn to a conversation.
#[uniffi::export]
pub fn add_conversation_turn(
    data_dir: String,
    conversation_id: u64,
    turn: u64,
    voice: String,
    user_text: String,
    assistant_text: String,
    thinking: String,
) -> bool {
    use std::fs::{OpenOptions, create_dir_all};
    use std::io::Write;
    
    let base = std::path::PathBuf::from(&data_dir);
    let conv_dir = base.join("conversations");
    
    if let Err(e) = create_dir_all(&conv_dir) {
        eprintln!("Failed to create conversations directory: {}", e);
        return false;
    }
    
    let jsonl = conv_dir.join(format!("{}.jsonl", conversation_id));
    let timestamp = chrono::Utc::now().to_rfc3339();
    
    let entry = serde_json::json!({
        "turn": turn,
        "timestamp": timestamp,
        "voice": voice,
        "userText": user_text,
        "assistantText": assistant_text,
        "thinking": thinking,  // Always recorded (even if showThinking is disabled)
    });
    
    let mut file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&jsonl)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open jsonl: {}", e);
            return false;
        }
    };
    
    match writeln!(file, "{}", entry) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("Failed to write turn: {}", e);
            false
        }
    }
}

// ── Configuration helpers ────────────────────────────────────────────────────

/// Get available voice names.
#[uniffi::export]
pub fn available_voices() -> Vec<String> {
    crate::config::VOICES.iter().map(|v| v.to_string()).collect()
}

/// Initialize logging for Android (file-only, no console).
#[uniffi::export]
pub fn init_android_logging(data_dir: String) {
    let data_path = std::path::PathBuf::from(data_dir);
    let _ = crate::logging::init_logging("DEBUG", false, false, &data_path);
}

/// Verify that an API key is valid by attempting a minimal connection.
/// Returns empty string on success, or error message on failure.
#[uniffi::export]
pub fn verify_api_key(api_key: String) -> String {
    rt().block_on(async {
        // Temporarily set the key for the connection attempt
        std::env::set_var("GEMINI_API_KEY", &api_key);
        match crate::client::GeminiClient::connect().await {
            Ok(mut client) => {
                let _ = client.close().await;
                String::new() // success
            }
            Err(e) => format!("{}", e),
        }
    })
}

// ── Live Session Support ─────────────────────────────────────────────────────

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
// use futures_util::StreamExt; // Removed unused import
use crate::client::GeminiClient;
use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction, Resampler};

/// Callback trait for session events.
/// Implemented in Kotlin to receive audio, transcripts, and errors.
#[uniffi::export(with_foreign)]
pub trait SessionCallback: Send + Sync {
    /// Called when a chunk of audio is received from Gemini (24kHz PCM).
    /// The Kotlin side should resample to 16kHz and play via Oboe.
    fn on_audio_chunk(&self, chunk: Vec<u8>);
    
    /// Called when user speech is transcribed.
    fn on_user_transcript(&self, text: String);
    
    /// Called when assistant speech is transcribed.
    fn on_assistant_transcript(&self, text: String);
    
    /// Called when Gemini is thinking (internal thought tokens, not visible response).
    fn on_thinking(&self, text: String);
    
    /// Called when an error occurs.
    fn on_error(&self, message: String);
    
    /// Called when the session ends (or is closed).
    fn on_session_end(&self);
    
    /// Called when a new session resumption handle is received.
    fn on_session_handle(&self, handle: String);
}

/// A live session with Gemini.
#[derive(uniffi::Object)]
pub struct Session {
    stop_tx: mpsc::Sender<()>,
    turn_end_tx: mpsc::Sender<()>,
    audio_tx: mpsc::UnboundedSender<Vec<u8>>,
    start_tx: Mutex<Option<oneshot::Sender<(String, String)>>>,
}

#[uniffi::export]
impl Session {
    /// Create a new session.
    /// The session will not start until `start()` is called.
    #[uniffi::constructor]
        pub fn new(callback: Arc<dyn SessionCallback>) -> Self {
        let (audio_tx, audio_rx) = mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = mpsc::channel(1);
        let (turn_end_tx, turn_end_rx) = mpsc::channel(1);
        let (start_tx, start_rx) = oneshot::channel();
        
        let callback_clone = callback.clone();
        
        // Spawn the session task with its own Tokio runtime
        std::thread::spawn(move || {
            // Create a new Tokio runtime for this thread
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");
            
            rt.block_on(async move {
                run_session_task(callback_clone, audio_rx, stop_rx, turn_end_rx, start_rx).await;
            });
        });
        
        Self {
            audio_tx,
            stop_tx,
            turn_end_tx,
            start_tx: Mutex::new(Some(start_tx)),
        }
    }
    
    /// Start the session with the given prompt and voice.
    /// This is non-blocking; connection happens in the background.
    /// Returns true if started successfully, false otherwise.
    pub fn start(&self, prompt: String, voice: String) -> bool {
            if let Some(sender) = self.start_tx.lock().unwrap().take() {
                let _ = sender.send((prompt, voice));
                true
            } else {
                false
            }
    }
    
    /// Send an audio chunk to Gemini.
    /// Audio should be 16kHz, 16-bit signed, mono, little-endian PCM.
    pub fn send_audio(&self, chunk: Vec<u8>) {
        let _ = self.audio_tx.send(chunk);
    }
    
    /// Stop the session and close the connection.
    /// This is non-blocking; the session task will terminate asynchronously.
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
    }

    /// End the current turn (activity).
    /// This sends activityEnd to the server.
    pub fn end_turn(&self) {
        let _ = self.turn_end_tx.try_send(());
    }
}

async fn run_session_task(
    callback: Arc<dyn SessionCallback>,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut stop_rx: mpsc::Receiver<()>,
    mut turn_end_rx: mpsc::Receiver<()>,
    start_rx: oneshot::Receiver<(String, String)>,
) {
    let mut is_recording = false;
    
    // Wait for start signal
    let (prompt, voice) = match start_rx.await {
        Ok(val) => val,
        Err(_) => {
            callback.on_error("Session start failed".to_string());
            return;
        }
    };
    
    // Connect to Gemini
    let mut client = match GeminiClient::connect().await {
        Ok(c) => c,
        Err(e) => {
            callback.on_error(format!("Connection failed: {}", e));
            return;
        }
    };
    
    // Send setup
    if let Err(e) = client.send_setup_persistent(Some(prompt), Some(voice), None).await {
        callback.on_error(format!("Setup failed: {}", e));
        let _ = client.close().await;
        return;
    }
    
    // Wait for setup complete
    loop {
        match client.receive_response().await {
            Ok(resp) => {
                if resp.setup_complete.is_some() {
                    break;
                }
                if let Some(err) = resp.error {
                    callback.on_error(format!("Server error: {} - {}", err.code, err.message));
                    let _ = client.close().await;
                    return;
                }
            }
            Err(e) => {
                callback.on_error(format!("Receive error: {}", e));
                let _ = client.close().await;
                return;
            }
        }
    }
    
    // Main loop
    let mut audio_buffer: Vec<u8> = Vec::new();
    
    loop {
        tokio::select! {
            biased;
            
            // Stop signal
            _ = stop_rx.recv() => {
                if is_recording {
                    if let Err(e) = client.send_activity_end().await {
                        callback.on_error(format!("Send activityEnd failed: {}", e));
                    }
                }
                break;
            }
            
            // Turn end signal
            _ = turn_end_rx.recv() => {
                if is_recording {
                    if let Err(e) = client.send_activity_end().await {
                        callback.on_error(format!("Send activityEnd failed: {}", e));
                    }
                    is_recording = false;
                }
            }
            
            // Audio chunk from Kotlin
            Some(chunk) = audio_rx.recv() => {
                // Send activityStart on first chunk
                if !is_recording {
                    if let Err(e) = client.send_activity_start().await {
                        callback.on_error(format!("Send activityStart failed: {}", e));
                        break;
                    }
                    is_recording = true;
                }
                
                // Accumulate audio and send in batches
                audio_buffer.extend_from_slice(&chunk);
                
                // Send in ~100ms chunks (3200 bytes for 16kHz 16-bit mono)
                // Or send immediately for lower latency
                // For now, send immediately
                if let Err(e) = client.send_audio(&chunk).await {
                    callback.on_error(format!("Send audio failed: {}", e));
                    break;
                }
            }
            
            // Server response
            resp_result = client.receive_response() => {
                match resp_result {
                    Ok(resp) => {
                        // Handle audio — pass 24kHz PCM directly, no resampling needed
                        if let Ok(Some(audio)) = crate::client::GeminiClient::extract_audio_data(&resp) {
                            callback.on_audio_chunk(audio);
                        }
                        
                        // Handle transcripts
                        if let Some(server_content) = &resp.server_content {
                            // User's transcription (what they said)
                            if let Some(transcription) = &server_content.input_transcription {
                                callback.on_user_transcript(transcription.text.clone());
                            }
                            
                            // Gemini's voice transcription (what they're saying audibly) - THIS IS WHAT WAS MISSING!
                            if let Some(transcription) = &server_content.output_transcription {
                                callback.on_assistant_transcript(transcription.text.clone());
                            }
                            
                            // Also handle model_turn.parts for text response
                            if let Some(model_turn) = &server_content.model_turn {
                                for part in &model_turn.parts {
                                    if let Some(text) = &part.text {
                                        if part.thought {
                                            // Internal thinking - not visible to user
                                            callback.on_thinking(text.clone());
                                        }
                                        // Note: we don't send model_turn text to on_assistant_transcript 
                                        // because output_transcription already handles the voice
                                    }
                                }
                            }
                            
                            // Handle session resumption
                            // Note: session_resumption_update is a field of resp, not server_content
                            // but it's accessible here because resp is in scope
                            if let Some(update) = &resp.session_resumption_update {
                                if let Some(handle) = &update.new_handle {
                                    callback.on_session_handle(handle.clone());
                                }
                            }
                            
                            // Handle turn complete
                            if server_content.turn_complete.unwrap_or(false) {
                                // Reset for next turn
                                audio_buffer.clear();
                                // Signal that the turn is complete - UI can return to idle
                                callback.on_session_end();
                            }
                        }
                    }
                    Err(e) => {
                        if is_recording {
                            let _ = client.send_activity_end().await;
                        }
                        callback.on_error(format!("Receive error: {}", e));
                        break;
                    }
                }
            }
        }
    }
    
    let _ = client.close().await;
    callback.on_session_end();
}
