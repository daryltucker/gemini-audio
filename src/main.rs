// Gemini Audio - Main Entry Point
// A Rust client for Gemini 2.5 Flash Native Audio

use clap::Parser;
use gemini_audio::*;
use std::path::PathBuf;
use std::env;

/// Gemini Audio - AI-powered audio processing with Gemini 2.5 Flash Native Audio
#[derive(Parser, Debug)]
#[command(name = "gemini-audio")]
#[command(about = "Process audio files with Gemini AI")]
struct Args {
    /// Input audio file path
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Output audio file path (optional)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Prompt name to use. Defaults to "default" (~/.config/gemini-audio/prompts/default.md).
    #[arg(short, long)]
    prompt: Option<String>,

    /// Disable audio playback
    #[arg(long)]
    no_audio_playback: bool,

    /// Audio device to use for playback
    #[arg(long)]
    audio_device: Option<String>,

    /// List available audio devices
    #[arg(long)]
    list_devices: bool,

    /// Chunk size in milliseconds (for streaming mode)
    #[arg(long, default_value = "3000")]
    chunk_size: u64,

    /// Buffer size in milliseconds (for playback)
    #[arg(long, default_value = "500")]
    buffer_size: u64,

    /// Log level (DEBUG, INFO, WARN, ERROR)
    #[arg(long, default_value = "INFO")]
    log_level: String,

    /// Disable console output (only file/journald logging)
    #[arg(long)]
    no_console_output: bool,

    /// Dry run - validate configuration without processing
    #[arg(long)]
    dry_run: bool,

    /// Max retries for 5xx errors
    #[arg(long, default_value = "3")]
    retry_5xx: usize,

    /// Launch the interactive Terminal User Interface (TUI)
    #[arg(long)]
    tui: bool,

    /// One-shot mode: make a fresh WebSocket connection for each recorded utterance.
    /// By default the TUI keeps a single persistent connection with session resumption.
    #[arg(long)]
    one_shot: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load environment variables
    dotenv::dotenv().ok();

    // Get data directory
    let data_dir = get_data_directory()?;

    // Initialize logging before any code path — TUI disables console to avoid corrupting the terminal
    let is_tui = args.input.is_none() || args.tui;
    let log_to_console = !args.no_console_output && !is_tui;
    init_logging(&args.log_level, log_to_console, false, &data_dir)?;

    // Launch TUI if no input is provided or --tui flag is used
    if is_tui {
        let prompt_manager = build_prompt_manager()?;
        prompt_manager.ensure_default()?;
        let prompt_id = args.prompt.as_deref().unwrap_or("default");
        let prompt_content = prompt_manager.load_prompt(prompt_id)?;
        return tui::run_tui(prompt_content, args.one_shot).await;
    }

    // List audio devices if requested
    if args.list_devices {
        list_audio_devices();
        return Ok(());
    }

    // Validate configuration
    validate_config(&args)?;

    if args.dry_run {
        println!("Configuration validated successfully");
        return Ok(());
    }

    // Process audio file
    process_single_file(&args, &data_dir).await?;

    Ok(())
}

/// Build the prompt manager: user config dir (~/.config/gemini-audio/prompts/) as primary,
/// ./prompts/ (cwd-relative) as bundled fallback for dev use.
fn build_prompt_manager() -> Result<PromptManager> {
    let base = directories_next::BaseDirs::new()
        .ok_or_else(|| GeminiAudioError::Configuration("Could not determine home directory".to_string()))?;

    let user_dir = base.config_dir().join("gemini-audio").join("prompts");
    let bundled_dir = std::env::current_dir()
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to get current directory: {}", e)))?
        .join("prompts");

    PromptManager::new(user_dir, bundled_dir)
}

/// Get data directory for database and logs — always ~/.local/share/gemini-audio
fn get_data_directory() -> Result<PathBuf> {
    let base = directories_next::BaseDirs::new()
        .ok_or_else(|| GeminiAudioError::Configuration("Could not determine home directory".to_string()))?;

    let data_dir = base.data_local_dir().join("gemini-audio");

    std::fs::create_dir_all(&data_dir)
        .map_err(|e| GeminiAudioError::Configuration(format!("Failed to create data directory {}: {}", data_dir.display(), e)))?;

    Ok(data_dir)
}

