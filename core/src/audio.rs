// Audio processing and format conversion — portable parts only.
// Platform-specific recording/playback (PulseAudio, Oboe, etc.) lives in the
// consumer crate (app/ for desktop, android/ for mobile).

use crate::error::{GeminiAudioError, Result};
use crate::config::{INPUT_SAMPLE_RATE, AUDIO_CHANNELS, AUDIO_BIT_DEPTH};
use hound::{WavSpec, WavWriter};
use std::path::Path;

/// Supported audio input formats
#[derive(Debug, Clone, PartialEq)]
pub enum AudioFormat {
    Mp3,
    Ogg,
    Flac,
    Wav,
    M4a,
    Webm,
    Mkv,
    Mp4,
    Unknown,
}

impl AudioFormat {
    pub fn from_extension(extension: &str) -> Self {
        match extension.to_lowercase().as_str() {
            "mp3" => AudioFormat::Mp3,
            "ogg" => AudioFormat::Ogg,
            "flac" => AudioFormat::Flac,
            "wav" => AudioFormat::Wav,
            "m4a" => AudioFormat::M4a,
            "webm" => AudioFormat::Webm,
            "mkv" => AudioFormat::Mkv,
            "mp4" => AudioFormat::Mp4,
            _ => AudioFormat::Unknown,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::M4a => "m4a",
            AudioFormat::Webm => "webm",
            AudioFormat::Mkv => "mkv",
            AudioFormat::Mp4 => "mp4",
            AudioFormat::Unknown => "unknown",
        }
    }
}

/// Audio file information
#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
    pub file_size: u64,
}

/// Decode any supported audio file (OGG, MP3, FLAC, WAV, M4A, etc.) to raw 16kHz mono PCM bytes.
/// Uses symphonia for decoding and rubato for resampling. No external tools required.
pub fn decode_to_pcm_16k<P: AsRef<Path>>(input_path: P) -> Result<Vec<u8>> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as SymphoniaError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction, Resampler};

    let path = input_path.as_ref();
    let src = std::fs::File::open(path)
        .map_err(|e| GeminiAudioError::FileIO(format!("Failed to open {}: {}", path.display(), e)))?;

    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| GeminiAudioError::AudioConversion(format!("Unsupported audio format: {}", e)))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| GeminiAudioError::AudioConversion("No audio track found".to_string()))?;

    let source_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| GeminiAudioError::AudioConversion("Unknown sample rate".to_string()))?;
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| GeminiAudioError::AudioConversion(format!("Failed to create decoder: {}", e)))?;

    // Decode all packets, downmix to mono f32
    let mut mono_f32: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(GeminiAudioError::AudioConversion(format!("Read error: {}", e))),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let n_ch = spec.channels.count();
                let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                sample_buf.copy_interleaved_ref(decoded);
                for frame in sample_buf.samples().chunks(n_ch) {
                    mono_f32.push(frame.iter().sum::<f32>() / n_ch as f32);
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(GeminiAudioError::AudioConversion(format!("Decode error: {}", e))),
        }
    }

    if mono_f32.is_empty() {
        return Err(GeminiAudioError::AudioConversion("No audio data decoded".to_string()));
    }

    // Resample to INPUT_SAMPLE_RATE (16 kHz) if necessary
    let resampled = if source_rate != INPUT_SAMPLE_RATE {
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        let chunk_size: usize = 1024;
        let ratio = INPUT_SAMPLE_RATE as f64 / source_rate as f64;

        let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk_size, 1)
            .map_err(|e| GeminiAudioError::AudioConversion(format!("Resampler init failed: {}", e)))?;

        let mut out_f32: Vec<f32> = Vec::new();
        let mut pos = 0;

        while pos < mono_f32.len() {
            let end = (pos + chunk_size).min(mono_f32.len());
            let actual_len = end - pos;
            let mut chunk = mono_f32[pos..end].to_vec();
            if chunk.len() < chunk_size {
                chunk.resize(chunk_size, 0.0);
            }

            let out = resampler.process(&[chunk], None)
                .map_err(|e| GeminiAudioError::AudioConversion(format!("Resample error: {}", e)))?;

            if actual_len < chunk_size {
                // Last partial chunk: trim to expected number of output frames
                let expected = (actual_len as f64 * ratio).ceil() as usize;
                out_f32.extend_from_slice(&out[0][..expected.min(out[0].len())]);
            } else {
                out_f32.extend_from_slice(&out[0]);
            }
            pos = end;
        }
        out_f32
    } else {
        mono_f32
    };

    // Convert f32 [-1.0, 1.0] to i16 little-endian PCM bytes
    let pcm_bytes: Vec<u8> = resampled
        .iter()
        .flat_map(|&s| {
            let s_i16 = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            s_i16.to_le_bytes()
        })
        .collect();

    Ok(pcm_bytes)
}

