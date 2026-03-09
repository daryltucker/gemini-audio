// Terminal User Interface (TUI) for Gemini Audio
// Built using Ratatui and Crossterm

use crate::error::{GeminiAudioError, Result};
use crate::client::GeminiClient;
use crate::audio;
use crate::config;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use futures_util::StreamExt as FuturesStreamExt;
use std::io::{self, Stdout};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use tokio::sync::mpsc;
use chrono::Utc;

// ── State types ───────────────────────────────────────────────────────────────

/// Possible states for the TUI application
#[derive(PartialEq)]
pub enum AppState {
    Idle,              // Waiting for user input
    Recording,         // Capturing audio from microphone
    Processing,        // Gemini is generating a response (network + decoding)
    Playing,           // Gemini's audio response is playing; Space will barge in
    Reconnecting,      // goAway received; reconnecting with saved session handle
    Error(String),     // Displaying an error (Space to dismiss)
}

/// Commands sent from the UI thread to the persistent session background task
pub enum TaskControl {
    /// Mic opened: session sends activityStart and enters streaming mode.
    BeginUtterance,
    /// One 100ms chunk of live mic PCM (16kHz/16-bit/mono). Sent by the recorder thread.
    AudioChunk(Vec<u8>),
    /// Mic closed: session sends activityEnd and awaits Gemini's response.
    EndUtterance,
    /// Shut down the session task (app is closing)
    Quit,
}

/// Updates sent from the background session task to the UI thread
pub enum AppUpdate {
    UserTranscript(String),        // Live STT transcription of what the user said
    AssistantTranscript(String),   // Gemini TEXT modality response
    Thinking(String),              // Internal thinking tokens
    VoiceText(String),             // Transcription of Gemini's spoken audio (outputAudioTranscription)
    Done,                          // Turn is complete, UI may return to Idle
    Error(String),                 // Fatal error in the session task
    Warning(String),               // Non-fatal notice appended to message history
    SessionHandle(String),         // Server sent a new resumption handle (save this)
    Reconnecting,                  // goAway received; transparent reconnect in progress
    PlaybackStarted(Arc<AtomicBool>), // Stop flag for current playback (set true to abort)
    PlaybackFinished,                  // Playback thread has completed; return to Idle
    /// Resolved after capability probe: true = Audio+Text, false = Audio-only
    ModeDetected(bool),
}

// ── ChatMessage ───────────────────────────────────────────────────────────────

/// Structured conversation history entry. Replaces flat Vec<String> rendering.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    /// Visual separator between turns (rendered as a dashed rule).
    Divider,
    /// System/status line (welcome, reconnecting, cancelled, copied, etc.)
    System(String),
    /// Non-fatal warning from the session task.
    Warning(String),
    /// Fatal or recoverable error message.
    ErrorMsg(String),
    /// One complete or in-progress conversation turn.
    Turn {
        /// User speech transcript (inputAudioTranscription).
        user: String,
        /// Internal model reasoning tokens. Hidden unless show_thinking is true.
        thinking: String,
        /// Transcription of Gemini's spoken audio (outputAudioTranscription / VoiceText).
        speech: String,
        /// TEXT modality response body (AssistantTranscript). Empty on audio-only accounts.
        text: String,
    },
}

impl ChatMessage {
    /// Returns the text to copy to clipboard, or None for non-copyable entries.
    pub fn copyable_text(&self) -> Option<String> {
        match self {
            ChatMessage::Turn { user, thinking, speech, text } => {
                let mut parts = Vec::new();
                if !user.is_empty()     { parts.push(format!("[You]: {}", user)); }
                if !speech.is_empty()   { parts.push(format!("[Speech]: {}", speech)); }
                if !text.is_empty()     { parts.push(format!("[Text]: {}", text)); }
                if !thinking.is_empty() { parts.push(format!("[Thinking]: {}", thinking)); }
                if parts.is_empty() { None } else { Some(parts.join("\n")) }
            }
            ChatMessage::System(s) | ChatMessage::Warning(s) | ChatMessage::ErrorMsg(s) => {
                if s.is_empty() { None } else { Some(s.clone()) }
            }
            ChatMessage::Divider => None,
        }
    }
}

// ── LineTag ───────────────────────────────────────────────────────────────────

/// Identifies which section of a message a rendered line belongs to.
/// Used to copy only the clicked section rather than the whole turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineTag {
    /// Not clickable (divider, [Gemini] section header).
    None,
    /// Whole message body — for System / Warning / ErrorMsg.
    WholeMsg,
    /// Turn.user
    User,
    /// Turn.speech (VoiceText / outputAudioTranscription)
    Speech,
    /// Turn.text (TEXT modality)
    Text,
    /// Turn.thinking
    Thinking,
}

// ── App struct ────────────────────────────────────────────────────────────────

/// Application state for the TUI
pub struct App {
    pub running: bool,
    /// Index into config::VOICES for the selected voice
    pub voice_idx: usize,
    /// System prompt sent to Gemini
    pub prompt: String,
    /// Finalized chat history
    pub messages: Vec<ChatMessage>,
    /// Live user transcript (streaming, committed to messages on Done)
    pub user_buffer: String,
    /// Internal thinking buffer (committed on Done)
    pub thinking_buffer: String,
    /// Live VoiceText / outputAudioTranscription (committed on Done)
    pub speech_buffer: String,
    /// Live TEXT modality response (committed on Done)
    pub text_buffer: String,
    /// Whether to render thinking blocks (toggle with 't')
    pub show_thinking: bool,
    /// Current state machine state
    pub state: AppState,
    /// Active mic recorder (Some while Recording)
    pub record_process: Option<crate::audio::Recorder>,
    /// Debounce timestamp
    pub last_action_time: chrono::DateTime<Utc>,
    /// When Some, the persistent session task is running. Voice is locked while this is Some.
    pub session_tx: Option<mpsc::UnboundedSender<TaskControl>>,
    /// Monotonic ID for this conversation (timestamp at launch), shown in header
    pub conversation_id: u64,
    /// Last session resumption handle from the server (last 8 chars shown in header)
    pub session_handle: Option<String>,
    /// In one-shot mode each press spawns a fresh connection (matches --input file behavior)
    pub is_one_shot: bool,
    /// Stop flag for the currently playing audio. Set to true for barge-in.
    pub playback_stop: Option<Arc<AtomicBool>>,
    /// Turn counter within this conversation (for per-turn log file naming)
    pub turn_count: u64,
    /// Resolved response modality: None = unknown, Some(true) = Audio+Text, Some(false) = Audio-only
    pub supports_text: Option<bool>,
    /// Rendered line index → (message index, section tag). None index = live turn.
    pub line_to_msg: Vec<(Option<usize>, LineTag)>,
    /// Y coordinate of the chat widget's top border (set each render, used for click mapping)
    pub chat_area_top: u16,
    /// Current scroll offset of the chat widget (set each render, used for click mapping)
    pub chat_scroll: u16,
}

impl App {
    pub fn new(prompt: String, is_one_shot: bool) -> Self {
        let default_voice = std::env::var("GEMINI_AUDIO_VOICE").unwrap_or_else(|_| "Fenrir".to_string());
        let voice_idx = crate::config::VOICES
            .iter()
            .position(|&v| v.eq_ignore_ascii_case(&default_voice))
            .unwrap_or(0);

        Self {
            running: true,
            voice_idx,
            prompt,
            messages: vec![ChatMessage::System("Welcome to Gemini Audio Live!".to_string())],
            user_buffer: String::new(),
            thinking_buffer: String::new(),
            speech_buffer: String::new(),
            text_buffer: String::new(),
            show_thinking: false,
            state: AppState::Idle,
            record_process: None,
            last_action_time: Utc::now(),
            session_tx: None,
            conversation_id: Utc::now().timestamp() as u64,
            session_handle: None,
            is_one_shot,
            playback_stop: None,
            turn_count: 0,
            supports_text: None,
            line_to_msg: Vec::<(Option<usize>, LineTag)>::new(),
            chat_area_top: 0,
            chat_scroll: 0,
        }
    }

