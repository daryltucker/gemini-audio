package audio.gemini.app.data

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.text.SimpleDateFormat
import java.util.*

/**
 * Repository for managing conversation data files.
 */
class ConversationRepository(private val context: Context) {

    private val conversationsDir: File
        get() = File(context.filesDir, "conversations")

    private val recordingsDir: File
        get() = File(context.filesDir, "recordings")

    init {
        conversationsDir.mkdirs()
        recordingsDir.mkdirs()
    }

    /**
     * Create a new conversation file and return its ID (timestamp-based).
     */
    suspend fun createNewConversation(): Long = withContext(Dispatchers.IO) {
        val timestamp = SimpleDateFormat("yyyyMMddHHmmss", Locale.getDefault()).format(Date())
        val conversationId = timestamp.toLong()
        val conversationFile = File(conversationsDir, "$conversationId.jsonl")
        conversationFile.createNewFile()
        conversationId
    }

    /**
     * Get the path for a conversation file.
     */
    fun getConversationPath(conversationId: Long): String {
        return File(conversationsDir, "$conversationId.jsonl").absolutePath
    }

    /**
     * Get the recordings directory path.
     */
    fun getRecordingsPath(): String {
        return recordingsDir.absolutePath
    }
}
