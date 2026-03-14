package audio.gemini.app.data

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.gemini_audio_core.listPrompts
import uniffi.gemini_audio_core.loadPrompt
import uniffi.gemini_audio_core.PromptInfo
import java.io.File

/**
 * Repository for managing prompts (bundled + user-created).
 *
 * Bundled prompts are read-only and stored in the app's assets.
 * User prompts are stored in the app's private files directory.
 */
class PromptRepository(private val context: Context) {

    private val userPromptsDir: File
        get() = File(context.filesDir, "prompts")

    init {
        userPromptsDir.mkdirs()
    }

    /**
     * List all available prompts (bundled + user).
     */
    suspend fun listPrompts(): List<PromptInfo> = withContext(Dispatchers.IO) {
        try {
            val bundledDir = context.assets.list("prompts")?.let { paths ->
                // Get the actual directory path
                context.cacheDir.absolutePath + "/bundled_prompts"
            } ?: context.cacheDir.absolutePath

            // Create bundled prompts directory if it doesn't exist
            val bundledPromptsDir = File(context.cacheDir, "bundled_prompts")
            bundledPromptsDir.mkdirs()

            // Copy bundled prompts to cache if not already there
            context.assets.list("prompts")?.forEach { filename ->
                if (filename.endsWith(".md")) {
                    val destFile = File(bundledPromptsDir, filename)
                    if (!destFile.exists()) {
                        context.assets.open("prompts/$filename").use { input ->
                            destFile.outputStream().use { output ->
                                input.copyTo(output)
                            }
                        }
                    }
                }
            }

            listPrompts(userPromptsDir.absolutePath, bundledPromptsDir.absolutePath)
        } catch (e: Exception) {
            emptyList()
        }
    }

    /**
     * Load prompt content by name.
     */
    suspend fun loadPrompt(name: String): String = withContext(Dispatchers.IO) {
        try {
            val bundledDir = File(context.cacheDir, "bundled_prompts").absolutePath
            loadPrompt(userPromptsDir.absolutePath, bundledDir, name)
        } catch (e: Exception) {
            ""
        }
    }

    /**
     * Create a new user prompt.
     */
    suspend fun createPrompt(name: String, content: String): Boolean = withContext(Dispatchers.IO) {
        try {
            val file = File(userPromptsDir, "$name.md")
            file.writeText(content)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Update an existing user prompt.
     */
    suspend fun updatePrompt(name: String, content: String): Boolean = withContext(Dispatchers.IO) {
        try {
            val file = File(userPromptsDir, "$name.md")
            if (file.exists()) {
                file.writeText(content)
                true
            } else {
                false
            }
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Delete a user prompt.
     */
    suspend fun deletePrompt(name: String): Boolean = withContext(Dispatchers.IO) {
        try {
            val file = File(userPromptsDir, "$name.md")
            file.delete()
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Get the full content of a prompt by name.
     */
    suspend fun getPromptContent(name: String): String = withContext(Dispatchers.IO) {
        try {
            val bundledDir = File(context.cacheDir, "bundled_prompts").absolutePath
            loadPrompt(userPromptsDir.absolutePath, bundledDir, name)
        } catch (e: Exception) {
            ""
        }
    }

    /**
     * Check if a prompt is bundled (read-only).
     */
    suspend fun isBundled(name: String): Boolean = withContext(Dispatchers.IO) {
        try {
            val bundledDir = File(context.cacheDir, "bundled_prompts")
            File(bundledDir, "$name.md").exists()
        } catch (e: Exception) {
            false
        }
    }
}
