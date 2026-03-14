package audio.gemini.app.ui.conversations

import android.app.Application
import android.content.Context
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.receiveAsFlow
import uniffi.gemini_audio_core.listConversations
import uniffi.gemini_audio_core.deleteConversation
import uniffi.gemini_audio_core.loadConversation

data class ConversationItem(
    val id: ULong,
    val timestamp: String,
    val preview: String,
    val turnCount: Int,
)

data class ConversationsUiState(
    val conversations: List<ConversationItem> = emptyList(),
    val isLoading: Boolean = false,
    val error: String? = null,
)

class ConversationsViewModel(application: Application) : AndroidViewModel(application) {

    private val context: Context = application.applicationContext

    private val _conversations = MutableStateFlow<List<ConversationItem>>(emptyList())
    private val _isLoading = MutableStateFlow(false)
    private val _error = MutableStateFlow<String?>(null)

    private val _shareText = Channel<String>(Channel.BUFFERED)
    val shareText = _shareText.receiveAsFlow()

    val uiState: StateFlow<ConversationsUiState> = combine(
        _conversations, _isLoading, _error
    ) { conversations, isLoading, error ->
        ConversationsUiState(conversations, isLoading, error)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ConversationsUiState())

    init {
        loadConversations()
    }

    fun loadConversations() {
        viewModelScope.launch {
            _isLoading.value = true
            _error.value = null
            try {
                val dataDir = context.filesDir.absolutePath
                val summaries = withContext(Dispatchers.IO) {
                    listConversations(dataDir)
                }
                val items = summaries.map { summary ->
                    ConversationItem(
                        id = summary.id,
                        timestamp = summary.timestamp,
                        preview = summary.preview,
                        turnCount = summary.turnCount.toInt(),
                    )
                }
                _conversations.value = items
            } catch (e: Exception) {
                _error.value = "Failed to load conversations: ${e.message}"
            } finally {
                _isLoading.value = false
            }
        }
    }

    fun deleteConversation(id: ULong) {
        viewModelScope.launch {
            try {
                val dataDir = context.filesDir.absolutePath
                withContext(Dispatchers.IO) {
                    deleteConversation(dataDir, id)
                }
                loadConversations()  // Refresh list
            } catch (e: Exception) {
                _error.value = "Failed to delete conversation: ${e.message}"
            }
        }
    }

    fun shareConversation(id: ULong, timestamp: String) {
        viewModelScope.launch {
            try {
                val dataDir = context.filesDir.absolutePath
                val turns = withContext(Dispatchers.IO) {
                    loadConversation(dataDir, id)
                }
                val sb = StringBuilder()
                sb.appendLine("Gemini Audio Conversation")
                sb.appendLine(timestamp)
                sb.appendLine()
                turns.forEach { turn ->
                    if (turn.userText.isNotBlank()) {
                        sb.appendLine("You: ${turn.userText}")
                    }
                    if (turn.assistantText.isNotBlank()) {
                        val label = if (turn.voice.isNotBlank()) "Gemini (${turn.voice})" else "Gemini"
                        sb.appendLine("$label: ${turn.assistantText}")
                    }
                    sb.appendLine()
                }
                _shareText.send(sb.toString().trimEnd())
            } catch (e: Exception) {
                _error.value = "Failed to share conversation: ${e.message}"
            }
        }
    }

    fun clearError() {
        _error.value = null
    }
}
