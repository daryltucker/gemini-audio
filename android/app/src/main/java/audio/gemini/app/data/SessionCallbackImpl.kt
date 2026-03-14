package audio.gemini.app.data

import uniffi.gemini_audio_core.SessionCallback
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/**
 * Transcript tracker that handles incremental updates from the server.
 * The server sends partial transcripts that may overlap with previous updates.
 * We need to find the overlap and only append new content.
 * 
 * This mirrors the SttTracker logic from the TUI implementation.
 */
class TranscriptTracker {
    private var text: String = ""

    /**
     * Update with new transcript, returning the merged result.
     * - Trim end of existing, trim start of new
     * - Find best overlap
     * - If overlap found, append only the new suffix
     * - If no overlap, append with space (handle hyphen specially)
     */
    fun update(newText: String): String {
        if (newText.isEmpty()) return text
        
        val a = text.trimEnd()
        val b = newText.trimStart()
        
        if (a.isEmpty()) {
            text = b
            return text
        }
        
        // Find best overlap - check if new text starts with part of existing
        var bestOverlap = 0
        val maxLen = minOf(a.length, b.length)
        
        for (i in maxLen downTo 1) {
            val suffix = a.substring(a.length - i)
            if (b.startsWith(suffix)) {
                bestOverlap = i
                break
            }
        }
        
        text = if (bestOverlap > 0) {
            a + b.substring(bestOverlap)
        } else {
            val space = if (a.endsWith('-')) "" else " "
            "$a$space$b"
        }
        
        return text
    }
    
    fun getText(): String = text
    
    fun clear() {
        text = ""
    }
}

class SessionCallbackImpl(
    private val userTranscriptFlow: MutableStateFlow<String>,
    private val assistantTranscriptFlow: MutableStateFlow<String>,
    private val thinkingFlow: MutableStateFlow<String>,
    private val errorFlow: MutableStateFlow<String?>,
    private val onAudioChunk: (ByteArray) -> Unit,
    private val onSessionEnd: () -> Unit,
    private val onError: (String) -> Unit = {},
) : SessionCallback {
    private val userTracker = TranscriptTracker()
    private val assistantTracker = TranscriptTracker()
    private val thinkingTracker = TranscriptTracker()
    
    override fun onAudioChunk(chunk: ByteArray) {
        android.util.Log.d("SessionCallback", "onAudioChunk: ${chunk.size} bytes")
        onAudioChunk.invoke(chunk)
    }

    override fun onUserTranscript(text: String) {
        // Accumulate incremental transcript updates (don't just replace)
        val updated = userTracker.update(text)
        userTranscriptFlow.value = updated
    }

    override fun onAssistantTranscript(text: String) {
        android.util.Log.d("SessionCallback", "onAssistantTranscript: '$text'")
        // Accumulate incremental transcript updates (don't just replace)
        val updated = assistantTracker.update(text)
        android.util.Log.d("SessionCallback", "onAssistantTranscript updated to: '$updated'")
        assistantTranscriptFlow.value = updated
    }
    
    override fun onThinking(text: String) {
        // Handle thought tokens (internal thinking) separately
        val updated = thinkingTracker.update(text)
        thinkingFlow.value = updated
    }
    
    override fun onError(message: String) {
        errorFlow.value = message
        onError.invoke(message)
    }

    override fun onSessionEnd() {
        android.util.Log.d("SessionCallback", "onSessionEnd CALLED")
        // First call the callback so it can save the transcripts while data is still available
        onSessionEnd.invoke()
        // THEN clear trackers for next turn
        userTracker.clear()
        assistantTracker.clear()
        thinkingTracker.clear()
    }

    override fun onSessionHandle(handle: String) {
        // Handle session resumption if needed
    }
}