    pub fn voice(&self) -> &'static str {
        crate::config::VOICES[self.voice_idx]
    }

    pub fn next_voice(&mut self) {
        self.voice_idx = (self.voice_idx + 1) % crate::config::VOICES.len();
    }

    pub fn prev_voice(&mut self) {
        self.voice_idx = self.voice_idx
            .checked_sub(1)
            .unwrap_or(crate::config::VOICES.len() - 1);
    }

    /// True when voice selection is available (no active session)
    pub fn voice_configurable(&self) -> bool {
        self.session_tx.is_none() && !matches!(self.state, AppState::Playing)
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Initialize and run the TUI application
pub async fn run_tui(prompt: String, is_one_shot: bool) -> Result<()> {
    // Set a custom panic hook to restore the terminal before panicking
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Best-effort terminal restoral if the main thread panics
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = execute!(io::stdout(), crossterm::cursor::Show);
        original_hook(panic_info);
    }));

    enable_raw_mode()
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to enable raw mode: {}", e)))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to setup terminal: {}", e)))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to create terminal: {}", e)))?;

    let mut app = App::new(prompt, is_one_shot);
    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to disable raw mode: {}", e)))?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(|e| GeminiAudioError::Configuration(format!("Failed to restore terminal: {}", e)))?;
    terminal
        .show_cursor()
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to show cursor: {}", e)))?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

// ── Event loop ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct SttTracker {
    history: String,
    active: String,
}

impl SttTracker {
    fn new() -> Self {
        Self { history: String::new(), active: String::new() }
    }
    
    fn update(&mut self, new_text: &str) -> String {
        if new_text.is_empty() { return self.full_text(); }
        
        if self.active.is_empty() {
            self.active = new_text.to_string();
            return self.full_text();
        }
        
        let active_first_word = self.active.split_whitespace().next().unwrap_or("").to_lowercase();
        let new_first_word = new_text.split_whitespace().next().unwrap_or("").to_lowercase();
        
        let is_same_burst = 
            (!active_first_word.is_empty() && new_first_word.starts_with(&active_first_word)) ||
            (!new_first_word.is_empty() && active_first_word.starts_with(&new_first_word));
            
        if is_same_burst {
            self.active = new_text.to_string();
        } else {
            if !self.history.is_empty() && !self.history.ends_with(' ') && !self.active.starts_with(' ') {
                self.history.push(' ');
            }
            self.history.push_str(&self.active);
            self.active = new_text.to_string();
        }
        
        self.full_text()
    }
    
    fn full_text(&self) -> String {
        if self.history.is_empty() {
            self.active.clone()
        } else if self.active.is_empty() {
            self.history.clone()
        } else {
            let space = if self.history.ends_with(' ') || self.active.starts_with(' ') { "" } else { " " };
            format!("{}{}{}", self.history, space, self.active)
        }
    }
    
    fn clear(&mut self) {
        self.history.clear();
        self.active.clear();
    }
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<AppUpdate>();
    let mut event_reader = crossterm::event::EventStream::new();

    while app.running {
        terminal.draw(|f| ui(f, app))?;

        tokio::select! {
            biased;

            // ── Update channel — prioritized so streaming text renders immediately ──
            Some(update) = rx.recv() => {
                apply_update(update, app);
                while let Ok(u) = rx.try_recv() {
                    apply_update(u, app);
                }
            }

            // ── Input events ───────────────────────────────────────────────────────
            maybe_event = event_reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }
                        match key.code {
                            // Global Exit
                            KeyCode::Char('q') | KeyCode::Esc => {
                                if let Some(mut recorder) = app.record_process.take() {
                                    tokio::task::block_in_place(|| recorder.stop());
                                }
                                if let Some(ref flag) = app.playback_stop {
                                    flag.store(true, Ordering::Relaxed);
                                }
                                if let Some(ref stx) = app.session_tx {
                                    let _ = stx.send(TaskControl::Quit);
                                }
                                app.running = false;
                            }

                            // Tab / Shift-Tab cycles voices — only when session is not locked
                            KeyCode::Tab => {
                                if app.voice_configurable() {
                                    app.next_voice();
                                }
                            }
                            KeyCode::BackTab => {
                                if app.voice_configurable() {
                                    app.prev_voice();
                                }
                            }

                            // [t] — toggle thinking block visibility
                            KeyCode::Char('t') => {
                                app.show_thinking = !app.show_thinking;
                            }

                            // Space — main action key with debounce
                            KeyCode::Char(' ') => {
                                let now = Utc::now();
                                if now
                                    .signed_duration_since(app.last_action_time)
                                    .num_milliseconds()
                                    < 300
                                {
                                    continue;
                                }
                                app.last_action_time = now;

                                // Dismiss error and return to Idle
                                if matches!(app.state, AppState::Error(_)) {
                                    app.state = AppState::Idle;
                                    continue;
                                }

                                match app.state {
                                    // ── Start recording ───────────────────────────────────────────
                                    AppState::Idle | AppState::Reconnecting => {
                                        if let Some(ref flag) = app.playback_stop {
                                            flag.store(true, Ordering::Relaxed);
                                        }
                                        app.playback_stop = None;
                                        app.messages.push(ChatMessage::Divider);

                                        let session_tx = if let Some(ref existing) = app.session_tx {
                                            existing.clone()
                                        } else {
                                            let (stx, srx) = mpsc::unbounded_channel::<TaskControl>();
                                            let tx2 = tx.clone();
                                            let prompt = app.prompt.clone();
                                            let voice = app.voice().to_string();
                                            let is_one_shot = app.is_one_shot;
                                            let conv_id = app.conversation_id;
                                            tokio::spawn(run_persistent_session(
                                                prompt,
                                                voice,
                                                srx,
                                                tx2,
                                                is_one_shot,
                                                conv_id,
                                            ));
                                            app.session_tx = Some(stx.clone());
                                            stx
                                        };

                                        let _ = session_tx.send(TaskControl::BeginUtterance);

                                        let stx_for_recorder = session_tx.clone();
                                        match audio::start_recording_streaming(move |chunk| {
                                            let _ = stx_for_recorder.send(TaskControl::AudioChunk(chunk));
                                        }) {
                                            Ok(recorder) => {
                                                app.record_process = Some(recorder);
                                                app.state = AppState::Recording;
                                            }
                                            Err(e) => {
                                                app.state = AppState::Error(format!(
                                                    "Failed to start recording: {:?}", e
                                                ));
                                            }
                                        }
                                    }

                                    // ── Stop recording ────────────────────────────────────────────
                                    AppState::Recording => {
                                        if let Some(mut recorder) = app.record_process.take() {
                                            tokio::task::block_in_place(|| recorder.stop());
                                        }
                                        app.state = AppState::Processing;
                                        app.turn_count += 1;

                                        if let Some(ref stx) = app.session_tx {
                                            let _ = stx.send(TaskControl::EndUtterance);
                                        }
                                    }

                                    // ── Barge-in: stop playback and start a new utterance ─────────
                                    AppState::Processing | AppState::Playing => {
                                        if let Some(ref flag) = app.playback_stop {
                                            flag.store(true, Ordering::Relaxed);
                                        }
                                        app.playback_stop = None;

                                        // Commit the current interrupted turn locally to the UI before starting the new one
                                        let user     = std::mem::take(&mut app.user_buffer).trim().to_string();
                                        let thinking = std::mem::take(&mut app.thinking_buffer).trim().to_string();
                                        let speech   = std::mem::take(&mut app.speech_buffer).trim().to_string();
                                        let text     = std::mem::take(&mut app.text_buffer).trim().to_string();

                                        if !user.is_empty() || !thinking.is_empty() || !speech.is_empty() || !text.is_empty() {
                                            app.messages.push(ChatMessage::Turn { user, thinking, speech, text });
                                        }

                                        if let Some(ref stx) = app.session_tx {
                                            let _ = stx.send(TaskControl::BeginUtterance);
                                        }

                                        let stx_for_recorder = app.session_tx.clone();
                                        match audio::start_recording_streaming(move |chunk| {
                                            if let Some(ref stx) = stx_for_recorder {
                                                let _ = stx.send(TaskControl::AudioChunk(chunk));
                                            }
                                        }) {
                                            Ok(recorder) => {
                                                app.record_process = Some(recorder);
                                                app.state = AppState::Recording;
                                            }
                                            Err(e) => {
                                                app.state = AppState::Error(format!(
                                                    "Failed to start recording: {:?}", e
                                                ));
                                            }
                                        }
                                    }

                                    _ => {}
                                }
                            }

                            // [c] — Cancel recording
                            KeyCode::Char('c') => {
                                if app.state == AppState::Recording {
                                    if let Some(mut recorder) = app.record_process.take() {
                                        tokio::task::block_in_place(|| recorder.stop());
                                    }
                                    if let Some(ref stx) = app.session_tx {
                                        let _ = stx.send(TaskControl::EndUtterance);
                                    }
                                    app.state = AppState::Processing;
                                    app.messages.push(ChatMessage::System("(cancelled)".to_string()));
                                }
                            }

                            _ => {}
                        }
                    }

