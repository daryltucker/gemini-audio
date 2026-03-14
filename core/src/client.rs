// WebSocket client for Gemini Live API

use crate::error::{GeminiAudioError, Result};
use crate::config::{WEBSOCKET_ENDPOINT, MODEL_ID};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::env;
use tokio_tungstenite::{connect_async, tungstenite::Message, tungstenite::client::IntoClientRequest};
use base64::{Engine as _, engine::general_purpose};
use tracing::debug;

// Type aliases for split WebSocket halves
type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsStream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

// ── Setup frames ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupFrame {
    setup: SetupConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupConfig {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    generation_config: GenerationConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_audio_transcription: Option<AudioTranscriptionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_audio_transcription: Option<AudioTranscriptionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    realtime_input_config: Option<RealtimeInputConfig>,
    /// Enables session resumption; `handle` is None on first connect, Some on reconnect.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_resumption: Option<SessionResumptionConfig>,
    /// Enables sliding-window context compression to keep sessions alive beyond the 128k limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    context_window_compression: Option<ContextWindowCompressionConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioTranscriptionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
}

/// Disable server-side VAD so we can manually bracket audio with activityStart/activityEnd.
/// This prevents VAD from firing mid-sentence on brief pauses in pre-recorded audio.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RealtimeInputConfig {
    automatic_activity_detection: AutomaticActivityDetection,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomaticActivityDetection {
    disabled: bool,
}

/// Pass `handle` from the last `SessionResumptionUpdate.new_handle` to resume a session.
/// Pass `None` (omit handle) to start a fresh session.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResumptionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    handle: Option<String>,
}

/// Sliding-window context compression. Triggers at ~80% of the 128k token limit.
/// Combined with session resumption this gives effectively unlimited-length sessions.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextWindowCompressionConfig {
    sliding_window: SlidingWindow,
}

/// Empty struct — serializes to `{}` as required by the API
#[derive(Debug, Serialize)]
struct SlidingWindow {}

/// Empty marker struct — serializes to `{}` as required by activityStart/activityEnd
#[derive(Debug, Serialize)]
struct ActivityMarker {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(default)]
    response_modalities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speech_config: Option<SpeechConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpeechConfig {
    voice_config: VoiceConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceConfig {
    prebuilt_voice_config: PrebuiltVoiceConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrebuiltVoiceConfig {
    voice_name: String,
}

// ── Audio input frames ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioInputFrame {
    realtime_input: RealtimeInput,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RealtimeInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    audio: Option<AudioBlob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_stream_end: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_start: Option<ActivityMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_end: Option<ActivityMarker>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioBlob {
    mime_type: String,
    data: String,
}

// ── Server response frames ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerResponse {
    #[serde(default)]
    pub server_content: Option<ServerContent>,
    #[serde(default)]
    pub setup_complete: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<ServerError>,
    /// Server is about to close the connection (approaching 10-minute hard limit).
    /// Save the latest session handle and reconnect transparently.
    #[serde(default)]
    pub go_away: Option<GoAway>,
    /// Updated resumption handle from the server. Save `new_handle` and pass it as
    /// `session_resumption.handle` on the next `send_setup_persistent` call.
    #[serde(default)]
    pub session_resumption_update: Option<SessionResumptionUpdate>,
}

/// Server is about to terminate the connection (approaching 10-minute hard limit).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoAway {
    /// Approximate time remaining before forced disconnection (e.g. "5s")
    #[serde(default)]
    pub time_left: Option<String>,
}

