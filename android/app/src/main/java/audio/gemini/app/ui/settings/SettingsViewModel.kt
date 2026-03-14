package audio.gemini.app.ui.settings

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import audio.gemini.app.data.SettingsRepository
import audio.gemini.app.data.PromptRepository
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch

import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.gemini_audio_core.verifyApiKey
import uniffi.gemini_audio_core.availableVoices

data class SettingsUiState(
    val apiKey: String = "",
    val saveAudio: Boolean = false,
    val showThinking: Boolean = false,
    val defaultPrompt: String = "default",
    val defaultVoice: String = "Fenrir",
    val apiKeyVisible: Boolean = false,
    val apiKeyError: String? = null,
    val availableVoices: List<String> = emptyList(),
    val availablePrompts: List<String> = emptyList(),
)

class SettingsViewModel(application: Application) : AndroidViewModel(application) {

    private val repo = SettingsRepository(application)
    private val promptRepo = PromptRepository(application)

    private val _apiKeyVisible = MutableStateFlow(false)
    private val _apiKeyError = MutableStateFlow<String?>(null)
    private val _availableVoices = MutableStateFlow<List<String>>(emptyList())
    private val _availablePrompts = MutableStateFlow<List<String>>(emptyList())

    init {
        // Load available voices from Rust core
        viewModelScope.launch {
            try {
                val voices = withContext(Dispatchers.IO) {
                    availableVoices()
                }
                _availableVoices.value = voices
            } catch (e: Exception) {
                // Fallback to default voice if unavailable
                _availableVoices.value = listOf("Fenrir")
            }
        }
        
        refreshPrompts()
    }

    fun refreshPrompts() {
        viewModelScope.launch {
            try {
                val promptInfos = promptRepo.listPrompts()
                val names = promptInfos.map { it.name }.sorted()
                _availablePrompts.value = names
                // If saved default no longer exists, reset to "default"
                val saved = repo.getDefaultPromptSync()
                if (saved !in names) {
                    repo.setDefaultPrompt("default")
                }
            } catch (e: Exception) {
                _availablePrompts.value = listOf("default")
            }
        }
    }

    // Combine all flows into one UI state
    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()

    // Collect individual flows and update state
    init {
        viewModelScope.launch {
            repo.apiKey.collect { value ->
                _uiState.update { it.copy(apiKey = value) }
            }
        }
        viewModelScope.launch {
            repo.saveAudio.collect { value ->
                _uiState.update { it.copy(saveAudio = value) }
            }
        }
        viewModelScope.launch {
            repo.showThinking.collect { value ->
                _uiState.update { it.copy(showThinking = value) }
            }
        }
        viewModelScope.launch {
            repo.defaultPrompt.collect { value ->
                _uiState.update { it.copy(defaultPrompt = value) }
            }
        }
        viewModelScope.launch {
            repo.defaultVoice.collect { value ->
                _uiState.update { it.copy(defaultVoice = value) }
            }
        }
        viewModelScope.launch {
            _apiKeyVisible.collect { value ->
                _uiState.update { it.copy(apiKeyVisible = value) }
            }
        }
        viewModelScope.launch {
            _apiKeyError.collect { value ->
                _uiState.update { it.copy(apiKeyError = value) }
            }
        }
        viewModelScope.launch {
            _availableVoices.collect { value ->
                _uiState.update { it.copy(availableVoices = value) }
            }
        }
        viewModelScope.launch {
            _availablePrompts.collect { value ->
                _uiState.update { it.copy(availablePrompts = value) }
            }
        }
    }

    fun setApiKey(key: String) = viewModelScope.launch {
        val trimmed = key.trim()
        
        // Verify key using Rust core via UniFFI
        if (trimmed.isNotEmpty()) {
            val errorMsg = withContext(Dispatchers.IO) {
                try {
                    verifyApiKey(trimmed)
                } catch (e: Exception) {
                    e.message ?: "Unknown error"
                }
            }
            if (errorMsg.isNotEmpty()) {
                _apiKeyError.value = errorMsg
                return@launch
            }
        }
        
        _apiKeyError.value = null
        repo.setApiKey(trimmed)
    }

    fun setSaveAudio(on: Boolean) = viewModelScope.launch { repo.setSaveAudio(on) }
    fun setShowThinking(on: Boolean) = viewModelScope.launch { repo.setShowThinking(on) }
    fun setDefaultPrompt(name: String) = viewModelScope.launch { repo.setDefaultPrompt(name) }
    fun setDefaultVoice(name: String) = viewModelScope.launch { repo.setDefaultVoice(name) }
    fun toggleApiKeyVisibility() { _apiKeyVisible.update { !it } }
}