                    // ── Mouse — left-click copies the clicked message ──────────────
                    Some(Ok(Event::Mouse(mouse))) => {
                        handle_mouse(mouse, app);
                    }

                    _ => {}
                }
            }

            // ── Periodic re-render (spinner, cursor blink, etc.) ──────────────────
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
    Ok(())
}

// ── Mouse click handler ───────────────────────────────────────────────────────

fn handle_mouse(mouse: MouseEvent, app: &mut App) {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    let border = 1u16;
    if mouse.row <= app.chat_area_top + border {
        return;
    }
    let logical = (mouse.row.saturating_sub(app.chat_area_top + border) as usize)
        .saturating_add(app.chat_scroll as usize);

    let Some(&(maybe_idx, tag)) = app.line_to_msg.get(logical) else { return };
    if tag == LineTag::None { return; }

    // Extract just the clicked section's text.
    let text: Option<String> = match tag {
        LineTag::None => return,

        LineTag::WholeMsg => match maybe_idx {
            Some(i) => app.messages.get(i).and_then(|m| m.copyable_text()),
            None    => None,
        },

        LineTag::User => match maybe_idx {
            Some(i) => app.messages.get(i).and_then(|m| {
                if let ChatMessage::Turn { user, .. } = m {
                    if user.is_empty() { None } else { Some(user.clone()) }
                } else { None }
            }),
            None => nonempty(app.user_buffer.trim()),
        },

        LineTag::Speech => match maybe_idx {
            Some(i) => app.messages.get(i).and_then(|m| {
                if let ChatMessage::Turn { speech, .. } = m {
                    if speech.is_empty() { None } else { Some(speech.clone()) }
                } else { None }
            }),
            None => nonempty(app.speech_buffer.trim()),
        },

        LineTag::Text => match maybe_idx {
            Some(i) => app.messages.get(i).and_then(|m| {
                if let ChatMessage::Turn { text, .. } = m {
                    if text.is_empty() { None } else { Some(text.clone()) }
                } else { None }
            }),
            None => nonempty(app.text_buffer.trim()),
        },

        LineTag::Thinking => match maybe_idx {
            Some(i) => app.messages.get(i).and_then(|m| {
                if let ChatMessage::Turn { thinking, .. } = m {
                    if thinking.is_empty() { None } else { Some(thinking.clone()) }
                } else { None }
            }),
            None => nonempty(app.thinking_buffer.trim()),
        },
    };

    if let Some(text) = text {
        copy_to_clipboard(text);
    }
}

