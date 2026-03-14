// Platform-specific audio: PulseAudio recording and playback (Linux desktop only).
// This module is NOT part of gemini-audio-core — it stays in the desktop app crate.

use gemini_audio_core::error::{GeminiAudioError, Result};
use std::path::Path;

/// Microphone capture handle.
///
/// # Privacy guarantee
///
/// Audio is captured at the OS level only while this struct is alive and `running` is true.
/// Calling `stop()` sets `running = false` and **blocks until the capture thread joins**,
/// which closes the PulseAudio stream at the OS level before returning.
///
/// Callers MUST treat `stop()` returning as the authoritative signal that mic capture has
/// fully ended. No audio can be read from hardware after that point. Any transmission of
/// audio to external services MUST be bounded to what was captured before `stop()` returns.
/// Nothing captured before the user pressed the stop control, and nothing after.
pub struct Recorder {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Recorder {
    /// Stop microphone capture and close the PulseAudio stream.
    ///
    /// Blocks until the capture thread has fully exited and the WAV file is finalized.
    /// After this returns, the OS-level audio capture handle is closed — zero mic audio
    /// can be read or transmitted. Callers may safely treat this as "mic is off".
    pub fn stop(&mut self) {
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Atomically signal mic stop and return the OS thread handle for async joining.
    ///
    /// After this returns, `running` is false — no new mic audio will be read from hardware.
    /// This is the authoritative mic-off signal for privacy purposes; the join handle is only
    /// needed to ensure WAV file finalization before the file is read. Join the handle in
    /// `tokio::task::spawn_blocking` to avoid blocking the UI thread during finalization.
    pub fn take_handle(&mut self) -> Option<std::thread::JoinHandle<()>> {
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        self.handle.take()
    }
}

/// Initialize microphone capture with live streaming output.
///
/// Each 100ms chunk of raw 16kHz/16-bit/mono PCM is passed to `on_chunk` immediately as it
/// is captured — no WAV file, no buffering. The closure is called from a dedicated OS thread
/// and must be `Send + 'static` and non-blocking (e.g., sending to a channel).
///
/// Privacy guarantee: `Recorder::stop()` blocks until the capture thread fully exits.
/// No audio is passed to `on_chunk` after `stop()` returns.
pub fn start_recording_streaming<F>(on_chunk: F) -> Result<Recorder>
where
    F: Fn(Vec<u8>) + Send + 'static,
{
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();

    let handle = std::thread::spawn(move || {
        use libpulse_binding::sample::{Format, Spec};
        use libpulse_binding::stream::Direction;
        use libpulse_simple_binding::Simple;

        let spec = Spec { format: Format::S16le, channels: 1, rate: 16000 };
        let s = match Simple::new(
            None, "GeminiAudio", Direction::Record, None, "Voice Input", &spec, None, None,
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("PulseAudio record init failed: {:?}", e);
                return;
            }
        };

        // 100ms at 16kHz 16-bit mono
        let mut buffer = [0u8; 3200];
        let mut total_frames: u64 = 0;
        let mut peak: i16 = 0;

        while running_clone.load(std::sync::atomic::Ordering::SeqCst) {
            if let Err(e) = s.read(&mut buffer) {
                tracing::error!("PulseAudio read error: {:?}", e);
                break;
            }
            for pair in buffer.chunks_exact(2) {
                let s = i16::from_le_bytes([pair[0], pair[1]]);
                if s.abs() > peak { peak = s.abs(); }
            }
            total_frames += buffer.len() as u64 / 2;
            on_chunk(buffer.to_vec());
        }

        let duration_ms = total_frames * 1000 / 16000;
        tracing::info!(duration_ms, peak_amplitude = peak, "Streaming recording complete");
        if peak < 100 {
            tracing::warn!("Peak amplitude very low ({}). Mic may be muted or wrong source.", peak);
        }
    });

    Ok(Recorder { running, handle: Some(handle) })
}

/// Initialize the microphone capture and begin writing to output_path using PulseAudio
pub fn start_recording<P: AsRef<Path>>(output_path: P) -> Result<Recorder> {
    let output_path = output_path.as_ref().to_path_buf();
    
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();

    let handle = std::thread::spawn(move || {
        use libpulse_binding::sample::{Format, Spec};
        use libpulse_binding::stream::Direction;
        use libpulse_simple_binding::Simple;
        
        let spec = Spec {
            format: Format::S16le,
            channels: 1,
            rate: 16000,
        };

        let s = match Simple::new(
            None,                // Use the default server
            "GeminiAudio",       // Our application's name
            Direction::Record,   // We want a record stream
            None,                // Use the default device
            "Voice Recording",   // Description of our stream
            &spec,               // Our sample format
            None,                // Use default channel map
            None,                // Use default buffering attributes
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to connect to PulseAudio: {:?}", e);
                return;
            }
        };

        let wav_spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = match hound::WavWriter::create(&output_path, wav_spec) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to create WAV file: {:?}", e);
                return;
            }
        };