/// Server-assigned resumption token. Store the latest `new_handle`; pass it
/// as `handle` in `SessionResumptionConfig` on the next connection.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResumptionUpdate {
    /// New resumption handle. Valid for ~2 hours after the session ends.
    #[serde(default)]
    pub new_handle: Option<String>,
    /// False while Gemini is actively generating (handle captures mid-turn state).
    #[serde(default)]
    pub resumable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerContent {
    #[serde(default)]
    pub model_turn: Option<ModelTurn>,
    #[serde(default)]
    pub turn_complete: Option<bool>,
    #[serde(default)]
    pub generation_complete: Option<bool>,
    #[serde(default)]
    pub interrupted: Option<bool>,
    #[serde(default)]
    pub input_transcription: Option<Transcription>,
    #[serde(default)]
    pub output_transcription: Option<Transcription>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcription {
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTurn {
    #[serde(default)]
    pub parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<InlineData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// True if this part is an internal thinking token, not a user-facing response
    #[serde(default)]
    pub thought: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerError {
    #[serde(default)]
    pub code: i32,
    #[serde(default)]
    pub message: String,
}

// ── GeminiSender ──────────────────────────────────────────────────────────────

/// Send half of a split Gemini WebSocket client.
/// Obtained via `GeminiClient::split()`. Use with `GeminiReceiver` in `tokio::select!`.
pub struct GeminiSender {
    ws_sender: WsSink,
}

impl GeminiSender {
    async fn send_raw(&mut self, json: String) -> Result<()> {
        self.ws_sender
            .send(Message::Text(json))
            .await
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to send frame: {}", e)))
    }

    fn build_setup_frame(
        system_instruction: Option<String>,
        voice: Option<String>,
        session_handle: Option<String>,
        persistent: bool,
        modalities: &[String],
    ) -> SetupFrame {
        let model = env::var("GEMINI_MODEL_ID").unwrap_or_else(|_| MODEL_ID.to_string());
        let voice_name = voice
            .filter(|v| !v.is_empty())
            .or_else(|| env::var("GEMINI_AUDIO_VOICE").ok().filter(|v| !v.is_empty()))
            .unwrap_or_else(|| "Fenrir".to_string());

        SetupFrame {
            setup: SetupConfig {
                model,
                system_instruction: system_instruction.map(|text| SystemInstruction {
                    parts: vec![Part {
                        text: Some(text),
                        inline_data: None,
                        thought: false,
                    }],
                }),
                generation_config: GenerationConfig {
                    response_modalities: modalities.to_vec(),
                    speech_config: Some(SpeechConfig {
                        voice_config: VoiceConfig {
                            prebuilt_voice_config: PrebuiltVoiceConfig { voice_name },
                        },
                    }),
                },
                input_audio_transcription: Some(AudioTranscriptionConfig { language_code: None }),
                output_audio_transcription: Some(AudioTranscriptionConfig { language_code: None }),
                // Disable auto-VAD: we manually bracket audio with activityStart/activityEnd.
                // This prevents mid-sentence pauses from triggering early generation.
                realtime_input_config: Some(RealtimeInputConfig {
                    automatic_activity_detection: AutomaticActivityDetection { disabled: true },
                }),
                session_resumption: if persistent {
                    Some(SessionResumptionConfig { handle: session_handle })
                } else {
                    None
                },
                context_window_compression: if persistent {
                    Some(ContextWindowCompressionConfig { sliding_window: SlidingWindow {} })
                } else {
                    None
                },
            },
        }
    }

    /// Send setup for one-shot / batch mode. Always uses AUDIO-only modality.
    pub async fn send_setup(
        &mut self,
        system_instruction: Option<String>,
        voice: Option<String>,
    ) -> Result<()> {
        let mods = vec!["AUDIO".to_string()];
        let frame = Self::build_setup_frame(system_instruction, voice, None, false, &mods);
        let json = serde_json::to_string(&frame)
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to serialize setup: {}", e)))?;
        debug!("Sending setup (one-shot): {}", &json[..json.len().min(400)]);
        self.send_raw(json).await
    }

    /// Send setup for persistent mode with explicit modalities.
    /// Use this to request `["AUDIO", "TEXT"]` when probing for TEXT capability.
    pub async fn send_setup_persistent_with_modalities(
        &mut self,
        system_instruction: Option<String>,
        voice: Option<String>,
        session_handle: Option<String>,
        modalities: &[String],
    ) -> Result<()> {
        let frame = Self::build_setup_frame(system_instruction, voice, session_handle, true, modalities);
        let json = serde_json::to_string(&frame)
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to serialize setup: {}", e)))?;
        debug!("Sending setup (persistent, modalities={:?}): {}", modalities, &json[..json.len().min(400)]);
        self.send_raw(json).await
    }

    /// Send setup for persistent mode with AUDIO-only modality (convenience wrapper).
    pub async fn send_setup_persistent(
        &mut self,
        system_instruction: Option<String>,
        voice: Option<String>,
        session_handle: Option<String>,
    ) -> Result<()> {
        let mods = vec!["AUDIO".to_string()];
        self.send_setup_persistent_with_modalities(system_instruction, voice, session_handle, &mods).await
    }

    /// Send raw PCM audio data (16kHz, 16-bit signed, mono, little-endian).
    pub async fn send_audio(&mut self, audio_data: &[u8]) -> Result<()> {
        let base64_data = general_purpose::STANDARD.encode(audio_data);
        let frame = AudioInputFrame {
            realtime_input: RealtimeInput {
                audio: Some(AudioBlob {
                    mime_type: "audio/pcm;rate=16000".to_string(),
                    data: base64_data,
                }),
                audio_stream_end: None,
                activity_start: None,
                activity_end: None,
            },
        };
        let json = serde_json::to_string(&frame)
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to serialize audio: {}", e)))?;
        debug!("Sending audio frame ({} json bytes)", json.len());
        self.send_raw(json).await
    }

    /// Signal end of audio stream (for auto-VAD mode only; not used with manual VAD).
    pub async fn send_audio_stream_end(&mut self) -> Result<()> {
        let frame = AudioInputFrame {
            realtime_input: RealtimeInput {
                audio: None,
                audio_stream_end: Some(true),
                activity_start: None,
                activity_end: None,
            },
        };
        let json = serde_json::to_string(&frame)
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to serialize audioStreamEnd: {}", e)))?;
        debug!("Sending audioStreamEnd");
        self.send_raw(json).await
    }

    /// Signal manual activity start. Send before audio when auto-VAD is disabled.
    ///
    /// PRIVACY: Do not send until `start_recording()` has returned successfully (mic is open).
    /// When used during an active generation, the server interprets this as a barge-in signal.
    pub async fn send_activity_start(&mut self) -> Result<()> {
        let frame = AudioInputFrame {
            realtime_input: RealtimeInput {
                audio: None,
                audio_stream_end: None,
                activity_start: Some(ActivityMarker {}),
                activity_end: None,
            },
        };
        let json = serde_json::to_string(&frame)
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to serialize activityStart: {}", e)))?;
        debug!("Sending activityStart");
        self.send_raw(json).await
    }

    /// Signal manual activity end — tells the server the utterance is complete and to generate.
    ///
    /// PRIVACY: Do not send until `recorder.stop()` has returned (blocking join confirmed).
    /// This guarantees the OS-level capture handle is closed before the server is told
    /// the utterance is over. Nothing beyond the bounded recorded buffer is ever transmitted.
    pub async fn send_activity_end(&mut self) -> Result<()> {
        let frame = AudioInputFrame {
            realtime_input: RealtimeInput {
                audio: None,
                audio_stream_end: None,
                activity_start: None,
                activity_end: Some(ActivityMarker {}),
            },
        };
        let json = serde_json::to_string(&frame)
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to serialize activityEnd: {}", e)))?;
        debug!("Sending activityEnd");
        self.send_raw(json).await
    }

    pub async fn close(&mut self) -> Result<()> {
        self.ws_sender
            .close()
            .await
            .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to close connection: {}", e)))
    }
}

// ── GeminiReceiver ────────────────────────────────────────────────────────────

/// Receive half of a split Gemini WebSocket client.
/// Obtained via `GeminiClient::split()`. Use with `GeminiSender` in `tokio::select!`.
///
/// Note: Pings are silently dropped (no pong). Use `GeminiClient` (unsplit) if pong is required.
pub struct GeminiReceiver {
    ws_receiver: WsStream,
}

impl GeminiReceiver {
    /// Receive and parse the next `ServerResponse`. Silently skips ping/pong/raw frames.
    pub async fn receive_response(&mut self) -> Result<ServerResponse> {
        loop {
            let message = self.ws_receiver
                .next()
                .await
                .ok_or_else(|| GeminiAudioError::WebSocket("Connection closed".to_string()))?
                .map_err(|e| GeminiAudioError::WebSocket(format!("Receive error: {}", e)))?;

            match message {
                Message::Text(text) => {
                    println!("RAW_FRAME: {}", text);
                    debug!("Received: {}", &text[..text.len().min(500)]);
                    return serde_json::from_str(&text).map_err(|e| {
                        GeminiAudioError::WebSocket(format!(
                            "Parse error: {} (text: {})",
                            e,
                            &text[..text.len().min(200)]
                        ))
                    });
                }
                Message::Binary(data) => {
                    debug!("Received binary: {} bytes", data.len());
                    let text = String::from_utf8(data).map_err(|e| {
                        GeminiAudioError::WebSocket(format!("Binary decode error: {}", e))
                    })?;
                    return serde_json::from_str(&text)
                        .map_err(|e| GeminiAudioError::WebSocket(format!("Parse error: {}", e)));
                }
                Message::Ping(_) => {
                    debug!("Received ping (split mode — no pong sent)");
                }
                Message::Pong(_) => {
                    debug!("Received pong");
                }
                Message::Close(frame) => {
                    return Err(GeminiAudioError::WebSocket(format!(
                        "Connection closed: {:?}",
                        frame
                    )));
                }
                Message::Frame(_) => {}
            }
        }
    }
}

// ── GeminiClient ──────────────────────────────────────────────────────────────

/// Gemini WebSocket client.
///
/// For one-shot/batch mode (`--input`): use the methods directly.
/// For persistent/TUI mode: call `split()` to get `(GeminiSender, GeminiReceiver)` for use
/// with `tokio::select!` to multiplex between incoming audio and server responses.
pub struct GeminiClient {
    sender: GeminiSender,
    receiver: GeminiReceiver,
}

impl GeminiClient {
    /// Connect to Gemini WebSocket endpoint.
    pub async fn connect() -> Result<Self> {
        let endpoint = env::var("GEMINI_WS_ENDPOINT")
            .unwrap_or_else(|_| WEBSOCKET_ENDPOINT.to_string())
            .replace("https://", "wss://")
            .replace("http://", "ws://");

        let api_key = env::var("GEMINI_API_KEY").ok().filter(|k| !k.is_empty())
            .ok_or_else(|| GeminiAudioError::Authentication("No API key for Gemini".to_string()))?;

        let url = format!("{}?key={}", endpoint, api_key);
        debug!(
            "Connecting to Gemini: {}?key=***{}",
            endpoint,
            &api_key[api_key.len().saturating_sub(4)..]
        );
        let request = url.into_client_request()
            .map_err(|e| GeminiAudioError::WebSocket(format!("Invalid URL: {}", e)))?;

        debug!("Connecting to: {}", endpoint);

        let (ws_stream, _) = match connect_async(request).await {
            Ok(result) => result,
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                let status = response.status();
                let mut error_msg = format!(
                    "{} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                );
                if let Some(retry_after) = response.headers().get("retry-after") {
                    if let Ok(s) = retry_after.to_str() {
                        error_msg.push_str(&format!(" (Retry-After: {})", s));
                    }
                }
                if let Some(body) = response.body() {
                    if let Ok(body_str) = std::str::from_utf8(body) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body_str) {
                            if let Some(details) = json
                                .get("error")
                                .and_then(|e| e.get("details"))
                                .and_then(|d| d.as_array())
                            {
                                for detail in details {
                                    if let Some(delay) =
                                        detail.get("retryDelay").and_then(|d| d.as_str())
                                    {
                                        error_msg.push_str(&format!(
                                            " (Retry-After: {})",
                                            delay.trim_end_matches('s')
                                        ));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                return Err(GeminiAudioError::API(error_msg));
            }
            Err(e) => {
                return Err(GeminiAudioError::WebSocket(format!(
                    "Failed to connect to {}: {}",
                    endpoint, e
                )));
            }
        };

        let (ws_sender, ws_receiver) = ws_stream.split();
        Ok(Self {
            sender: GeminiSender { ws_sender },
            receiver: GeminiReceiver { ws_receiver },
        })
    }

    /// Split into independent sender and receiver halves for use with `tokio::select!`.
    /// Consumes `self`. After splitting, use `GeminiSender` for all writes and
    /// `GeminiReceiver` for all reads.
    pub fn split(self) -> (GeminiSender, GeminiReceiver) {
        (self.sender, self.receiver)
    }

    // ── Delegating methods (backwards compatibility for main.rs one-shot mode) ──

    pub async fn send_setup(&mut self, system_instruction: Option<String>, voice: Option<String>) -> Result<()> {
        self.sender.send_setup(system_instruction, voice).await
    }

    pub async fn send_setup_persistent(
        &mut self,
        system_instruction: Option<String>,
        voice: Option<String>,
        session_handle: Option<String>,
    ) -> Result<()> {
        self.sender.send_setup_persistent(system_instruction, voice, session_handle).await
    }

    pub async fn send_setup_persistent_with_modalities(
        &mut self,
        system_instruction: Option<String>,
        voice: Option<String>,
        session_handle: Option<String>,
        modalities: &[String],
    ) -> Result<()> {
        self.sender.send_setup_persistent_with_modalities(system_instruction, voice, session_handle, modalities).await
    }

    pub async fn send_audio(&mut self, audio_data: &[u8]) -> Result<()> {
        self.sender.send_audio(audio_data).await
    }

    pub async fn send_audio_stream_end(&mut self) -> Result<()> {
        self.sender.send_audio_stream_end().await
    }

    pub async fn send_activity_start(&mut self) -> Result<()> {
        self.sender.send_activity_start().await
    }

    pub async fn send_activity_end(&mut self) -> Result<()> {
        self.sender.send_activity_end().await
    }

    pub async fn close(&mut self) -> Result<()> {
        self.sender.close().await
    }

    /// Receive and parse the next server response, sending pong for pings.
    pub async fn receive_response(&mut self) -> Result<ServerResponse> {
        loop {
            let message = self.receiver.ws_receiver
                .next()
                .await
                .ok_or_else(|| GeminiAudioError::WebSocket("No response received".to_string()))?
                .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to receive message: {}", e)))?;

            match message {
                Message::Text(text) => {
                    debug!("Received: {}", &text[..text.len().min(500)]);
                    return serde_json::from_str(&text).map_err(|e| {
                        GeminiAudioError::WebSocket(format!(
                            "Failed to parse response: {} (text: {})",
                            e,
                            &text[..text.len().min(200)]
                        ))
                    });
                }
                Message::Binary(data) => {
                    debug!("Received binary message: {} bytes", data.len());
                    let text = String::from_utf8(data).map_err(|e| {
                        GeminiAudioError::WebSocket(format!("Failed to decode binary: {}", e))
                    })?;
                    debug!("Binary content: {}", &text[..text.len().min(500)]);
                    return serde_json::from_str(&text).map_err(|e| {
                        GeminiAudioError::WebSocket(format!("Failed to parse binary response: {}", e))
                    });
                }
                Message::Ping(data) => {
                    debug!("Received ping, sending pong");
                    self.sender
                        .ws_sender
                        .send(Message::Pong(data))
                        .await
                        .map_err(|e| GeminiAudioError::WebSocket(format!("Failed to send pong: {}", e)))?;
                }
                Message::Pong(_) => {
                    debug!("Received pong");
                }
                Message::Close(frame) => {
                    debug!("Received close: {:?}", frame);
                    return Err(GeminiAudioError::WebSocket(format!(
                        "Connection closed: {:?}",
                        frame
                    )));
                }
                Message::Frame(_) => {
                    debug!("Received raw frame");
                }
            }
        }
    }

    /// Extract and concatenate all audio parts from a server response.
    pub fn extract_audio_data(response: &ServerResponse) -> Result<Option<Vec<u8>>> {
        if let Some(server_content) = &response.server_content {
            if let Some(model_turn) = &server_content.model_turn {
                let mut all_audio: Vec<u8> = Vec::new();
                for part in &model_turn.parts {
                    if let Some(inline_data) = &part.inline_data {
                        match general_purpose::STANDARD
                            .decode(&inline_data.data)
                            .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(&inline_data.data))
                            .or_else(|_| general_purpose::URL_SAFE.decode(&inline_data.data))
                            .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(&inline_data.data))
                        {
                            Ok(chunk) => all_audio.extend(chunk),
                            Err(e) => {
                                tracing::warn!("Failed to decode base64 audio data: {}", e);
                            }
                        }
                    }
                }
                if !all_audio.is_empty() {
                    return Ok(Some(all_audio));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_frame_serialization() {
        let model = env::var("GEMINI_MODEL_ID").unwrap_or_else(|_| MODEL_ID.to_string());
        let setup_frame = SetupFrame {
            setup: SetupConfig {
                model: model.clone(),
                system_instruction: None,
                generation_config: GenerationConfig {
                    response_modalities: vec!["AUDIO".to_string()],
                    speech_config: None,
                },
                input_audio_transcription: Some(AudioTranscriptionConfig { language_code: None }),
                output_audio_transcription: None,
                realtime_input_config: None,
                session_resumption: None,
                context_window_compression: None,
            },
        };
        let json = serde_json::to_string(&setup_frame).unwrap();
        assert!(json.contains("setup"));
        assert!(json.contains(&model));
        assert!(!json.contains("sessionResumption"));
    }

    #[test]
    fn test_persistent_setup_serialization() {
        let model = env::var("GEMINI_MODEL_ID").unwrap_or_else(|_| MODEL_ID.to_string());
        let setup_frame = SetupFrame {
            setup: SetupConfig {
                model: model.clone(),
                system_instruction: None,
                generation_config: GenerationConfig {
                    response_modalities: vec!["AUDIO".to_string()],
                    speech_config: None,
                },
                input_audio_transcription: None,
                output_audio_transcription: None,
                realtime_input_config: None,
                session_resumption: Some(SessionResumptionConfig {
                    handle: Some("test-handle-abc".to_string()),
                }),
                context_window_compression: Some(ContextWindowCompressionConfig {
                    sliding_window: SlidingWindow {},
                }),
            },
        };
        let json = serde_json::to_string(&setup_frame).unwrap();
        assert!(json.contains("sessionResumption"));
        assert!(json.contains("contextWindowCompression"));
        assert!(json.contains("slidingWindow"));
        assert!(json.contains("test-handle-abc"));
    }

    #[test]
    fn test_audio_input_frame_serialization() {
        let audio_data = vec![0u8, 1, 2, 3, 4, 5];
        let base64_data = general_purpose::STANDARD.encode(&audio_data);
        let audio_frame = AudioInputFrame {
            realtime_input: RealtimeInput {
                audio: Some(AudioBlob {
                    mime_type: "audio/pcm;rate=16000".to_string(),
                    data: base64_data,
                }),
                audio_stream_end: None,
                activity_start: None,
                activity_end: None,
            },
        };
        let json = serde_json::to_string(&audio_frame).unwrap();
        assert!(json.contains("realtimeInput"));
        assert!(json.contains("audio/pcm;rate=16000"));
    }

    #[test]
    fn test_setup_complete_deserialization() {
        let json = r#"{"setupComplete": {}}"#;
        let response: ServerResponse = serde_json::from_str(json).unwrap();
        assert!(response.setup_complete.is_some());
    }

    #[test]
    fn test_go_away_deserialization() {
        let json = r#"{"goAway": {"timeLeft": "5s"}}"#;
        let response: ServerResponse = serde_json::from_str(json).unwrap();
        let go_away = response.go_away.unwrap();
        assert_eq!(go_away.time_left.as_deref(), Some("5s"));
    }

    #[test]
    fn test_session_resumption_update_deserialization() {
        let json = r#"{"sessionResumptionUpdate": {"newHandle": "abc123xyz", "resumable": true}}"#;
        let response: ServerResponse = serde_json::from_str(json).unwrap();
        let update = response.session_resumption_update.unwrap();
        assert_eq!(update.new_handle.as_deref(), Some("abc123xyz"));
        assert_eq!(update.resumable, Some(true));
    }
}