fn nonempty(s: &str) -> Option<String> {
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// Copy text to the system clipboard.
///
/// On Linux/X11 the clipboard is owned by the application — data is only available
/// while we're actively serving selection requests. We spawn a thread that holds the
/// Clipboard object alive for 30 seconds, which is plenty of time to paste.
/// On Wayland the clipboard is server-side so arboard returns immediately.
fn copy_to_clipboard(text: String) {
    std::thread::spawn(move || {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            if cb.set_text(&text).is_ok() {
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
        }
    });
}

// ── apply_update ──────────────────────────────────────────────────────────────

fn apply_update(update: AppUpdate, app: &mut App) {
    match update {
        AppUpdate::UserTranscript(text) => {
            app.user_buffer = text;
        }
        AppUpdate::AssistantTranscript(text) => {
            app.text_buffer.push_str(&text);
        }
        AppUpdate::Thinking(text) => {
            app.thinking_buffer.push_str(&text);
        }
        AppUpdate::VoiceText(text) => {
            app.speech_buffer = text;
        }
        AppUpdate::Warning(w) => {
            app.messages.push(ChatMessage::Warning(w));
        }
        AppUpdate::Done => {
            let user     = std::mem::take(&mut app.user_buffer).trim().to_string();
            let thinking = std::mem::take(&mut app.thinking_buffer).trim().to_string();
            let speech   = std::mem::take(&mut app.speech_buffer).trim().to_string();
            let text     = std::mem::take(&mut app.text_buffer).trim().to_string();

            if !user.is_empty() || !thinking.is_empty() || !speech.is_empty() || !text.is_empty() {
                app.messages.push(ChatMessage::Turn { user, thinking, speech, text });
            }
            if app.is_one_shot {
                app.session_tx = None;
            }
            if !matches!(app.state, AppState::Recording | AppState::Playing) {
                app.state = AppState::Idle;
            }
        }
        AppUpdate::PlaybackFinished => {
            app.playback_stop = None;
            if matches!(app.state, AppState::Playing) {
                app.state = AppState::Idle;
            }
        }
        AppUpdate::Error(e) => {
            app.messages.push(ChatMessage::ErrorMsg(e.clone()));
            if let Some(ref flag) = app.playback_stop {
                flag.store(true, Ordering::Relaxed);
            }
            app.playback_stop = None;
            app.session_tx = None;
            app.state = AppState::Error(e);
        }
        AppUpdate::SessionHandle(handle) => {
            app.session_handle = Some(handle);
        }
        AppUpdate::Reconnecting => {
            if !matches!(app.state, AppState::Recording) {
                app.state = AppState::Reconnecting;
            }
            app.messages.push(ChatMessage::System("↺ Reconnecting...".to_string()));
        }
        AppUpdate::PlaybackStarted(flag) => {
            app.playback_stop = Some(flag);
            if matches!(app.state, AppState::Idle | AppState::Processing) {
                app.state = AppState::Playing;
            }
        }
        AppUpdate::ModeDetected(supports_text) => {
            app.supports_text = Some(supports_text);
        }
    }
}

// ── Persistent session task ───────────────────────────────────────────────────

/// Long-lived background task that owns a single WebSocket connection to Gemini.
///
/// Uses `tokio::select!` on `audio_rx` (user utterances from UI) and
/// `receiver.receive_response()` (server messages) simultaneously, so both
/// idle server messages (goAway, resumption updates) and barge-in audio
/// are handled without blocking each other.
///
/// Session state machine (SessionPhase):
///   Idle         — waiting for the user's next utterance or a server-initiated event
///   WaitingResponse — audio sent, streaming server response back to UI
///
/// On goAway: saves session handle, signals UI with Reconnecting, reconnects transparently.
/// On connection drop with a saved handle: same reconnect flow.
/// On barge-in (SendAudio while WaitingResponse): clears buffers, sends new utterance,
///   sets pending_barge_in=true. When `interrupted` arrives for the old turn, continues
///   waiting for the new turn's response without going to Idle.
async fn run_persistent_session(
    prompt: String,
    voice: String,
    mut audio_rx: mpsc::UnboundedReceiver<TaskControl>,
    tx: mpsc::UnboundedSender<AppUpdate>,
    is_one_shot: bool,
    conversation_id: u64,
) {
    use crate::capabilities::{
        OutputTextMode, resolve_modalities, write_cache_entry,
        is_modality_error, is_modality_ws_error, modality_error_reason, current_cache_coords,
    };

    let mut session_handle: Option<String> = None;
    let mut turn_num: u64 = 0;
    let mut is_reconnect = false;
    // Resolved once on first successful setup; reused on goAway reconnects.
    let mut known_supports_text: Option<bool> = None;
    // Set when goAway fires during Streaming. Cleared after auto-resume on new connection.
    let mut was_streaming_on_disconnect = false;
    // Set when EndUtterance arrived in the drain (user finished before reconnect completed).
    let mut had_utterance_end_during_drain = false;

    'reconnect: loop {
        // Only drain stale commands on a true reconnect (goAway / connection drop).
        // On the first connection the user may have already queued BeginUtterance +
        // AudioChunks while the session was connecting — draining would discard them.
        if is_reconnect {
            had_utterance_end_during_drain = false;
            loop {
                match audio_rx.try_recv() {
                    Ok(TaskControl::Quit) => break 'reconnect,
                    Ok(TaskControl::EndUtterance) => { had_utterance_end_during_drain = true; }
                    Ok(_) => {} // discard stale BeginUtterance / AudioChunk
                    Err(_) => break,
                }
            }
            // If goAway interrupted streaming we will auto-resume — don't send Done yet.
            // For non-streaming reconnects, a stale EndUtterance means we should reset the UI.
            if !was_streaming_on_disconnect && had_utterance_end_during_drain {
                let _ = tx.send(AppUpdate::Done);
            }
        }

        // ── Resolve modalities for this iteration ──────────────────────────────
        // On reconnects (goAway) reuse what was already determined. On first connect,
        // consult the capability cache / env var.
        let output_mode = OutputTextMode::from_env();
        let (modalities, mode_from_cache) = if let Some(known) = known_supports_text {
            // Already resolved — use it directly, no cache lookup needed
            let mods = if known {
                vec!["AUDIO".to_string(), "TEXT".to_string()]
            } else {
                vec!["AUDIO".to_string()]
            };
            (mods, Some(known))
        } else {
            resolve_modalities(&output_mode)
        };

        // If mode was already resolved from cache/env, tell the UI immediately.
        // If not (probe needed), we'll tell it after setupComplete.
        if let Some(s) = mode_from_cache {
            let _ = tx.send(AppUpdate::ModeDetected(s));
        }

        // Cache coords for writing results after a probe (None if no API key in env).
        let cache_coords = current_cache_coords();

        // Connect
        let mut client = match GeminiClient::connect().await {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppUpdate::Error(format!("Connection failed: {:?}", e)));
                return;
            }
        };

        // Setup — persistent mode includes resumption + context compression
        let setup_result = if is_one_shot {
            client.send_setup(Some(prompt.clone()), Some(voice.clone())).await
        } else {
            client
                .send_setup_persistent_with_modalities(
                    Some(prompt.clone()),
                    Some(voice.clone()),
                    session_handle.clone(),
                    &modalities,
                )
                .await
        };

        if let Err(e) = setup_result {
            let _ = tx.send(AppUpdate::Error(format!("Setup failed: {:?}", e)));
            return;
        }

        // Wait for setupComplete — also detect modality rejection here.
        let mut setup_modality_rejected = false;
        'setup: loop {
            match tokio::time::timeout(
                Duration::from_secs(15),
                client.receive_response(),
            )
            .await
            {
                Ok(Ok(r)) if r.setup_complete.is_some() => {
                    // Probe succeeded — if this was a probe, cache the positive result.
                    if mode_from_cache.is_none() {
                        let supports = modalities.contains(&"TEXT".to_string());
                        known_supports_text = Some(supports);
                        let _ = tx.send(AppUpdate::ModeDetected(supports));
                        if let Some((ck, hint, model)) = &cache_coords {
                            write_cache_entry(ck, hint, model, supports, "probed_ok");
                        }
                    }
                    break 'setup;
                }
                Ok(Ok(r)) if r.error.is_some() => {
                    let e = r.error.unwrap();
                    if mode_from_cache.is_none() && is_modality_error(e.code, &e.message) {
                        let reason = modality_error_reason(e.code, &e.message);
                        if let Some((ck, hint, model)) = &cache_coords {
                            write_cache_entry(ck, hint, model, false, reason);
                        }
                        known_supports_text = Some(false);
                        let _ = tx.send(AppUpdate::ModeDetected(false));
                        setup_modality_rejected = true;
                        break 'setup;
                    }
                    let _ = tx.send(AppUpdate::Error(format!("Setup error: {}", e.message)));
                    return;
                }
                Ok(Ok(_)) => {} // ignore other messages during setup
                Ok(Err(e)) => {
                    // Server sometimes closes the WebSocket at protocol level (close frame)
                    // instead of sending a JSON error body. Check if this is a modality
                    // rejection when we're actively probing.
                    if mode_from_cache.is_none() && is_modality_ws_error(&format!("{:?}", e)) {
                        let reason = "modality_not_supported";
                        if let Some((ck, hint, model)) = &cache_coords {
                            write_cache_entry(ck, hint, model, false, reason);
                        }
                        known_supports_text = Some(false);
                        let _ = tx.send(AppUpdate::ModeDetected(false));
                        setup_modality_rejected = true;
                        break 'setup;
                    }
                    let _ = tx.send(AppUpdate::Error(format!("Setup receive failed: {:?}", e)));
                    return;
                }
                Err(_) => {
                    let _ = tx.send(AppUpdate::Error("Setup timed out".to_string()));
                    return;
                }
            }
        }

        // Modality probe was rejected — close and retry with AUDIO-only.
        // Don't drain audio_rx (user's queued utterance is still valid).
        if setup_modality_rejected {
            drop(client);
            continue 'reconnect;
        }

        // Split for select! — sender for writes, receiver for reads
        let (mut sender, mut receiver) = client.split();

        // If goAway fired during an active recording, transparently resume streaming on the
        // new connection. The recorder never stopped — we lost the audio from the dead
        // connection window, but everything from here forward is captured normally.
        let initial_phase = if was_streaming_on_disconnect {
            was_streaming_on_disconnect = false;
            if sender.send_activity_start().await.is_ok() {
                if had_utterance_end_during_drain {
                    // User finished speaking before reconnect completed — close the window.
                    had_utterance_end_during_drain = false;
                    if sender.send_activity_end().await.is_ok() {
                        SessionPhase::WaitingResponse {
                            audio_tx: None,
                            audio_buffer: Vec::new(),
                            user_transcript: SttTracker::default(),
                            voice_transcript: SttTracker::default(),
                            assistant_transcript: String::new(),
                            pending_barge_in: false,
                        }
                    } else {
                        let _ = tx.send(AppUpdate::Done);
                        SessionPhase::Idle
                    }
                } else {
                    // User is still speaking — chunks will keep flowing via audio_rx.
                    SessionPhase::Streaming {
                        user_transcript: SttTracker::default(),
                        voice_transcript: SttTracker::default(),
                        assistant_transcript: String::new(),
                        is_barge_in: false,
                    }
                }
            } else {
                // activityStart failed on new connection — give up, reset UI
                had_utterance_end_during_drain = false;
                let _ = tx.send(AppUpdate::Done);
                SessionPhase::Idle
            }
        } else {
            SessionPhase::Idle
        };

        // Session phase state
        enum SessionPhase {
            Idle,
            /// Mic is open and chunks are arriving. We relay each chunk to Gemini immediately.
            Streaming {
                user_transcript: SttTracker,
                voice_transcript: SttTracker,
                assistant_transcript: String,
                /// True when this utterance is a barge-in (BeginUtterance arrived during
                /// WaitingResponse). Carried into WaitingResponse as pending_barge_in.
                is_barge_in: bool,
            },
            WaitingResponse {
                /// Sender to the live streaming playback thread.
                /// None until the first audio chunk arrives; dropped (→ channel close → PA drain)
                /// on turn end or barge-in.
                audio_tx: Option<std::sync::mpsc::Sender<Vec<u8>>>,
                /// Audio buffer for optional WAV recording. Always collected regardless of
                /// GEMINI_AUDIO_SAVE_RECORDINGS so the decision to write is deferred to turn-end.
                audio_buffer: Vec<u8>,
                user_transcript: SttTracker,
                voice_transcript: SttTracker,
                assistant_transcript: String,
                pending_barge_in: bool,
            },
        }
        let mut phase = initial_phase;
        let mut needs_reconnect = false;

        'session: loop {
            // Select between:
            //   1. A command from the UI (audio to send, quit)
            //   2. A server message (response content, goAway, resumption update, ping)
            // 60-second timeout on the receive side keeps the loop alive during idle.
            tokio::select! {
                biased; // prefer audio_rx so barge-in is low latency

                cmd = audio_rx.recv() => {
                    match cmd {
                        None | Some(TaskControl::Quit) => break 'reconnect,

                        Some(TaskControl::BeginUtterance) => {
                            // Barge-in: if we're mid-response, drop the playback channel
                            // (stop_flag was already set by the UI) and clear stale buffers.
                            let is_barge_in = match phase {
                                SessionPhase::WaitingResponse {
                                    ref mut audio_tx,
                                    ref mut audio_buffer,
                                    ref mut user_transcript,
                                    ref mut voice_transcript,
                                    ref mut assistant_transcript,
                                    ..
                                } => {
                                    let _ = audio_tx.take(); // closes playback channel → thread exits
                                    audio_buffer.clear();
                                    user_transcript.clear();
                                    voice_transcript.clear();
                                    assistant_transcript.clear();
                                    true
                                }
                                _ => false,
                            };

                            // PRIVACY: mic is open and streaming when BeginUtterance arrives.
                            // activityStart tells Gemini the utterance window is open.
                            if let Err(e) = sender.send_activity_start().await {
                                let _ = tx.send(AppUpdate::Error(format!("{:?}", e)));
                                needs_reconnect = !is_one_shot;
                                break 'session;
                            }

                            phase = SessionPhase::Streaming {
                                user_transcript: SttTracker::new(),
                                voice_transcript: SttTracker::new(),
                                assistant_transcript: String::new(),
                                is_barge_in,
                            };
                        }

                        Some(TaskControl::AudioChunk(chunk)) => {
                            // Forward live mic chunk to Gemini.
                            // If not in Streaming (e.g. stale chunk after EndUtterance), discard.
                            if matches!(phase, SessionPhase::Streaming { .. }) {
                                if let Err(e) = sender.send_audio(&chunk).await {
                                    let _ = tx.send(AppUpdate::Error(format!("{:?}", e)));
                                    needs_reconnect = !is_one_shot;
                                    break 'session;
                                }
                            }
                        }

                        Some(TaskControl::EndUtterance) => {
                            // Only valid in Streaming phase. Stale EndUtterance (e.g., after
                            // reconnect drained BeginUtterance) arrives in Idle — send Done
                            // so the UI resets from Processing rather than hanging.
                            if let SessionPhase::Streaming { user_transcript, voice_transcript, assistant_transcript, is_barge_in } =
                                std::mem::replace(&mut phase, SessionPhase::Idle)
                            {
                                // PRIVACY: mic is fully closed (recorder.stop() completed in the UI
                                // thread) before EndUtterance was sent. activityEnd closes the window.
                                if let Err(e) = sender.send_activity_end().await {
                                    let _ = tx.send(AppUpdate::Error(format!("{:?}", e)));
                                    needs_reconnect = !is_one_shot;
                                    break 'session;
                                }

                                phase = SessionPhase::WaitingResponse {
                                    audio_tx: None,
                                    audio_buffer: Vec::new(),
                                    user_transcript,
                                    voice_transcript,
                                    assistant_transcript,
                                    pending_barge_in: is_barge_in,
                                };
                            } else {
                                // Stale EndUtterance — reset UI
                                let _ = tx.send(AppUpdate::Done);
                            }
                        }
                    }
                }

                timed_result = tokio::time::timeout(
                    // While generating a response, use a generous timeout — the model may be
                    // producing a long reply. In Idle/Streaming, 60s is fine to keep the loop alive.
                    if matches!(phase, SessionPhase::WaitingResponse { .. }) {
                        Duration::from_secs(120)
                    } else {
                        Duration::from_secs(60)
                    },
                    receiver.receive_response()
                ) => {
                    match timed_result {
                        Err(_elapsed) => {
                            // Timeout: only an error during active response, idle is fine
                            if matches!(phase, SessionPhase::WaitingResponse { .. }) {
                                let _ = tx.send(AppUpdate::Error("Response timed out".to_string()));
                                let _ = tx.send(AppUpdate::Done);
                                phase = SessionPhase::Idle;
                                if is_one_shot { break 'session; }
                            }
                            // Idle timeout: loop again (keeps select! alive)
                        }

                        Ok(Err(e)) => {
                            // Connection error
                            if !is_one_shot && session_handle.is_some() {
                                let _ = tx.send(AppUpdate::Reconnecting);
                                needs_reconnect = true;
                            } else {
                                let _ = tx.send(AppUpdate::Error(format!("Connection lost: {:?}", e)));
                                let _ = tx.send(AppUpdate::Done);
                            }
                            break 'session;
                        }

                        Ok(Ok(response)) => {
                            // Update session resumption handle (server sends these periodically)
                            if let Some(update) = &response.session_resumption_update {
                                if let Some(handle) = &update.new_handle {
                                    session_handle = Some(handle.clone());
                                    let _ = tx.send(AppUpdate::SessionHandle(handle.clone()));
                                }
                            }

                            // goAway: server is terminating; reconnect with saved handle.
                            // If recording is in progress, mark it so we auto-resume on the
                            // new connection — the recorder keeps running across the reconnect.
                            if response.go_away.is_some() {
                                was_streaming_on_disconnect =
                                    matches!(phase, SessionPhase::Streaming { .. });
                                let _ = tx.send(AppUpdate::Reconnecting);
                                needs_reconnect = !is_one_shot;
                                break 'session;
                            }

                            if let Some(error) = &response.error {
                                let _ = tx.send(AppUpdate::Error(format!("{}", error.message)));
                                let _ = tx.send(AppUpdate::Done);
                                phase = SessionPhase::Idle;
                                if is_one_shot { break 'session; }
                                // Continue in persistent mode — might recover
                            }

                            if let Some(sc) = &response.server_content {
                                // Output transcription & model turns only arrive when we're WaitingResponse.
                                // However, input_transcription (user speech) arrives continuously from the
                                // server *while* we are Streaming logic.
                                // Extract mutable references based on current phase
                                let (mut user_t, mut voice_t, mut asst_t) = match &mut phase {
                                    SessionPhase::Streaming { user_transcript, voice_transcript, assistant_transcript, .. } => {
                                        (Some(user_transcript), Some(voice_transcript), Some(assistant_transcript))
                                    }
                                    SessionPhase::WaitingResponse { user_transcript, voice_transcript, assistant_transcript, .. } => {
                                        (Some(user_transcript), Some(voice_transcript), Some(assistant_transcript))
                                    }
                                    SessionPhase::Idle => (None, None, None),
                                };

                                if let Some(t) = &sc.input_transcription {
                                    if let Some(ref mut ut) = user_t {
                                        let updated = ut.update(&t.text);
                                        let _ = tx.send(AppUpdate::UserTranscript(updated));
                                    }
                                }

                                if let Some(t) = &sc.output_transcription {
                                    if let Some(ref mut vt) = voice_t {
                                        let updated = vt.update(&t.text);
                                        let _ = tx.send(AppUpdate::VoiceText(updated));
                                    }
                                }

                                if let Some(model_turn) = &sc.model_turn {
                                    for part in &model_turn.parts {
                                        if let Some(text) = &part.text {
                                            if part.thought {
                                                let _ = tx.send(AppUpdate::Thinking(text.clone()));
                                            } else {
                                                if let Some(ref mut at) = asst_t {
                                                    at.push_str(text);
                                                    let _ = tx.send(AppUpdate::AssistantTranscript(text.clone()));
                                                }
                                            }
                                        }
                                    }
                                }
                                if let SessionPhase::WaitingResponse {
                                    audio_tx,
                                    audio_buffer,
                                    ..
                                } = &mut phase {
                                    // Wait until we have the first audio chunk to start playback
                                    if let Ok(Some(pcm)) = GeminiClient::extract_audio_data(&response) {
                                            if audio_tx.is_none() {
                                                // First chunk: open PulseAudio and start the playback thread.
                                                let (atx, arx) = std::sync::mpsc::channel::<Vec<u8>>();
                                                let stop_flag = Arc::new(AtomicBool::new(false));
                                                let _ = tx.send(AppUpdate::PlaybackStarted(stop_flag.clone()));
                                                let tx2 = tx.clone();
                                                std::thread::spawn(move || {
                                                    if let Err(e) = audio::stream_pcm_pulseaudio(
                                                        arx,
                                                        config::OUTPUT_SAMPLE_RATE,
                                                        &stop_flag,
                                                    ) {
                                                        let _ = tx2.send(AppUpdate::Warning(
                                                            format!("Playback: {:?}", e),
                                                        ));
                                                    }
                                                    let _ = tx2.send(AppUpdate::PlaybackFinished);
                                                });
                                                *audio_tx = Some(atx);
                                            }
                                            audio_buffer.extend_from_slice(&pcm);
                                            // Ignore send error: stop_flag was set → thread already exiting
                                            if let Some(ref atx) = audio_tx {
                                                let _ = atx.send(pcm);
                                            }
                                        }

                                        // generationComplete → audio generation done; drop the channel
                                        // so the PulseAudio streaming thread starts draining.
                                        let is_audio_end = sc.generation_complete.unwrap_or(false)
                                            || sc.turn_complete.unwrap_or(false)
                                            || sc.interrupted.unwrap_or(false);

                                        let is_turn_end = sc.turn_complete.unwrap_or(false)
                                            || sc.interrupted.unwrap_or(false);

                                        if is_audio_end {
                                            let pb = match phase { SessionPhase::WaitingResponse { pending_barge_in, .. } => pending_barge_in, _ => false };
                                            if pb && sc.interrupted.unwrap_or(false) {
                                                if let SessionPhase::WaitingResponse {
                                                    audio_tx, audio_buffer, user_transcript, voice_transcript, assistant_transcript, pending_barge_in
                                                } = &mut phase {
                                                    let _ = audio_tx.take();
                                                    turn_num += 1;
                                                    let ut = std::mem::take(user_transcript).full_text();
                                                    let vt = std::mem::take(voice_transcript).full_text();
                                                    let asst = std::mem::take(assistant_transcript);
                                                    let at = format!("{}{}", vt, asst);
                                                    let v = voice.clone();
                                                    let audio_save = std::mem::take(audio_buffer);
                                                    let save_wav = std::env::var("GEMINI_AUDIO_SAVE_RECORDINGS")
                                                        .map(|v| !v.is_empty() && v != "0")
                                                        .unwrap_or(false);
                                                    let cid = conversation_id;
                                                    let tn = turn_num;
                                                    tokio::spawn(async move {
                                                        append_turn_to_conversation(
                                                            cid, tn, &v, &ut, &at,
                                                            if save_wav { &audio_save } else { &[] },
                                                        ).await;
                                                    });
                                                    audio_buffer.clear();
                                                    user_transcript.clear();
                                                    voice_transcript.clear();
                                                    assistant_transcript.clear();
                                                    *pending_barge_in = false;
                                                }
                                            } else {
                                                if let SessionPhase::WaitingResponse { audio_tx, .. } = &mut phase {
                                                    let _ = audio_tx.take();
                                                }
                                            }
                                        }

                                        if is_turn_end {
                                            let pb = match phase { SessionPhase::WaitingResponse { pending_barge_in, .. } => pending_barge_in, _ => false };
                                            if !(pb && sc.interrupted.unwrap_or(false)) {
                                                if sc.interrupted.unwrap_or(false) {
                                                    let _ = tx.send(AppUpdate::Warning(
                                                        "Response was interrupted — partial audio only".to_string(),
                                                    ));
                                                }
                                                if let SessionPhase::WaitingResponse {
                                                    audio_tx: _, audio_buffer, user_transcript, voice_transcript, assistant_transcript, pending_barge_in: _
                                                } = std::mem::replace(&mut phase, SessionPhase::Idle) {
                                                    turn_num += 1;
                                                    let ut = user_transcript.full_text();
                                                    let vt = voice_transcript.full_text();
                                                    let asst = assistant_transcript;
                                                    let at = format!("{}{}", vt, asst);
                                                    let v = voice.clone();
                                                    let audio_save = audio_buffer;
                                                    let save_wav = std::env::var("GEMINI_AUDIO_SAVE_RECORDINGS")
                                                        .map(|v| !v.is_empty() && v != "0")
                                                        .unwrap_or(false);
                                                    let cid = conversation_id;
                                                    let tn = turn_num;
                                                    tokio::spawn(async move {
                                                        append_turn_to_conversation(
                                                            cid, tn, &v, &ut, &at,
                                                            if save_wav { &audio_save } else { &[] },
                                                        ).await;
                                                    });
                                                }
                                                let _ = tx.send(AppUpdate::Done);
                                                if is_one_shot {
                                                    break 'session;
                                                }
                                            }
                                        }
                                    }
                            }
                            // Idle phase: non-response server messages (goAway handled above)
                        }
                    }
                }
            }
        } // 'session

        // Close before reconnecting or exiting. Dead connections can hang sender.close()
        // indefinitely at the TCP layer, so cap it hard. Ignore the result — we're done with it.
        let _ = tokio::time::timeout(Duration::from_millis(500), sender.close()).await;

        if is_one_shot || !needs_reconnect {
            break 'reconnect;
        }

        // Brief pause before reconnecting (avoid hammering on transient errors)
        tokio::time::sleep(Duration::from_millis(800)).await;
        is_reconnect = true;
        // Loop 'reconnect with updated session_handle
    }
}