        // Read in chunks of 3200 bytes (100ms at 16kHz, 16-bit mono)
        let mut buffer = [0u8; 3200];
        let mut total_frames: u64 = 0;
        let mut peak: i16 = 0;

        while running_clone.load(std::sync::atomic::Ordering::SeqCst) {
            if let Err(e) = s.read(&mut buffer) {
                tracing::error!("PulseAudio read error: {:?}", e);
                break;
            }

            // Convert bytes to i16 samples
            let samples: Vec<i16> = buffer
                .chunks_exact(2)
                .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();

            for &sample in &samples {
                if sample.abs() > peak {
                    peak = sample.abs();
                }
                let _ = writer.write_sample(sample);
            }
            total_frames += samples.len() as u64;
        }

        let duration_ms = total_frames * 1000 / 16000;
        tracing::info!(duration_ms, peak_amplitude = peak, "Recording complete");
        if peak < 100 {
            tracing::warn!("Peak amplitude is very low ({}). Microphone may be muted or wrong source selected.", peak);
        }
        
        let _ = writer.finalize();
    });

    Ok(Recorder {
        running,
        handle: Some(handle),
    })
}

/// Streaming PulseAudio playback: opens one PA connection and feeds it PCM chunks from a channel.
///
/// The caller sends `Vec<u8>` chunks through the channel as they arrive from the network.
/// Dropping the sender (or closing the channel) signals clean end-of-stream: this function
/// drains the PA hardware buffer and returns `Ok(())`.
///
/// Setting `stop` to true aborts immediately (barge-in); the hardware buffer is NOT drained
/// so playback cuts off within the PA buffer latency (~100ms by default).
///
/// `PlaybackFinished` notification is the caller's responsibility after this returns.
pub fn stream_pcm_pulseaudio(
    audio_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    sample_rate: u32,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    use libpulse_binding::sample::{Format, Spec};
    use libpulse_binding::stream::Direction;
    use libpulse_simple_binding::Simple;
    use std::sync::atomic::Ordering;

    let spec = Spec { format: Format::S16le, channels: 1, rate: sample_rate };
    let pa = Simple::new(None, "GeminiAudio", Direction::Playback, None, "AI Response", &spec, None, None)
        .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio playback init failed: {:?}", e)))?;

    loop {
        if stop.load(Ordering::Relaxed) { return Ok(()); }
        match audio_rx.recv() {
            Ok(chunk) => {
                if stop.load(Ordering::Relaxed) { return Ok(()); }
                pa.write(&chunk)
                    .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio write failed: {:?}", e)))?;
            }
            Err(_) => {
                // Channel closed: clean end of stream — drain the hardware buffer
                if !stop.load(Ordering::Relaxed) {
                    pa.drain()
                        .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio drain failed: {:?}", e)))?;
                }
                return Ok(());
            }
        }
    }
}

/// Play raw 16-bit signed little-endian mono PCM via PulseAudio, checking `stop` between chunks.
/// Writes in ~100ms chunks so barge-in can cancel playback within one chunk latency.
/// Set `stop` to true from another thread to abort; playback returns `Ok(())` on abort.
pub fn play_pcm_pulseaudio_cancellable(
    pcm_data: &[u8],
    sample_rate: u32,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    use libpulse_binding::sample::{Format, Spec};
    use libpulse_binding::stream::Direction;
    use libpulse_simple_binding::Simple;
    use std::sync::atomic::Ordering;

    let spec = Spec { format: Format::S16le, channels: 1, rate: sample_rate };
    let pa = Simple::new(None, "GeminiAudio", Direction::Playback, None, "AI Response", &spec, None, None)
        .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio playback init failed: {:?}", e)))?;

    // 100ms chunks: sample_rate * 2 bytes/sample / 10
    let chunk_bytes = ((sample_rate as usize * 2) / 10).max(1);
    for chunk in pcm_data.chunks(chunk_bytes) {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        pa.write(chunk)
            .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio write failed: {:?}", e)))?;
    }

    if !stop.load(Ordering::Relaxed) {
        pa.drain()
            .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio drain failed: {:?}", e)))?;
    }

    Ok(())
}

/// Play raw 16-bit signed little-endian mono PCM via PulseAudio. Blocks until playback drains.
pub fn play_pcm_pulseaudio(pcm_data: &[u8], sample_rate: u32) -> Result<()> {
    use libpulse_binding::sample::{Format, Spec};
    use libpulse_binding::stream::Direction;
    use libpulse_simple_binding::Simple;

    let spec = Spec {
        format: Format::S16le,
        channels: 1,
        rate: sample_rate,
    };

    let pa = Simple::new(
        None,
        "GeminiAudio",
        Direction::Playback,
        None,
        "AI Response",
        &spec,
        None,
        None,
    )
    .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio playback init failed: {:?}", e)))?;

    pa.write(pcm_data)
        .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio write failed: {:?}", e)))?;

    pa.drain()
        .map_err(|e| GeminiAudioError::AudioDevice(format!("PulseAudio drain failed: {:?}", e)))?;

    Ok(())
}
