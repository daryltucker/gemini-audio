package audio.gemini.app.ui.prompts

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import audio.gemini.app.data.PromptRepository
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class PromptItem(
    val name: String,
    val isBundled: Boolean,
    val preview: String,
    val content: String = "",
)

data class PromptsUiState(
    val prompts: List<PromptItem> = emptyList(),
    val isLoading: Boolean = false,
    val error: String? = null,
    val selectedPrompt: String? = null,
)

class PromptViewModel(application: Application) : AndroidViewModel(application) {

    private val repo = PromptRepository(application)
    private val settings = audio.gemini.app.data.SettingsRepository(application)

    private val _prompts = MutableStateFlow<List<PromptItem>>(emptyList())
    private val _isLoading = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)
    private val _selectedPrompt = MutableStateFlow<String?>(null)

    val uiState: StateFlow<PromptsUiState> = combine(
        _prompts, _isLoading, _error, _selectedPrompt
    ) { prompts, isLoading, error, selected ->
        PromptsUiState(prompts, isLoading, error, selected)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), PromptsUiState())

    init {
        loadPrompts()
    }

    fun loadPrompts() {
        viewModelScope.launch {
            _isLoading.value = true
            _error.value = null
            try {
                val promptInfos = repo.listPrompts()
                val items = promptInfos.map { info ->
                    val content = repo.loadPrompt(info.name)
                    val preview = content.take(100).replace("\n", " ").trim()
                    PromptItem(
                        name = info.name,
                        isBundled = repo.isBundled(info.name),
                        preview = if (preview.isNotEmpty()) preview else "No content",
                        content = content,
                    )
                }
                _prompts.value = items.sortedBy { it.name }
            } catch (e: Exception) {
                _error.value = "Failed to load prompts: ${e.message}"
            } finally {
                _isLoading.value = false
            }
        }
    }

    fun selectPrompt(name: String?) {
        _selectedPrompt.value = name
    }

    fun createPrompt(name: String, content: String) {
        viewModelScope.launch {
            try {
                val success = repo.createPrompt(name, content)
                if (success) {
                    loadPrompts()
                } else {
                    _error.value = "Failed to create prompt"
                }
            } catch (e: Exception) {
                _error.value = "Failed to create prompt: ${e.message}"
            }
        }
    }

    fun updatePrompt(name: String, content: String) {
        viewModelScope.launch {
            try {
                val success = repo.updatePrompt(name, content)
                if (success) {
                    loadPrompts()
                } else {
                    _error.value = "Failed to update prompt"
                }
            } catch (e: Exception) {
                _error.value = "Failed to update prompt: ${e.message}"
            }
        }
    }

    fun deletePrompt(name: String) {
        viewModelScope.launch {
            try {
                val isBundled = repo.isBundled(name)
                if (isBundled) {
                    _error.value = "Cannot delete bundled prompt"
                    return@launch
                }
                val success = repo.deletePrompt(name)
                if (success) {
                    loadPrompts()
                    if (_selectedPrompt.value == name) {
                        _selectedPrompt.value = null
                    }
                    // If this was the saved default, reset to "default"
                    if (settings.getDefaultPromptSync() == name) {
                        settings.setDefaultPrompt("default")
                    }
                } else {
                    _error.value = "Failed to delete prompt"
                }
            } catch (e: Exception) {
                _error.value = "Failed to delete prompt: ${e.message}"
            }
        }
    }

    /**
     * Load a single prompt's content by name.
     */
    fun loadPromptContent(name: String, onLoaded: (String, Boolean) -> Unit) {
        viewModelScope.launch {
            try {
                val content = repo.getPromptContent(name)
                val isBundled = repo.isBundled(name)
                onLoaded(content, isBundled)
            } catch (e: Exception) {
                _error.value = "Failed to load prompt: ${e.message}"
                onLoaded("", false)
            }
        }
    }

    fun clearError() {
        _error.value = null
    }
}
