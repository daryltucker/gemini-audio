# Gemini Live API — WebSocket Spec Reference

> Source: [ai.google.dev/api/live](https://ai.google.dev/api/live) · [Live API capabilities guide](https://ai.google.dev/gemini-api/docs/live-guide)
> Last verified: March 2026

---

## Connection

```
wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={API_KEY}
```

The connection is a full-duplex WebSocket. All messages are JSON text frames. The client sends exactly one field per message (a union type).

---

## Session Lifecycle

1. Client opens WebSocket
2. Client sends `setup` message (must be first)
3. Server replies with `setupComplete`
4. Client streams audio via `realtimeInput`
5. Server streams audio/text back via `serverContent`
6. Session ends via close frame or `goAway`

---

## Client Messages

Each message is a JSON object with exactly one of these top-level keys.

### `setup` — BidiGenerateContentSetup

Sent once, immediately after connecting. Cannot be changed mid-session.

```json
{
  "setup": {
    "model": "models/gemini-2.5-flash-native-audio-preview-12-2025",
    "systemInstruction": {
      "parts": [{ "text": "You are a helpful assistant." }]
    },
    "generationConfig": {
      "responseModalities": ["AUDIO"],
      "speechConfig": {
        "voiceConfig": {
          "prebuiltVoiceConfig": { "voiceName": "Fenrir" }
        }
      }
    },
    "realtimeInputConfig": {
      "automaticActivityDetection": {
        "disabled": false,
        "startOfSpeechSensitivity": "START_SENSITIVITY_HIGH",
        "endOfSpeechSensitivity": "END_SENSITIVITY_HIGH",
        "prefixPaddingMs": 20,
        "silenceDurationMs": 500
      },
      "activityHandling": "START_OF_ACTIVITY_INTERRUPTS",
      "turnCoverage": "TURN_INCLUDES_ONLY_ACTIVITY"
    },
    "inputAudioTranscription": {},
    "outputAudioTranscription": {}
  }
}
```

| Field | Type | Notes |
|---|---|---|
| `model` | string | Required. `models/{model_id}` |
| `systemInstruction` | Content | Text-only parts |
| `generationConfig` | GenerationConfig | See below |
| `realtimeInputConfig` | RealtimeInputConfig | VAD + turn behavior |
| `inputAudioTranscription` | AudioTranscriptionConfig | STT for user audio |
| `outputAudioTranscription` | AudioTranscriptionConfig | STT for model audio output |
| `tools` | Tool[] | Function calling definitions |
| `sessionResumption` | SessionResumptionConfig | Enable session tokens |
| `contextWindowCompression` | ContextWindowCompressionConfig | Auto-compress long contexts |
| `proactivity` | ProactivityConfig | Model declines irrelevant responses |

**GenerationConfig** — unsupported fields for Live API: `responseLogprobs`, `responseMimeType`, `logprobs`, `responseSchema`, `stopSequence`, `routingConfig`, `audioTimestamp`.

**Response modalities:** Only one at a time — `"AUDIO"` or `"TEXT"`, not both.

---

### `realtimeInput` — BidiGenerateContentRealtimeInput

Streams user audio (and optionally video/text) to the server.

```json
{
  "realtimeInput": {
    "audio": {
      "mimeType": "audio/pcm;rate=16000",
      "data": "<base64-encoded PCM>"
    }
  }
}
```

| Field | Type | Notes |
|---|---|---|
| `audio` | Blob | Preferred. Raw PCM audio chunk |
| `video` | Blob | Raw video frame |
| `text` | string | Realtime text input |
| `activityStart` | ActivityStart | Manual VAD only — marks start of speech |
| `activityEnd` | ActivityEnd | Manual VAD only — marks end of speech |
| `audioStreamEnd` | bool | Signal mic closed; send after ~1s silence with auto-VAD |
| `mediaChunks` | Blob[] | **Deprecated.** Use `audio` instead |

**Key distinction:**
- `activityEnd: {}` — for **manual VAD** (when `automaticActivityDetection.disabled = true`). Tells the server the user has finished speaking.
- `audioStreamEnd: true` — for **automatic VAD**. Signals the mic has been closed/paused. Server flushes any buffered audio.

With automatic VAD (the default), neither signal is strictly required — the server detects end-of-speech from silence. However, sending `audioStreamEnd` after sending all audio ensures the server flushes immediately rather than waiting for silence detection.

```json
{ "realtimeInput": { "audioStreamEnd": true } }
```

```json
{ "realtimeInput": { "activityEnd": {} } }
```

---

### `clientContent` — BidiGenerateContentClientContent

Sends conversation turns (text-based, non-streaming). Uses deterministic ordering — messages are added to context sequentially, unlike `realtimeInput` which is VAD-driven.

```json
{
  "clientContent": {
    "turns": [
      { "role": "user", "parts": [{ "text": "Hello" }] }
    ],
    "turnComplete": true
  }
}
```

| Field | Type | Notes |
|---|---|---|
| `turns` | Content[] | **Required** — must not be null or empty array |
| `turnComplete` | bool | True signals model should respond |

> **Important:** The server rejects `clientContent` if `turns` is `null` or `[]`. Always provide at least one turn with content. For pure audio sessions, use `realtimeInput` + `audioStreamEnd`/`activityEnd` instead.

---

### `toolResponse` — BidiGenerateContentToolResponse

Response to a `toolCall` from the server.

```json
{
  "toolResponse": {
    "functionResponses": [
      { "id": "call_abc123", "response": { "result": "done" } }
    ]
  }
}
```

---

## Server Messages

Each server message is a JSON object with one of these top-level keys.

### `setupComplete`

Sent once after the server processes the `setup` message. No fields.

```json
{ "setupComplete": {} }
```

---

### `serverContent` — BidiGenerateContentServerContent

Carries model output: audio chunks, transcriptions, and turn signals.

```json
{
  "serverContent": {
    "modelTurn": {
      "parts": [
        {
          "inlineData": {
            "mimeType": "audio/pcm;rate=24000",
            "data": "<base64 PCM>"
          }
        }
      ]
    },
    "generationComplete": true,
    "turnComplete": true,
    "interrupted": false,
    "inputTranscription": { "text": "what time is it" },
    "outputTranscription": { "text": "It's currently..." }
  }
}
```

| Field | Type | Notes |
|---|---|---|
| `modelTurn` | Content | Audio or text parts from the model |
| `generationComplete` | bool | Model finished generating audio for this response. **Primary signal to stop collecting audio.** |
| `turnComplete` | bool | Model finished its turn; awaiting more input |
| `interrupted` | bool | Client input interrupted ongoing generation — discard buffered audio and stop playback |
| `inputTranscription` | Transcription | STT of user audio (requires `inputAudioTranscription` in setup) |
| `outputTranscription` | Transcription | STT of model audio output (requires `outputAudioTranscription` in setup) |
| `groundingMetadata` | GroundingMetadata | Search grounding info |

**Turn signal sequence (normal flow):**
1. Multiple `serverContent` messages with `modelTurn` audio chunks (no boolean flags)
2. `serverContent` with `generationComplete: true` — generation finished, collect all audio
3. `serverContent` with `turnComplete: true` — model is done, ready for next input

**Interrupted flow:**
1. Client sends new audio while model is generating
2. Server sends `serverContent` with `interrupted: true`
3. Client must stop playback and discard any buffered audio
4. "Only the information already sent to the client is retained in session history"

---

### `toolCall`

Server requests the client execute one or more functions.

```json
{
  "toolCall": {
    "functionCalls": [
      { "id": "call_abc123", "name": "get_weather", "args": { "city": "NYC" } }
    ]
  }
}
```

Respond with `toolResponse` matching the `id`.

---

### `toolCallCancellation`

Server cancels pending tool calls (e.g., due to interruption).

```json
{
  "toolCallCancellation": {
    "ids": ["call_abc123"]
  }
}
```

Discard these calls; do not send responses for cancelled IDs.

---

### `goAway`

Server is about to close the connection.

```json
{
  "goAway": {
    "timeLeft": "5s"
  }
}
```

`timeLeft` is a duration string (e.g., `"5s"`). Save session state and reconnect before this elapses to avoid losing context.

---

### `sessionResumptionUpdate`

Issued periodically when `sessionResumption` is configured in setup.

```json
{
  "sessionResumptionUpdate": {
    "newHandle": "session_token_abc...",
    "resumable": true
  }
}
```

Store `newHandle` to resume the session on reconnect. `resumable` is `false` during function execution or active generation.

---

## Audio Format

| Direction | Format | Sample Rate | Encoding |
|---|---|---|---|
| Input (client → server) | Raw PCM | 16 kHz (native); other rates resampled automatically | 16-bit signed little-endian |
| Output (server → client) | Raw PCM | **Always 24 kHz** | 16-bit signed little-endian |

MIME type for input: `audio/pcm;rate=16000`
MIME type for output: `audio/pcm;rate=24000`

---

## Voice Activity Detection (VAD)

### Automatic VAD (default)

The server detects speech automatically. No `activityStart`/`activityEnd` needed.

```json
"realtimeInputConfig": {
  "automaticActivityDetection": {
    "disabled": false,
    "startOfSpeechSensitivity": "START_SENSITIVITY_HIGH",
    "endOfSpeechSensitivity": "END_SENSITIVITY_HIGH",
    "prefixPaddingMs": 20,
    "silenceDurationMs": 500
  }
}
```

After sending all audio, send `audioStreamEnd` to flush immediately:
```json
{ "realtimeInput": { "audioStreamEnd": true } }
```

### Manual VAD

Disable auto-detection and bracket audio with explicit signals:

```json
"automaticActivityDetection": { "disabled": true }
```

Then wrap audio with:
```json
{ "realtimeInput": { "activityStart": {} } }
// ... send audio chunks ...
{ "realtimeInput": { "activityEnd": {} } }
```

### Activity Handling

| Enum | Behavior |
|---|---|
| `START_OF_ACTIVITY_INTERRUPTS` (default) | New user speech interrupts ongoing generation |
| `NO_INTERRUPTION` | Generation completes even if user speaks |

### Turn Coverage

| Enum | Behavior |
|---|---|
| `TURN_INCLUDES_ONLY_ACTIVITY` (default) | Only VAD-detected speech is included in the turn |
| `TURN_INCLUDES_ALL_INPUT` | All audio (including silence) is included |

---

## Session Limits

| Limit | Value |
|---|---|
| Max session duration (audio only) | 15 minutes |
| Max session duration (audio + video) | 2 minutes |
| Context window — native audio models | 128k tokens |
| Context window — other Live API models | 32k tokens |
| Response modalities per session | 1 (AUDIO or TEXT, not both) |

---

## Available Models

| Model ID | Notes |
|---|---|
| `models/gemini-2.5-flash-native-audio-preview-12-2025` | Native audio output; thinking support; 128k context |
| `models/gemini-live-2.5-flash-preview` | Text-based audio; 32k context |

Set via `GEMINI_MODEL_ID` env var or in `setup.model`.

---

## Available Voices

Set via `GEMINI_AUDIO_VOICE` env var or in `setup.generationConfig.speechConfig.voiceConfig.prebuiltVoiceConfig.voiceName`.

| Voice | Character | Voice | Character |
|---|---|---|---|
| Zephyr | Bright | Aoede | Breezy |
| Puck | Upbeat | Leda | Youthful |
| Charon | Informative | Orus | Firm |
| Kore | Firm | Schedar | — |
| **Fenrir** | Friendly/Excitable | Gacrux | Mature |
| Laomedeia | Upbeat | Pulcherrima | Forward |
| Enceladus | Breezy | Achird | Friendly |
| Iapetus | Clear | Zubenelgenubi | Casual |
| Umbriel | Easy-going | Vindemiatrix | Gentle |
| Achernar | Soft | Sadachbia | Lively |
| Rasalgethi | Knowledgeable | Sadaltager | Knowledgeable |
| Algieba | Smooth | Sulafat | Warm |
| Despina | Smooth | Alnilam | Firm |
| Erinome | Clear | Algenib | Gravelly |
| Autonoe | Bright | Callirrhoe | Easy-going |

Default in this app: **Fenrir**

---

## Error Codes

| HTTP / Close Code | Meaning |
|---|---|
| 400 / `Invalid` | Malformed message (e.g., `clientContent` with null/empty `turns`) |
| 401 | Invalid or missing API key |
| 429 | Rate limit / quota exceeded — check `Retry-After` header |
| 503 | Server overloaded — retry with backoff |

---

## Implementation Notes

### What we currently use

```
realtimeInput.audio                    — PCM audio chunks (preferred, not deprecated mediaChunks)
realtimeInput.activityStart/activityEnd — manual VAD bracketing (auto-VAD disabled in setup)
realtimeInput.audioStreamEnd           — one-shot mode only: flush after full file sent
serverContent.generationComplete       — primary break condition for receive loop
serverContent.turnComplete             — secondary break condition
serverContent.interrupted              — stop playback, keep received transcript, go mic-live
serverContent.inputTranscription       — streaming STT of user audio (display only)
serverContent.outputTranscription      — STT of Gemini's spoken audio (VOICE_TEXT)
modelTurn.parts[].text + thought:true  — internal thinking tokens
modelTurn.parts[].text + thought:false — text content (rare in AUDIO mode)
```

### VAD strategy

Auto-VAD is **disabled** (`automaticActivityDetection.disabled: true`). Manual bracketing with `activityStart` / `activityEnd` is used instead. This prevents the server's VAD from firing on brief mid-sentence pauses in pre-recorded or live audio, which would trigger early generation and cause `interrupted`.

### What we do NOT yet use

- `sessionResumption` — see Persistent Session section below
- `contextWindowCompression` — see Persistent Session section below
- `goAway` — see Persistent Session section below
- `toolCall` / `toolResponse` — function calling (out of scope)
- `clientContent` — text-based turn injection (not needed for audio sessions)

---

## Privacy and Microphone Security Commitment

**This section is non-negotiable and takes precedence over all convenience or architectural decisions.**

### The commitment

When the application displays the microphone as **Off** (Idle, Processing, NotConnected, or any state other than Listening), **zero bytes of microphone audio are transmitted to Google or any external service.** This is not a best-effort goal — it is a hard guarantee enforced in code.

### Why this matters

In persistent session mode, the WebSocket connection to Google remains open across multiple exchanges. An open WebSocket does not mean audio is flowing — but a user cannot verify that from the outside. They are trusting the application's stated mic state completely. That trust must be earned through explicit, auditable code behavior, not just intent.

### How the guarantee is enforced

1. **PulseAudio capture is the only source of microphone audio.** The `Recorder` struct holds the OS-level capture handle. When `recorder.stop()` is called, the PulseAudio stream is closed at the OS level. No further audio can be read from hardware regardless of what the rest of the application does.

2. **`activityStart` is sent only immediately before starting `Recorder`.** The sequence is strictly: call `recorder = start_recording()` → confirm capture started → send `activityStart`. Never the reverse.

3. **`activityEnd` is sent only after `recorder.stop()` completes.** The sequence is strictly: call `recorder.stop()` (blocks until thread joins and WAV is finalized) → send `activityEnd`. This ensures the server receives the end-of-activity signal only after the hardware capture is confirmed stopped.

4. **No audio buffering across state transitions.** Audio captured during Listening is not buffered anywhere that could be accidentally re-sent in a later turn. Each recording is discrete.

5. **The WebSocket being open carries no audio unless the Recorder is actively running.** An open connection in Idle state sends only non-audio protocol frames (ping/pong, session resumption updates). It is explicitly verified in code that no `realtimeInput.audio` frames are sent outside of the Listening state.

### What "Mic Off" means in this application

```
AppState::Idle         → Recorder stopped, no audio frames sent
AppState::NotConnected → Recorder stopped, no audio frames sent
AppState::Processing   → Recorder stopped, no audio frames sent
AppState::Reconnecting → Recorder stopped, no audio frames sent
AppState::Error(_)     → Recorder stopped, no audio frames sent
AppState::Listening    → Recorder running, audio frames sent — MIC IS ON
```

The UI reflects this with a clear visual indicator. The "Listening" state must be unambiguously distinct from all other states.

### Code review requirement

Any change to the audio send path, state machine transitions, or Recorder lifecycle must be reviewed against these guarantees. If a refactor could cause audio to be sent in a non-Listening state, it must be rejected regardless of other benefits.

---

## Persistent Session Design (Planned)

### The problem with one-shot mode

The current TUI creates a new WebSocket connection per recording. Each exchange starts cold — Gemini has no memory of what was said 30 seconds ago. This is fine for single queries but prevents natural multi-turn conversation.

### Persistent session architecture

```
App starts
  → user configures voice (Tab), voice unlocked
  → user presses Space for first time
    → connect WebSocket with sessionResumption + contextWindowCompression in setup
    → send setup, wait for setupComplete
    → send activityStart, begin streaming mic chunks live
  → user presses Space again
    → send activityEnd, stop mic, keep connection OPEN
    → receive response (concurrent with mic being off)
    → play audio on background thread
  → user presses Space again (next turn)
    → send activityStart (same connection, context preserved)
    → ...
  → goAway received
    → store latest session handle
    → reconnect transparently with handle (context preserved)
  → user quits
    → store final handle for optional manual resume
```

### Session resumption

Configure in setup:
```json
{ "setup": { "sessionResumption": { "handle": "<previous_handle_or_omit_for_new>" } } }
```

Server sends `sessionResumptionUpdate` messages periodically:
```json
{ "sessionResumptionUpdate": { "newHandle": "token_abc...", "resumable": true } }
```

- Always store the latest `newHandle` (old ones become invalid).
- `resumable: false` during generation or function calls — do not attempt reconnect at these moments.
- **Handles are valid for 2 hours after session termination.**
- What is preserved: full conversation context (subject to compression below).
- On `goAway`: save handle, reconnect before `timeLeft` elapses.

### Context window compression

Configure in setup to prevent hitting the 128k token limit mid-conversation:
```json
{ "setup": { "contextWindowCompression": { "slidingWindow": {} } } }
```

- Triggers automatically at 80% of context window (≈102k tokens for native audio model).
- Drops oldest content while preserving system instruction.
- Combined with session resumption → effectively unlimited conversation duration.
- Default `targetTokens` is `triggerTokens / 2`, retaining roughly 50% of the window.

### goAway handling

```json
{ "goAway": { "timeLeft": "5s" } }
```

- Server sends before hard disconnect. `timeLeft` will always be ≥ some model-specific minimum.
- Client must save the latest handle and reconnect within `timeLeft` to avoid losing context.
- In practice: reconnect happens in the background, user sees `Reconnecting...` in TUI footer briefly.

### Connection lifecycle and server resets

- Server hard-resets connections roughly every **10 minutes** regardless of activity.
- The 15-minute audio-only session limit is separate from this.
- Session resumption handles both: always reconnect with stored handle.
- With compression + resumption, sessions are effectively unbounded.

### Voice locking

Voice is set in the `setup` message and cannot change mid-session. Design decision:
- Voice Tab works **before first connection** (startup configuration) and after a **full disconnect**.
- Voice is greyed out (dark gray) and Tab is disabled while connected.
- Reconnecting on `goAway` reuses the same voice — no UI change needed.
- To change voice: quit and restart, or disconnect explicitly (future feature).

### Interrupt / barge-in behavior

When user presses Space while Gemini is playing audio:
1. Stop PulseAudio playback immediately (cancel playback thread).
2. Send `activityStart` — this signals new user speech to the server.
3. State transitions to `Listening`.
4. Server sends `interrupted: true` — we keep any transcript already received, discard unplayed audio.
5. Gemini waits for the new `activityEnd` before generating a response.

This is `START_OF_ACTIVITY_INTERRUPTS` behavior (the default `activityHandling` value).

### TUI state machine (persistent mode)

```
NotConnected  — startup, Tab works, voice configurable
Connecting    — first Space pressed, WebSocket handshake in progress
Idle          — connected and waiting, voice locked (greyed)
Listening     — mic live, streaming chunks, activityStart sent
Processing    — activityEnd sent, Gemini generating + playing
Reconnecting  — goAway received, transparently reconnecting with handle
Error(String) — unrecoverable error, Space to dismiss and attempt reconnect
```

### One-shot mode (--one-shot flag)

Preserves current behavior for quick queries and file processing:
- New WebSocket connection per exchange.
- No session resumption, no context across exchanges.
- Still uses manual VAD (activityStart → audio → activityEnd).
- `--input <file>` always implies one-shot.

### Session info display (TUI header)

Header splits into left (title) and right (session metadata):
```
| Gemini Audio Live              Conversation: 1234567890  Session: abc12... |
```

- **Conversation ID**: Unix timestamp of session start (stable for the session lifetime).
- **Session handle**: Last 8 chars of the resumption token (truncated for display).
- Both update on reconnect if handle changes.

---

## Sources

- [Live API WebSocket Reference](https://ai.google.dev/api/live)
- [Live API Capabilities Guide](https://ai.google.dev/gemini-api/docs/live-guide)
- [Configure Language and Voice (Vertex AI)](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/live-api/configure-language-voice)
- [Gemini Live API Overview (Vertex AI)](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/live-api)
