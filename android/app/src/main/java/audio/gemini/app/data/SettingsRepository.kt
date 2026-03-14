package audio.gemini.app.data

import android.content.Context
import androidx.datastore.preferences.core.*
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.runBlocking

/** DataStore singleton via extension property. */
private val Context.dataStore by preferencesDataStore(name = "gemini_audio_settings")

/**
 * Persistent settings backed by Jetpack DataStore.
 *
 * Stores: API key, save-audio toggle, show-thinking toggle, default prompt name.
 */
class SettingsRepository(private val context: Context) {

    companion object {
        val API_KEY = stringPreferencesKey("gemini_api_key")
        val SAVE_AUDIO = booleanPreferencesKey("save_audio")
        val SHOW_THINKING = booleanPreferencesKey("show_thinking")
        val DEFAULT_PROMPT = stringPreferencesKey("default_prompt")
        val DEFAULT_VOICE = stringPreferencesKey("default_voice")
    }

    // ── Readers ──────────────────────────────────────────────────────────────

    val apiKey: Flow<String> = context.dataStore.data.map { it[API_KEY] ?: "" }
    val saveAudio: Flow<Boolean> = context.dataStore.data.map { it[SAVE_AUDIO] ?: false }
    val showThinking: Flow<Boolean> = context.dataStore.data.map { it[SHOW_THINKING] ?: false }
    val defaultPrompt: Flow<String> = context.dataStore.data.map { it[DEFAULT_PROMPT] ?: "default" }
    val defaultVoice: Flow<String> = context.dataStore.data.map { it[DEFAULT_VOICE] ?: "Fenrir" }

    // Synchronous getters for use in non-suspending contexts
    fun getApiKeySync(): String = runBlocking { context.dataStore.data.map { it[API_KEY] ?: "" }.first() }
    fun getSaveAudioSync(): Boolean = runBlocking { context.dataStore.data.map { it[SAVE_AUDIO] ?: false }.first() }
    fun getShowThinkingSync(): Boolean = runBlocking { context.dataStore.data.map { it[SHOW_THINKING] ?: false }.first() }
    fun getDefaultPromptSync(): String = runBlocking { context.dataStore.data.map { it[DEFAULT_PROMPT] ?: "default" }.first() }
    fun getDefaultVoiceSync(): String = runBlocking { context.dataStore.data.map { it[DEFAULT_VOICE] ?: "Fenrir" }.first() }

    // ── Writers ──────────────────────────────────────────────────────────────

    suspend fun setApiKey(key: String) {
        context.dataStore.edit { it[API_KEY] = key }
    }

    suspend fun setSaveAudio(enabled: Boolean) {
        context.dataStore.edit { it[SAVE_AUDIO] = enabled }
    }

    suspend fun setShowThinking(enabled: Boolean) {
        context.dataStore.edit { it[SHOW_THINKING] = enabled }
    }

    suspend fun setDefaultPrompt(name: String) {
        context.dataStore.edit { it[DEFAULT_PROMPT] = name }
    }

    suspend fun setDefaultVoice(name: String) {
        context.dataStore.edit { it[DEFAULT_VOICE] = name }
    }
}
