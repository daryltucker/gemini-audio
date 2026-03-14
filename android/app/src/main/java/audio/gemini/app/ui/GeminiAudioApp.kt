package audio.gemini.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import audio.gemini.app.ui.conversations.ConversationsScreen
import audio.gemini.app.ui.settings.SettingsScreen
import audio.gemini.app.ui.prompts.PromptListScreen
import audio.gemini.app.ui.prompts.PromptEditorScreen
import audio.gemini.app.ui.newconversation.NewConversationScreen
import audio.gemini.app.ui.activeconversation.ActiveConversationScreen
import uniffi.gemini_audio_core.createConversation

/**
 * Root composable — Material 3 dynamic theme + navigation.
 */
@Composable
fun GeminiAudioApp() {
    val colorScheme = if (isSystemInDarkTheme()) {
        dynamicDarkColorScheme(androidx.compose.ui.platform.LocalContext.current)
    } else {
        dynamicLightColorScheme(androidx.compose.ui.platform.LocalContext.current)
    }

    MaterialTheme(colorScheme = colorScheme) {
        val navController = rememberNavController()

        NavHost(navController = navController, startDestination = "conversations") {
            composable("conversations") {
                ConversationsScreen(
                    onSettingsClick = { navController.navigate("settings") },
                    onPromptsClick = { navController.navigate("prompts") },
                    onNewConversation = { navController.navigate("new_conversation") },
                    onConversationClick = { id -> navController.navigate("conversation/$id") }
                )
            }
            composable("settings") {
                SettingsScreen(
                    onBack = { navController.popBackStack() }
                )
            }
            composable("prompts") {
                PromptListScreen(
                    onBack = { navController.popBackStack() },
                    onEditPrompt = { name ->
                        if (name != null) {
                            navController.navigate("prompt_edit/$name")
                        } else {
                            navController.navigate("prompt_new")
                        }
                    }
                )
            }
            composable("prompt_new") {
                PromptEditorScreen(
                    promptName = null,
                    onBack = { navController.popBackStack() }
                )
            }
            composable("prompt_edit/{name}") { backStackEntry ->
                val name = backStackEntry.arguments?.getString("name")
                PromptEditorScreen(
                    promptName = name,
                    onBack = { navController.popBackStack() }
                )
            }
            composable("new_conversation") {
                NewConversationScreen(
                    onBack = { navController.popBackStack() },
                    onStartConversation = { voice, prompt, context ->
                        // Create a new conversation in the database first
                        val conversationId = (System.currentTimeMillis() / 1000).toULong()
                        val dataDir = context.filesDir.absolutePath
                        createConversation(dataDir, conversationId)
                        // Pass voice and prompt as navigation arguments
                        navController.navigate("conversation/$conversationId/$voice/$prompt") {
                            popUpTo("conversations") { inclusive = false }
                        }
                    }
                )
            }
            composable("conversation/{id}") { backStackEntry ->
                val conversationId = backStackEntry.arguments?.getString("id")?.toULongOrNull() ?: 0UL
                ActiveConversationScreen(
                    conversationId = conversationId,
                    selectedVoice = "Fenrir",
                    selectedPrompt = "default",
                    onBack = { navController.popBackStack() }
                )
            }
            composable("conversation/{id}/{voice}/{prompt}") { backStackEntry ->
                val conversationId = backStackEntry.arguments?.getString("id")?.toULongOrNull() ?: 0UL
                val voice = backStackEntry.arguments?.getString("voice") ?: "Fenrir"
                val prompt = backStackEntry.arguments?.getString("prompt") ?: "default"
                ActiveConversationScreen(
                    conversationId = conversationId,
                    selectedVoice = voice,
                    selectedPrompt = prompt,
                    onBack = { navController.popBackStack() }
                )
            }
        }
    }
}
