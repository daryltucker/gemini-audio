package audio.gemini.app.ui.newconversation

import android.app.Application
import android.content.Context
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import audio.gemini.app.data.ConversationRepository
import audio.gemini.app.data.PromptRepository
import audio.gemini.app.data.SettingsRepository
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.gemini_audio_core.availableVoices
import uniffi.gemini_audio_core.listPrompts

data class NewConversationUiState(
    val selectedVoice: String = "Fenrir",
    val selectedPrompt: String = "default",
    val availableVoices: List<String> = emptyList(),
    val availablePrompts: List<String> = emptyList(),
    val isLoading: Boolean = false,
)

class NewConversationViewModel(application: Application) : AndroidViewModel(application) {

    private val context: Context = application.applicationContext
    private val settingsRepo = SettingsRepository(application)
    private val promptRepo = PromptRepository(application)
    private val conversationRepo = ConversationRepository(application)

    private val _selectedVoice = MutableStateFlow("Fenrir")
    private val _selectedPrompt = MutableStateFlow("default")
    private val _availableVoices = MutableStateFlow<List<String>>(emptyList())
    private val _availablePrompts = MutableStateFlow<List<String>>(emptyList())
    private val _isLoading = MutableStateFlow(true)

    val uiState: StateFlow<NewConversationUiState> = combine(
        _selectedVoice, _selectedPrompt, _availableVoices, _availablePrompts, _isLoading
    ) { voice, prompt, voices, prompts, loading ->
        NewConversationUiState(voice, prompt, voices, prompts, loading)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), NewConversationUiState())

    init {
        loadDefaults()
    }

    private fun loadDefaults() {
        viewModelScope.launch {
            _isLoading.value = true
            try {
                // Load default voice from settings (use first() to get initial value)
                _selectedVoice.value = settingsRepo.defaultVoice.first()

                // Load default prompt from settings (use first() to get initial value)
                _selectedPrompt.value = settingsRepo.defaultPrompt.first()

                // Load available voices from Rust
                val voices = withContext(Dispatchers.IO) {
                    availableVoices()
                }
                _availableVoices.value = voices

                // Load available prompts
                val promptInfos = promptRepo.listPrompts()
                _availablePrompts.value = promptInfos.map { it.name }
            } catch (e: Exception) {
                // Use fallback values
                _availableVoices.value = listOf("Fenrir")
                _availablePrompts.value = listOf("default")
            } finally {
                _isLoading.value = false
            }
        }
    }

    fun selectVoice(voice: String) {
        _selectedVoice.value = voice
    }

    fun selectPrompt(prompt: String) {
        _selectedPrompt.value = prompt
    }

    /**
     * Refresh prompts and voices - call when screen becomes visible
     */
    fun refresh() {
        viewModelScope.launch {
            try {
                // Load available voices from Rust
                val voices = withContext(Dispatchers.IO) {
                    availableVoices()
                }
                _availableVoices.value = voices

                // Load available prompts
                val promptInfos = promptRepo.listPrompts()
                _availablePrompts.value = promptInfos.map { it.name }.sorted()
            } catch (e: Exception) {
                // Keep existing values on error
            }
        }
    }
}