/// Detect audio format from file extension
pub fn detect_audio_format<P: AsRef<Path>>(path: P) -> Result<AudioFormat> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("unknown");
    
    Ok(AudioFormat::from_extension(extension))
}

/// Get WAV file information
pub fn get_wav_info<P: AsRef<Path>>(path: P) -> Result<AudioInfo> {
    let path = path.as_ref();
    
    let reader = hound::WavReader::open(path)
        .map_err(|e| GeminiAudioError::AudioConversion(format!("Failed to open WAV file: {}", e)))?;
    
    let spec = reader.spec();
    let duration = reader.duration() as f64 / spec.sample_rate as f64;
    let file_size = std::fs::metadata(path)
        .map_err(|e| GeminiAudioError::FileIO(format!("Failed to get file metadata: {}", e)))?
        .len();

    Ok(AudioInfo {
        format: AudioFormat::Wav,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        duration_secs: duration,
        file_size,
    })
}

/// Read PCM data from WAV file
pub fn read_wav_pcm<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let path = path.as_ref();
    
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| GeminiAudioError::AudioConversion(format!("Failed to open WAV file: {}", e)))?;
    
    let samples: Vec<i16> = reader
        .samples()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| GeminiAudioError::AudioConversion(format!("Failed to read samples: {}", e)))?;
    
    // Convert to bytes (little-endian)
    let pcm_data: Vec<u8> = samples
        .iter()
        .flat_map(|&sample| sample.to_le_bytes())
        .collect();
    
    Ok(pcm_data)
}

/// Write PCM data to WAV file
pub fn write_wav_pcm<P: AsRef<Path>>(path: P, pcm_data: &[u8], sample_rate: u32) -> Result<()> {
    let path = path.as_ref();
    
    let spec = WavSpec {
        channels: AUDIO_CHANNELS,
        sample_rate,
        bits_per_sample: AUDIO_BIT_DEPTH,
        sample_format: hound::SampleFormat::Int,
    };
    
    let mut writer = WavWriter::create(path, spec)
        .map_err(|e| GeminiAudioError::AudioConversion(format!("Failed to create WAV file: {}", e)))?;
    
    // Convert bytes back to samples
    let samples: Vec<i16> = pcm_data
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    
    // Write samples
    for sample in samples {
        writer.write_sample(sample)
            .map_err(|e| GeminiAudioError::AudioConversion(format!("Failed to write sample: {}", e)))?;
    }
    
    writer.finalize()
        .map_err(|e| GeminiAudioError::AudioConversion(format!("Failed to finalize WAV file: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_format_detection() {
        assert_eq!(AudioFormat::from_extension("mp3"), AudioFormat::Mp3);
        assert_eq!(AudioFormat::from_extension("ogg"), AudioFormat::Ogg);
        assert_eq!(AudioFormat::from_extension("wav"), AudioFormat::Wav);
        assert_eq!(AudioFormat::from_extension("unknown"), AudioFormat::Unknown);
    }

    #[test]
    fn test_audio_format_extension() {
        assert_eq!(AudioFormat::Mp3.extension(), "mp3");
        assert_eq!(AudioFormat::Ogg.extension(), "ogg");
        assert_eq!(AudioFormat::Unknown.extension(), "unknown");
    }
}