/// Validate configuration
fn validate_config(args: &Args) -> Result<()> {
    // Check if input file exists (only if provided)
    if let Some(input) = &args.input {
        if !input.exists() {
            return Err(GeminiAudioError::InvalidInput(
                format!("Input file not found: {}", input.display())
            ));
        }
    }

    // Provide a clear error if the API key is missing.
    if env::var("GEMINI_API_KEY").is_err() {
        return Err(GeminiAudioError::Authentication(
            "GEMINI_API_KEY environment variable is not set.".to_string()
        ));
    }

    Ok(())
}

/// List available audio devices
fn list_audio_devices() {
    println!("Available audio devices:");
    // TODO: Implement audio device listing with cpal
    println!("  (Device listing not yet implemented)");
}

/// Process single audio file
async fn process_single_file(args: &Args, data_dir: &PathBuf) -> Result<()> {
    use chrono::Utc;
    use tracing::{info, error, debug};

    let input_path = args.input.as_ref().ok_or_else(|| {
        GeminiAudioError::InvalidInput("No input file provided for processing".to_string())
    })?;

    let prompt_id = args.prompt.as_deref().unwrap_or("default");

    info!("Starting single file processing");
    info!(input = %input_path.display(), prompt = %prompt_id);

    // Initialize database
    let db_path = data_dir.join(config::DEFAULT_DATABASE_NAME);
    let database = Database::new(&db_path).await?;

    // Initialize prompt manager
    let prompt_manager = build_prompt_manager()?;
    prompt_manager.ensure_default()?;

    // Load prompt
    let prompt_content = prompt_manager.load_prompt(prompt_id)?;
    database.save_prompt(prompt_id, &prompt_content).await?;

    // Create session record
    let input_format = audio::detect_audio_format(input_path)?;
    let session = Session {
        id: None,
        created_at: Utc::now(),
        prompt_id: prompt_id.to_string(),
        input_file: input_path.display().to_string(),
        input_format: input_format.extension().to_string(),
        output_file: args.output.as_ref().map(|p| p.display().to_string()),
        output_format: Some("wav".to_string()),
        status: SessionStatus::Pending,
        error_message: None,
        retry_count: 0,
        last_retry_at: None,
        audio_device: args.audio_device.clone(),
        play_audio: !args.no_audio_playback,
        chunk_size_ms: Some(args.chunk_size as i32),
        buffer_size_ms: Some(args.buffer_size as i32),
        log_id: None,
    };

    let session_id = database.create_session(&session).await?;
    info!(session_id = session_id, "Created session");

    // Update status to processing
    database.update_session_status(session_id, SessionStatus::Processing, None).await?;

    // Decode and resample audio to 16 kHz mono PCM using symphonia + rubato
    info!("Decoding audio to 16kHz PCM");
    let pcm_data = audio::decode_to_pcm_16k(input_path)?;
    info!(pcm_size = pcm_data.len(), "Decoded PCM data");

    // Connect to Gemini API and process
    info!("Connecting to Gemini API and processing audio");
    let mut retry_manager = RetryManager::new(config::RetryConfig::default());
    
    let (all_output_audio, final_user_transcript, final_assistant_transcript) = retry_manager.execute_with_retry(|| {
        let pcm_data = pcm_data.clone();
        let prompt_content = prompt_content.clone();
        
        async move {
            let mut client = GeminiClient::connect().await?;
            
            // Send setup
            client.send_setup(Some(prompt_content), None).await?;
            debug!("Sent setup frame");

            // Wait for setup complete
            loop {
                let response = client.receive_response().await?;
                if response.setup_complete.is_some() {
                    break;
                }
                if let Some(error) = &response.error {
                    return Err(GeminiAudioError::API(format!("Server error: {} - {}", error.code, error.message)));
                }
            }

            // Manual activity markers (auto-VAD is disabled in setup).
            // Brackets the full recording so the server processes it as one utterance.
            debug!("Sending audio data");
            client.send_activity_start().await?;
            client.send_audio(&pcm_data).await?;
            client.send_activity_end().await?;

            // Receive response(s) until turn complete
            debug!("Waiting for response");
            let mut output_audio = Vec::new();
            let mut user_transcript = String::new();
            let mut assistant_transcript = String::new();
            
            loop {
                let response_result = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    client.receive_response()
                ).await;

                let response = match response_result {
                    Ok(Ok(res)) => res,
                    Ok(Err(e)) => {
                        return Err(e);
                    }
                    Err(_) => {
                        return Err(GeminiAudioError::Timeout("Timed out waiting for server response".to_string()));
                    }
                };
                
                if let Some(error) = &response.error {
                    return Err(GeminiAudioError::API(format!("Server error: {} - {}", error.code, error.message)));
                }

                if let Some(output_pcm) = GeminiClient::extract_audio_data(&response)? {
                    output_audio.extend(output_pcm);
                }

                if let Some(server_content) = &response.server_content {
                    if let Some(transcription) = &server_content.input_transcription {
                        print!("\rUser: {}", transcription.text);
                        user_transcript.push_str(&transcription.text);
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }

                    if let Some(model_turn) = &server_content.model_turn {
                        for part in &model_turn.parts {
                            if let Some(text) = &part.text {
                                print!("{}", text);
                                assistant_transcript.push_str(text);
                                use std::io::Write;
                                let _ = std::io::stdout().flush();
                            }
                        }
                    }

                    if server_content.interrupted.unwrap_or(false) {
                        return Err(GeminiAudioError::Processing(
                            "Generation interrupted by server — please try again".to_string()
                        ));
                    }

                    if server_content.turn_complete.unwrap_or(false)
                        || server_content.generation_complete.unwrap_or(false)
                    {
                        println!();
                        break;
                    }
                }
            }
            
            let _ = client.close().await;
            Ok((output_audio, user_transcript, assistant_transcript))
        }
    }).await?;
    
    if !all_output_audio.is_empty() {
        info!(total_output_size = all_output_audio.len(), "Received full audio response");

        // Save conversation transcript to JSON file
        if let Some(base_dirs) = directories_next::BaseDirs::new() {
            let mut conv_dir = base_dirs.config_dir().to_path_buf();
            conv_dir.push("gemini-audio");
            conv_dir.push("conversations");
            
            if std::fs::create_dir_all(&conv_dir).is_ok() {
                let log_file = conv_dir.join(format!("session_{}.json", session_id));
                let log_data = serde_json::json!({
                    "session_id": session_id,
                    "timestamp": Utc::now().to_rfc3339(),
                    "prompt_id": prompt_id,
                    "user_transcript": final_user_transcript.trim(),
                    "assistant_transcript": final_assistant_transcript.trim()
                });
                
                if let Ok(json_str) = serde_json::to_string_pretty(&log_data) {
                    if std::fs::write(&log_file, json_str).is_ok() {
                        info!(transcript_file = %log_file.display(), "Saved conversation transcript");
                    }
                }
            }
        }

        // Save output
        let output_path = args.output.clone().unwrap_or_else(|| {
            input_path.with_extension("output.wav")
        });

        audio::write_wav_pcm(&output_path, &all_output_audio, config::OUTPUT_SAMPLE_RATE)?;
        info!(output_path = %output_path.display(), "Saved output audio");

        // Update session status
        database.update_session_status(session_id, SessionStatus::Completed, None).await?;
        info!("Session completed successfully");

        // Play audio if requested
        if !args.no_audio_playback {
            info!("Playing output audio");
            audio::play_pcm_pulseaudio(&all_output_audio, config::OUTPUT_SAMPLE_RATE)?;
        }
    } else {
        error!("No audio data in response");
        database.update_session_status(session_id, SessionStatus::Failed, Some("No audio data received".to_string())).await?;
        return Err(GeminiAudioError::Processing("No audio data received".to_string()));
    }

    // Close database connection
    database.close().await;

    Ok(())
}

