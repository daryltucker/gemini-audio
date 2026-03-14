# gemini-audio

Real-time voice conversation powered by [Gemini 2.5 Flash Native Audio](https://ai.google.dev/gemini-api/docs/live). Speak, listen, respond — low-latency, persistent session, terminal-native.

Built in Rust. Runs on Linux. No Electron, no browser, no Python runtime.

---

## Install

**Pre-built binary (Linux x86\_64 / aarch64):**

```bash
cargo binstall gemini-audio
```

**From source:**

> Requires libpulse-dev

```bash
sudo apt install libpulse-dev          # Debian/Ubuntu/Pop!_OS
sudo dnf install pulseaudio-libs-devel # Fedora

cargo install --git https://github.com/daryltucker/gemini-audio
```

**pre-built:**

```bash
cargo binstall --git https://github.com/daryltucker/gemini-audio
```

---

## Get an API Key

gemini-audio uses the [Gemini Live API](https://ai.google.dev/gemini-api/docs/live). You need a key from one of:

- **[Google AI Studio](https://aistudio.google.com/)** — free tier available, instant setup, no billing required to get started.
- **Google Cloud Vertex AI** — for production workloads and enterprise billing. Enable the Gemini API in the [Cloud Console](https://console.cloud.google.com/vertex-ai).

```bash
export GEMINI_API_KEY=your_key_here
gemini-audio
```

A `.env` file in the working directory is loaded automatically.

---

## Use Cases

`gemini-audio` is designed for scenarios where typing is inconvenient or slow, but human-like conversational fluidity is required:
*   **Hands-free Ideation:** Brainstorming architecture, creative writing, or problem-solving while walking or away from the keyboard.
*   **Language Practice:** Conversing with a highly responsive AI that replies instantly via voice, mimicking a real conversation partner.
*   **Accessibility:** Providing a pure voice interface to an LLM without relying on bloated web interfaces or browsers.

---

## Usage

```bash
gemini-audio          # Launch TUI (interactive voice mode)
gemini-audio --help   # Full options
```

### TUI Controls

| Key | Action |
|-----|--------|
| `Space` | Start recording |
| `Space` (again) | Stop and send to Gemini |
| `Space` (during response) | Barge in — interrupt and speak |
| `Tab` / `Shift-Tab` | Cycle through voices |
| `c` | Cancel current recording |
| `q` / `Esc` | Quit |

Voice selection is available before the first exchange. Once a session is connected, voice is locked until you restart (a Gemini Live API constraint).

### Single File Mode

```bash
gemini-audio --input audio.ogg
gemini-audio --input audio.ogg --output response.wav --no-audio-playback
```

Output defaults to `<input>.output.wav`. Accepts any format symphonia can decode (OGG, MP3, FLAC, WAV, M4A, and more).

---

## Prompts (System Instructions)

gemini-audio uses a markdown file as the system prompt sent to Gemini at session start. On first launch, a `default` prompt is created automatically at:

```
~/.config/gemini-audio/prompts/default.md
```

Edit it to change the assistant's persona and behavior. To use a different prompt:

```bash
gemini-audio --prompt coach       # loads ~/.config/gemini-audio/prompts/coach.md
```

Prompts in `./prompts/` (relative to cwd) work as a fallback, useful during development.

---

## Configuration

| Variable | Description |
|----------|-------------|
| `GEMINI_API_KEY` | Gemini API key (**required**) |
| `GEMINI_AUDIO_VOICE` | Default voice on startup (e.g. `Fenrir`). Tab overrides per-session. |
| `GEMINI_AUDIO_SAVE_RECORDINGS` | Set to `1` to save input + output WAV files |
| `GEMINI_MODEL_ID` | Override model ID (default: `gemini-2.5-flash-native-audio-preview-12-2025`) |
| `GEMINI_WS_ENDPOINT` | Override WebSocket endpoint |

### Saving Recordings

```bash
GEMINI_AUDIO_SAVE_RECORDINGS=1 gemini-audio
```

Saved to `~/.local/share/gemini-audio/recordings/session_<timestamp>_{input,output}.wav`.
Conversation transcripts (JSONL) are always written to `~/.local/share/gemini-audio/conversations/`.

---

## Voices

30 voices available. Cycle with `Tab` in the TUI, or set `GEMINI_AUDIO_VOICE` to any of:

`Fenrir` `Puck` `Charon` `Kore` `Aoede` `Leda` `Orus` `Zephyr` `Laomedeia` `Enceladus`
`Iapetus` `Umbriel` `Achernar` `Rasalgethi` `Algieba` `Despina` `Erinome` `Autonoe`
`Callirrhoe` `Achird` `Zubenelgenubi` `Vindemiatrix` `Sadachbia` `Sulafat` `Gacrux`
`Pulcherrima` `Algenib` `Alnilam` `Sadaltager` `Schedar`

> Currently hard-coded due to lack of API support.

---

## Technical Details

### Architecture

- **Persistent session** (default): single WebSocket connection with `sessionResumption` and `contextWindowCompression`. Context accumulates across exchanges. Transparent reconnect on `goAway`.
- **One-shot mode** (`--one-shot`): fresh connection per utterance, no context.

### WebSocket Flow

```
connect → BidiGenerateContent setup → wait setupComplete
→ activityStart → stream PCM chunks (16kHz/16-bit/mono) → activityEnd
→ receive audio (24kHz) + transcripts until turnComplete
→ [persistent: wait for next Space] [one-shot: disconnect]
```

VAD is **manual** — `activityStart`/`activityEnd` bracket each utterance explicitly, preventing the server from cutting off mid-sentence pauses.

### Audio

- **Input to API**: 16 kHz / 16-bit signed / mono PCM
- **Output from API**: 24 kHz / 16-bit signed / mono PCM
- **Mic capture**: PulseAudio at 16 kHz, streamed live as it's recorded
- **Playback**: PulseAudio, streamed as chunks arrive from the API
- Audio decoding via [symphonia](https://github.com/pdeljanov/Symphonia) + [rubato](https://github.com/HEnquist/rubato) — no ffmpeg required

### Privacy

When the mic indicator shows **Off**, zero bytes of audio are sent to Google. This is enforced at the code level: `recorder.stop()` fully joins the capture thread before `activityEnd` is sent. An open WebSocket does not mean audio is flowing.

### Data Storage

| Path | Contents |
|------|----------|
| `~/.config/gemini-audio/prompts/` | User prompts (system instructions) |
| `~/.local/share/gemini-audio/gemini-audio.db` | SQLite session database |
| `~/.local/share/gemini-audio/logs/gemini-audio.log` | Structured JSON logs |
| `~/.local/share/gemini-audio/conversations/` | Per-session JSONL transcripts |
| `~/.local/share/gemini-audio/recordings/` | Saved WAV files (opt-in) |

---

## Building from Source

### Desktop (Linux)

```bash
git clone https://github.com/daryltucker/gemini-audio
cd gemini-audio
sudo apt install libpulse-dev
cargo build --release -p gemini-audio
cargo test --workspace
```

```bash
# Run with your key
GEMINI_API_KEY=your_key cargo run --release -p gemini-audio
```

### Android

> Requires: Rust nightly/stable, Android NDK 27+, JDK 17+

**One-time setup:**

```bash
# 1. Rust Android target + cargo-ndk
rustup target add aarch64-linux-android
cargo install cargo-ndk

# 2. Android SDK + NDK (if not already installed)
mkdir -p ~/src/android-sdk-linux/cmdline-tools
cd /tmp
wget https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
unzip commandlinetools-linux-11076708_latest.zip
mv cmdline-tools ~/src/android-sdk-linux/cmdline-tools/latest

~/src/android-sdk-linux/cmdline-tools/latest/bin/sdkmanager \
  --sdk_root=$HOME/src/android-sdk-linux \
  "ndk;27.2.12479018" "platforms;android-35" "build-tools;35.0.0"

# 3. Environment variables (add to ~/.bashrc or ~/.profile)
export ANDROID_HOME=$HOME/src/android-sdk-linux
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
```

**Build & install:**

```bash
make android-build     # Cross-compile Rust core + build APK
make android-install   # Build + adb install to connected device
```

**Setting up your API key:**

1. Open the app on your phone
2. Go to **Settings** (gear icon)
3. Tap the **API Key** field and paste your key
4. That's it! The key is stored securely on your device

You can get a free API key from [Google AI Studio](https://aistudio.google.com/).

**Quick transfer via QR code:**

On your host machine (where `GEMINI_API_KEY` is set):

```bash
qrencode -t UTF8 "$GEMINI_API_KEY"
```

Scan the QR code with your phone, then paste the copied value into the API Key field.

---

*gemini-audio is an independent open-source project. It is not affiliated with or endorsed by Google. It uses the Google Gemini API under Google's standard [Terms of Service](https://ai.google.dev/gemini-api/terms).*
