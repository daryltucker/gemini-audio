package audio.gemini.app.ui.activeconversation

import android.app.Application
import android.content.Context
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.gemini_audio_core.loadConversation
import uniffi.gemini_audio_core.addConversationTurn
import uniffi.gemini_audio_core.loadPrompt
import uniffi.gemini_audio_core.Session
import uniffi.gemini_audio_core.SessionCallback
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.io.File
import audio.gemini.app.audio.OboeAudioCapture
import audio.gemini.app.audio.OboeAudioPlayback
import audio.gemini.app.data.SessionCallbackImpl
import audio.gemini.app.data.SettingsRepository

data class ConversationTurn(
    val turn: ULong,
    val timestamp: String,
    val voice: String,
    val userText: String,
    val assistantText: String,
    val thinking: String = "",
    val hasRecording: Boolean = false,
)

data class ActiveConversationUiState(
    val turns: List<ConversationTurn> = emptyList(),
    val isListening: Boolean = false,
    val isProcessing: Boolean = false,
    val error: String? = null,
    val conversationId: ULong? = null,
    val userTranscript: String = "",
    val assistantTranscript: String = "",
    val thinking: String = "",
    val showThinking: Boolean = false,
)

class ActiveConversationViewModel(
    application: Application,
    initialVoice: String = "Fenrir",
    initialPrompt: String = "default"
) : AndroidViewModel(application) {

    private val context: Context = application.applicationContext
    private val settingsRepository = SettingsRepository(application)
    
    // Store the selected voice and prompt for this conversation
    private val selectedVoice = initialVoice
    private val selectedPrompt = initialPrompt

    private val _turns = MutableStateFlow<List<ConversationTurn>>(emptyList())
    private val _isListening = MutableStateFlow(false)
    private val _isProcessing = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)
    private val _conversationId = MutableStateFlow<ULong?>(null)
    
    // Live transcripts for the current turn
    private val _userTranscript = MutableStateFlow("")
    private val _assistantTranscript = MutableStateFlow("")
    private val _thinking = MutableStateFlow("")
    
    // Load showThinking setting
    private val _showThinking = settingsRepository.showThinking.stateIn(
        viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    val uiState: StateFlow<ActiveConversationUiState> = combine(
        _turns, _isListening, _isProcessing, _error, _conversationId, _userTranscript, _assistantTranscript, _thinking, _showThinking
    ) { values ->
        // values is Array<Any?>
        val turns = values[0] as List<ConversationTurn>
        val listening = values[1] as Boolean
        val processing = values[2] as Boolean
        val error = values[3] as String?
        val id = values[4] as ULong?
        val userTranscript = values[5] as String
        val assistantTranscript = values[6] as String
        val thinking = values[7] as String
        val showThinking = values[8] as Boolean
        
        ActiveConversationUiState(turns, listening, processing, error, id, userTranscript, assistantTranscript, thinking, showThinking)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ActiveConversationUiState())

    private var session: Session? = null
    private var recorder: OboeAudioCapture? = null
    private var player: OboeAudioPlayback? = null

    init {
        // Initialize audio playback - wrap in try/catch to prevent crash on init
        try {
            player = OboeAudioPlayback().apply { start() }
        } catch (e: Exception) {
            android.util.Log.e("ActiveConvVM", "Audio playback init error: ${e.message}")
        }
    }

    fun loadConversation(conversationId: ULong) {
        _conversationId.value = conversationId
        viewModelScope.launch {
            try {
                val dataDir = context.filesDir.absolutePath
                val turns = withContext(Dispatchers.IO) {
                    loadConversation(dataDir, conversationId)
                }.map { turn ->
                    ConversationTurn(
                        turn = turn.turn,
                        timestamp = turn.timestamp,
                        voice = turn.voice,
                        userText = turn.userText,
                        assistantText = turn.assistantText,
                        thinking = turn.thinking,
                        hasRecording = turn.hasRecording,
                    )
                }
                _turns.value = turns
            } catch (e: Exception) {
                _error.value = "Failed to load conversation: ${e.message}"
            }
        }
    }

    fun startListening() {
        try {
            // Clear transcripts at START of new turn (before previous ones are hidden)
            _userTranscript.value = ""
            _assistantTranscript.value = ""
            _thinking.value = ""
            
            // Get API key from settings and set it for Rust
            val apiKey = settingsRepository.getApiKeySync()
            if (apiKey.isEmpty()) {
                _error.value = "No API Key for Gemini. Please set it in Settings."
                return
            }
            // Set the API key in environment for Rust
            uniffi.gemini_audio_core.verifyApiKey(apiKey)
            
            // Use selected prompt and voice (passed from NewConversation), not defaults from settings
            // Load the actual prompt CONTENT (not just name)
            val userDir = File(context.filesDir, "prompts").absolutePath
            val bundledDir = File(context.cacheDir, "bundled_prompts").absolutePath
            val promptContent = try {
                loadPrompt(userDir, bundledDir, selectedPrompt).ifEmpty {
                    // Prompt was deleted — fall back to bundled default
                    loadPrompt(userDir, bundledDir, "default")
                }
            } catch (e: Exception) {
                android.util.Log.e("ActiveConvVM", "Failed to load prompt: ${e.message}")
                ""
            }
            val prompt = promptContent
            val voice = selectedVoice
            
            if (session == null) {
                // Create session callback
                val callback = SessionCallbackImpl(
                    userTranscriptFlow = _userTranscript,
                    assistantTranscriptFlow = _assistantTranscript,
                    thinkingFlow = _thinking,
                    errorFlow = _error,
                    onAudioChunk = { chunk ->
                        player?.write(chunk)
                    },
                    onSessionEnd = {
                        val userText = _userTranscript.value
                        val assistantText = _assistantTranscript.value
                        if (userText.isNotEmpty() || assistantText.isNotEmpty()) {
                            saveTurnSync(userText, assistantText)
                        }
                        _isProcessing.value = false
                    },
                    onError = {
                        // Connection broke — stop recording, discard session so next press reconnects
                        recorder?.stop()
                        recorder = null
                        session = null
                        _isListening.value = false
                        _isProcessing.value = false
                    }
                )
                
                session = Session(callback)
                session?.start(prompt, voice)
            }
            
            _isListening.value = true
            _isProcessing.value = false
            
            // Start recording - wrap in try/catch to prevent crash
            try {
                recorder = OboeAudioCapture { chunk ->
                    // Send audio chunk to session
                    session?.sendAudio(chunk)
                }
                recorder?.start()
            } catch (e: Exception) {
                _error.value = "Audio capture error: ${e.message}"
                _isListening.value = false
            }
        } catch (e: Exception) {
            _error.value = "Failed to start: ${e.message}"
            _isListening.value = false
        }
    }

    fun stopListening() {
        _isListening.value = false
        
        // Stop recording
        recorder?.stop()
        recorder = null
        
        // Do NOT stop player here - we need to hear Gemini's response!
        // Player will be stopped when session ends or on explicit interrupt
        
        // End the current turn (activity) - this sends activityEnd to server
        // Server will respond, then onSessionEnd will be called on turn_complete
        session?.endTurn()
        
        // Keep processing while waiting for Gemini's response
        _isProcessing.value = true
    }
    
    /**
     * Interrupt Gemini's response (barge-in).
     * - Stop audio playback immediately
     * - Save Gemini's partial response to conversation
     * - Start a new turn to record user's response
     */
    fun interruptAndStartListening() {
        // Cut audio instantly — non-blocking, doesn't stall the UI thread
        player?.stopImmediate()
        player = try {
            OboeAudioPlayback().apply { start() }
        } catch (e: Exception) {
            android.util.Log.e("ActiveConvVM", "Failed to restart player: ${e.message}")
            null
        }

        // Save Gemini's partial response
        val userText = _userTranscript.value
        val assistantText = _assistantTranscript.value
        if (userText.isNotEmpty() || assistantText.isNotEmpty()) {
            saveTurnSync(userText, assistantText)
        }

        // Clear transcripts for new turn
        _userTranscript.value = ""
        _assistantTranscript.value = ""
        _thinking.value = ""
        _isProcessing.value = false

        // Start recording immediately
        startListening()
    }
    
    // Synchronous version for callback context
    private fun saveTurnSync(userText: String, assistantText: String) {
        val convId = _conversationId.value ?: return
        val voice = settingsRepository.getDefaultVoiceSync()
        val thinking = _thinking.value  // Always save thinking, regardless of showThinking setting
        
        try {
            val nextTurn = (_turns.value.size + 1).toULong()
            addConversationTurn(
                context.filesDir.absolutePath,
                convId,
                nextTurn,
                voice,
                userText,
                assistantText,
                thinking  // Always recorded
            )
            
            // Add to UI
            val timestamp = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.US).format(Date())
            val newTurn = ConversationTurn(
                turn = nextTurn,
                timestamp = timestamp,
                voice = voice,
                userText = userText,
                assistantText = assistantText,
                thinking = thinking,
                hasRecording = false
            )
            _turns.value = _turns.value + newTurn
        } catch (e: Exception) {
            _error.value = "Failed to save turn: ${e.message}"
        }
    }
    
    fun clearError() {
        _error.value = null
    }
    
    override fun onCleared() {
        super.onCleared()
        player?.stop()
        recorder?.stop()
    }
}

/**
 * Factory for creating ActiveConversationViewModel with custom voice/prompt
 */
class ActiveConversationViewModelFactory(
    private val application: android.app.Application,
    private val initialVoice: String = "Fenrir",
    private val initialPrompt: String = "default"
) : androidx.lifecycle.ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : androidx.lifecycle.ViewModel> create(modelClass: Class<T>): T {
        if (modelClass.isAssignableFrom(ActiveConversationViewModel::class.java)) {
            return ActiveConversationViewModel(application, initialVoice, initialPrompt) as T
        }
        throw IllegalArgumentException("Unknown ViewModel class")
    }
}
