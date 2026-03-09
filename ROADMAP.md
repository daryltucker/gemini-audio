# Gemini Audio Roadmap

This document outlines the planned future features, enhancements, and structural changes for `gemini-audio`. It represents the vision for what the project will become.

## Phase 1: Stability and Core Experience (Current/Short-term)
Focusing on cementing the foundational real-time voice experience.
- [x] Persistent WebSockets for conversational sessions
- [x] SQLite session tracking
- [x] Intelligent network retry logic
- [x] Fix edge cases where user transcriptions arrive early (mid-stream)
- [ ] Improve error messages for invalid configurations
- [ ] Stabilize TUI UX for unexpected disconnection states

## Phase 2: Tool Calling & System Integrations (Mid-term)
Capitalizing on the Gemini Live API's ability to seamlessly bridge voice input with programmatic actions.
- **Calendar Integration:** Passing tool schemas to the model allowing it to fetch, summarize, and schedule calendar events in real time.
- **System Commands:** Giving the model limited, explicit capabilities to execute local system commands (e.g., controlling a smart home via local APIs, setting timers).
- **Tool-calling UI:** Enhancing the TUI to visually denote when the assistant is invoking a tool versus generating speech.

## Phase 3: Full Multimodal Streaming (Long-term)
Expanding beyond pure audio to true multimodal interaction.
- **Screen/Camera Capture:** Streaming video frames or terminal screen captures alongside audio in real time via `BidiGenerateContentRealtimeInput`.
- **Multimodal Context:** Allowing the model to see what is on the user's screen and answer questions about it verbally.
- **Streaming Optimizations:** Moving to lower-level AV integration if PulseAudio + Symphonia proves insufficient for synchronized AV streams.
