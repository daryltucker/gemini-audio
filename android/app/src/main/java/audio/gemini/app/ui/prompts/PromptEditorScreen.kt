package audio.gemini.app.ui.prompts

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PromptEditorScreen(
    promptName: String?,  // null for new prompt
    onBack: () -> Unit,
    vm: PromptViewModel = viewModel(),
) {
    var name by remember { mutableStateOf(promptName ?: "") }
    var content by remember { mutableStateOf("") }
    var isBundled by remember { mutableStateOf(false) }
    var isEditing by remember { mutableStateOf(promptName != null) }
    var showSaveError by remember { mutableStateOf(false) }
    var isLoading by remember { mutableStateOf(promptName != null) }

    // Load prompt content when editing existing prompt
    LaunchedEffect(promptName) {
        if (promptName != null) {
            vm.loadPromptContent(promptName) { loadedContent, bundled ->
                content = loadedContent
                isBundled = bundled
                isLoading = false
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (isEditing && !isLoading) "Edit Prompt" else "New Prompt") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    TextButton(
                        onClick = {
                            if (name.isBlank()) {
                                showSaveError = true
                                return@TextButton
                            }
                            if (isEditing) {
                                vm.updatePrompt(name, content)
                            } else {
                                vm.createPrompt(name, content)
                            }
                            onBack()
                        },
                        enabled = name.isNotBlank() && !isLoading,
                    ) {
                        Text("Save")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
            )
        }
    ) { padding ->
        if (isLoading) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding),
                contentAlignment = Alignment.Center,
            ) {
                CircularProgressIndicator()
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("Prompt Name") },
                    singleLine = true,
                    isError = showSaveError && name.isBlank(),
                    enabled = !isBundled,  // Can't edit name of bundled prompts
                    supportingText = if (showSaveError && name.isBlank()) {
                        { Text("Name is required", color = MaterialTheme.colorScheme.error) }
                    } else if (isBundled) {
                        { Text("Bundled prompts cannot be renamed", color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    } else {
                        null
                    },
                )

                OutlinedTextField(
                    value = content,
                    onValueChange = { content = it },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(300.dp),
                    label = { Text("Prompt Content (Markdown)") },
                    enabled = !isBundled,  // Can't edit bundled prompts
                    supportingText = {
                        Text("Use Markdown formatting for your prompt.")
                    },
                )

                // Show warning for bundled prompts
                if (isBundled && isEditing) {
                    Card(
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.surfaceVariant,
                        ),
                    ) {
                        Text(
                            text = "This is a bundled prompt and cannot be modified. You can copy it and create a new prompt with your changes.",
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.padding(16.dp),
                        )
                    }
                }
            }
        }
    }
}