// ── Conversation log (async, one file per TUI session) ───────────────────────

/// Appends a completed turn to `conversations/<conversation_id>.jsonl`.
///
/// Each line is one self-contained JSON record — append-only, no parsing of existing data.
/// View a full conversation: `cat ~/.local/share/gemini-audio/conversations/1741334400.jsonl`
/// Pretty-print:            `jq . ~/.local/share/gemini-audio/conversations/1741334400.jsonl`
///
/// Pass `output_audio = &[]` to skip WAV writing (when GEMINI_AUDIO_SAVE_RECORDINGS is unset).
async fn append_turn_to_conversation(
    conversation_id: u64,
    turn_num: u64,
    voice: &str,
    user_transcript: &str,
    assistant_transcript: &str,
    output_audio: &[u8],
) {
    let Some(base_dirs) = directories_next::BaseDirs::new() else { return };

    let data_dir = base_dirs.data_local_dir().join("gemini-audio");
    let conv_dir = data_dir.join("conversations");
    if tokio::fs::create_dir_all(&conv_dir).await.is_err() { return; }

    // Optionally write WAV
    let recording_path = if !output_audio.is_empty() {
        let rec_dir = data_dir.join("recordings");
        if tokio::fs::create_dir_all(&rec_dir).await.is_ok() {
            let path = rec_dir.join(format!("conv_{}_turn_{}.wav", conversation_id, turn_num));
            let audio_clone = output_audio.to_vec();
            let path_clone = path.clone();
            let ok = tokio::task::spawn_blocking(move || {
                audio::write_wav_pcm(&path_clone, &audio_clone, config::OUTPUT_SAMPLE_RATE).is_ok()
            })
            .await
            .unwrap_or(false);
            if ok { path.display().to_string() } else { String::new() }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Build one line and append to the JSONL file
    let record = serde_json::json!({
        "conversation_id": conversation_id,
        "turn": turn_num,
        "timestamp": Utc::now().to_rfc3339(),
        "voice": voice,
        "user": user_transcript.trim(),
        "assistant": assistant_transcript.trim(),
        "output_recording": recording_path,
    });
    if let Ok(mut line) = serde_json::to_string(&record) {
        line.push('\n');
        let conv_file = conv_dir.join(format!("{}.jsonl", conversation_id));
        // OpenOptions::append so concurrent turns don't clobber each other
        use tokio::io::AsyncWriteExt;
        if let Ok(mut f) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&conv_file)
            .await
        {
            let _ = f.write_all(line.as_bytes()).await;
        }
    }
}

// ── Strip markdown ────────────────────────────────────────────────────────────

fn strip_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            i += 2;
        } else if chars[i] == '*' || chars[i] == '_' {
            i += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

// ── Message rendering ─────────────────────────────────────────────────────────
//
// Color palette (xterm-256 indexed for terminal compatibility):
//   [You]:    brackets Indexed(240) gray   | label Indexed(82) bright-green | body Indexed(255) white
//   [Gemini]: brackets Indexed(240) gray   | label Indexed(87) light-cyan
//   ◈ Speech: prefix Indexed(67) steel-blue | body Indexed(152) pale-blue
//   ◈ Text:   prefix Indexed(136) gold     | body Indexed(222) light-yellow
//   ◈ Think:  prefix Indexed(67) steel-blue | body Indexed(238) dim-gray italic
//   Divider:  Indexed(238) — light dashed rule
//   System:   DarkGray
//   Warning:  Yellow
//   Error:    Red
//
// Each message produces a Vec<Line<'static>>. Newlines within content are split into
// continuation lines with an indent prefix instead of a label.

fn render_message(msg: &ChatMessage, show_thinking: bool) -> Vec<(Line<'static>, LineTag)> {
    match msg {
        ChatMessage::Divider => {
            vec![(Line::from(Span::styled(
                "  ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌".to_string(),
                Style::default().fg(Color::Indexed(238)),
            )), LineTag::None)]
        }

        ChatMessage::System(s) => {
            vec![(Line::from(Span::styled(
                format!("  {}", s),
                Style::default().fg(Color::DarkGray),
            )), LineTag::WholeMsg)]
        }

        ChatMessage::Warning(w) => {
            vec![(Line::from(vec![
                Span::styled("  ⚠  ".to_string(), Style::default().fg(Color::Yellow)),
                Span::styled(w.clone(), Style::default().fg(Color::Yellow)),
            ]), LineTag::WholeMsg)]
        }

        ChatMessage::ErrorMsg(e) => {
            vec![(Line::from(vec![
                Span::styled("  ✗  ".to_string(), Style::default().fg(Color::Red)),
                Span::styled(e.clone(), Style::default().fg(Color::Red)),
            ]), LineTag::WholeMsg)]
        }

        ChatMessage::Turn { user, thinking, speech, text } => {
            let mut lines: Vec<(Line<'static>, LineTag)> = Vec::new();

            // ── [You]: section ────────────────────────────────────────────────
            if !user.is_empty() {
                let lbl_gray  = Style::default().fg(Color::Indexed(240));
                let lbl_green = Style::default().fg(Color::Indexed(82)).add_modifier(Modifier::BOLD);
                let body_col  = Style::default().fg(Color::Indexed(255));
                for (i, segment) in user.lines().enumerate() {
                    let line = if i == 0 {
                        Line::from(vec![
                            Span::styled("  [".to_string(),       lbl_gray),
                            Span::styled("You".to_string(),       lbl_green),
                            Span::styled("]: ".to_string(),       lbl_gray),
                            Span::styled(segment.to_string(),     body_col),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled("       ".to_string(),   Style::default()),
                            Span::styled(segment.to_string(),     body_col),
                        ])
                    };
                    lines.push((line, LineTag::User));
                }
            }

            // ── [Gemini]: section ─────────────────────────────────────────────
            let has_speech   = !speech.is_empty();
            let has_text     = !text.is_empty();
            let has_thinking = !thinking.is_empty() && show_thinking;

            if has_speech || has_text || has_thinking {
                let lbl_gray = Style::default().fg(Color::Indexed(240));
                let lbl_cyan = Style::default().fg(Color::Indexed(87)).add_modifier(Modifier::BOLD);
                lines.push((Line::from(vec![
                    Span::styled("  [".to_string(),      lbl_gray),
                    Span::styled("Gemini".to_string(),   lbl_cyan),
                    Span::styled("]".to_string(),        lbl_gray),
                ]), LineTag::None));

                // ◈ Speech ─────────────────────────────────────────────────────
                if has_speech {
                    let pfx_col  = Style::default().fg(Color::Indexed(67));
                    let lbl_col  = Style::default().fg(Color::Indexed(67)).add_modifier(Modifier::BOLD);
                    let body_col = Style::default().fg(Color::Indexed(152));
                    let cont     = "      ".to_string();
                    for (i, segment) in speech.lines().enumerate() {
                        let clean = strip_markdown(segment);
                        let line = if i == 0 {
                            Line::from(vec![
                                Span::styled("    ◈ ".to_string(),   pfx_col),
                                Span::styled("Speech".to_string(),  lbl_col),
                                Span::styled(": ".to_string(),      pfx_col),
                                Span::styled(clean,                 body_col),
                            ])
                        } else {
                            Line::from(vec![
                                Span::styled(cont.clone(),          Style::default()),
                                Span::styled(clean,                 body_col),
                            ])
                        };
                        lines.push((line, LineTag::Speech));
                    }
                }

                // ◈ Text ───────────────────────────────────────────────────────
                if has_text {
                    let pfx_col  = Style::default().fg(Color::Indexed(136));
                    let lbl_col  = Style::default().fg(Color::Indexed(136)).add_modifier(Modifier::BOLD);
                    let body_col = Style::default().fg(Color::Indexed(222));
                    let cont     = "      ".to_string();
                    for (i, segment) in text.lines().enumerate() {
                        let clean = strip_markdown(segment);
                        let line = if i == 0 {
                            Line::from(vec![
                                Span::styled("    ◈ ".to_string(),  pfx_col),
                                Span::styled("Text".to_string(),   lbl_col),
                                Span::styled(":   ".to_string(),   pfx_col),
                                Span::styled(clean,                body_col),
                            ])
                        } else {
                            Line::from(vec![
                                Span::styled(cont.clone(),         Style::default()),
                                Span::styled(clean,                body_col),
                            ])
                        };
                        lines.push((line, LineTag::Text));
                    }
                }

                // ◈ Think ──────────────────────────────────────────────────────
                if has_thinking {
                    let pfx_col  = Style::default().fg(Color::Indexed(67));
                    let lbl_col  = Style::default().fg(Color::Indexed(67)).add_modifier(Modifier::BOLD);
                    let body_col = Style::default().fg(Color::Indexed(238)).add_modifier(Modifier::ITALIC);
                    let cont     = "      ".to_string();
                    for (i, segment) in thinking.lines().enumerate() {
                        let clean = strip_markdown(segment);
                        let line = if i == 0 {
                            Line::from(vec![
                                Span::styled("    ◈ ".to_string(),  pfx_col),
                                Span::styled("Think".to_string(),  lbl_col),
                                Span::styled(": ".to_string(),     pfx_col),
                                Span::styled(clean,                body_col),
                            ])
                        } else {
                            Line::from(vec![
                                Span::styled(cont.clone(),         Style::default()),
                                Span::styled(clean,                body_col),
                            ])
                        };
                        lines.push((line, LineTag::Thinking));
                    }
                }
            }

            lines
        }
    }
}

// ── UI rendering ──────────────────────────────────────────────────────────────

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(1),    // Chat history
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    // ── Header: title | conv ID + mode indicator ──────────────────────────────
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(36)])
        .split(chunks[0]);

    let title = Paragraph::new(" ◆ GEMINI AUDIO ")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, header_chunks[0]);

    let (mode_label, mode_style) = match app.supports_text {
        Some(true)  => (" · Audio+Text", Style::default().fg(Color::Rgb(100, 220, 140)).add_modifier(Modifier::BOLD)),
        Some(false) => (" · Audio",      Style::default().fg(Color::DarkGray)),
        None        => ("",              Style::default()),
    };
    let conv_line = Line::from(vec![
        Span::styled(format!("  {}  ", app.conversation_id), Style::default().fg(Color::DarkGray)),
        Span::styled(mode_label, mode_style),
    ]);
    let conv_info = Paragraph::new(conv_line)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(conv_info, header_chunks[1]);

    // ── Chat history ──────────────────────────────────────────────────────────
    //
    // Build display list: committed messages + optional live (in-progress) turn.
    // line_to_msg[i] = Some(idx into app.messages) or None for the live turn.

    let mut display: Vec<(Option<usize>, &ChatMessage)> = app.messages
        .iter()
        .enumerate()
        .map(|(i, m)| (Some(i), m))
        .collect();

    // Live turn for in-progress streaming (owned, rendered at the bottom)
    let live_turn: Option<ChatMessage>;
    let has_live = !app.user_buffer.is_empty()
        || !app.thinking_buffer.is_empty()
        || !app.speech_buffer.is_empty()
        || !app.text_buffer.is_empty();

    if has_live {
        live_turn = Some(ChatMessage::Turn {
            user: app.user_buffer.clone(),
            thinking: if app.thinking_buffer.is_empty() {
                String::new()
            } else {
                format!("{}▌", app.thinking_buffer)
            },
            speech: if app.speech_buffer.is_empty() {
                String::new()
            } else {
                format!("{}▌", app.speech_buffer)
            },
            text: if app.text_buffer.is_empty() {
                String::new()
            } else {
                format!("{}▌", app.text_buffer)
            },
        });
        display.push((None, live_turn.as_ref().unwrap()));
    } else {
        live_turn = None;
    }

    // Render all messages to Lines and build click map simultaneously
    let mut all_lines: Vec<Line> = Vec::new();
    let mut new_line_to_msg: Vec<(Option<usize>, LineTag)> = Vec::new();

    for (maybe_idx, msg) in &display {
        for (line, tag) in render_message(msg, app.show_thinking) {
            all_lines.push(line);
            new_line_to_msg.push((*maybe_idx, tag));
        }
    }

    // Scroll to bottom
    let inner_width = chunks[1].width.saturating_sub(2).max(1) as usize;
    let area_height = chunks[1].height.saturating_sub(2) as usize;
    let total_rows: usize = all_lines.iter().map(|line| {
        let char_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        ((char_len.max(1) - 1) / inner_width) + 1
    }).sum();
    let scroll = total_rows.saturating_sub(area_height) as u16;

    // Store state for click handler
    app.line_to_msg = new_line_to_msg;
    app.chat_scroll = scroll;
    app.chat_area_top = chunks[1].y;

    let chat = Paragraph::new(all_lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::ALL).title(" [ CONVERSATION ] "));
    f.render_widget(chat, chunks[1]);

    // Drop live_turn to release borrows on app buffers before the footer widgets
    drop(live_turn);

    // ── Footer: Status | Legend | Voice ───────────────────────────────────────
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20), // Status
            Constraint::Min(1),     // Legend
            Constraint::Length(22), // Voice
        ])
        .split(chunks[2]);

    let (status_label, status_color) = match &app.state {
        AppState::Idle        => (" ◉  IDLE",          Color::DarkGray),
        AppState::Recording   => (" ●  RECORDING",     Color::Red),
        AppState::Processing  => (" ⟳  PROCESSING",    Color::Yellow),
        AppState::Playing     => (" ▶  PLAYING",       Color::Cyan),
        AppState::Reconnecting => (" ↺  RECONNECTING", Color::Yellow),
        AppState::Error(_)    => (" ✗  ERROR",         Color::Red),
    };
    let status = Paragraph::new(status_label)
        .style(Style::default().fg(status_color).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title(" Status "));
    f.render_widget(status, footer_chunks[0]);

    let dim = Style::default().fg(Color::DarkGray);
    let key = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
    let (think_key, think_style) = if app.show_thinking {
        ("t", Style::default().fg(Color::Indexed(87)).add_modifier(Modifier::BOLD))
    } else {
        ("t", key)
    };
    let legend_line = Line::from(vec![
        Span::styled(" SPC ",  key),   Span::styled("Rec/Send ",  dim),
        Span::styled(" c ",    key),   Span::styled("Cancel ",    dim),
        Span::styled(" TAB ",  key),   Span::styled("Voice ",     dim),
        Span::styled(format!(" {} ", think_key), think_style),
                                       Span::styled("Think ",     dim),
        Span::styled(" q ",    key),   Span::styled("Quit",       dim),
    ]);
    let legend = Paragraph::new(legend_line)
        .block(Block::default().borders(Borders::ALL).title(" Keys "));
    f.render_widget(legend, footer_chunks[1]);

    let voice_color = if app.voice_configurable() { Color::Cyan } else { Color::DarkGray };
    let voice_label = format!(" {} ({}/{}) ", app.voice(), app.voice_idx + 1, crate::config::VOICES.len());
    let voice_widget = Paragraph::new(voice_label)
        .style(Style::default().fg(voice_color))
        .block(Block::default().borders(Borders::ALL).title(" Voice "));
    f.render_widget(voice_widget, footer_chunks[2]);
}
