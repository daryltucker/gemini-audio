package audio.gemini.app.ui.settings

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    onBack: () -> Unit,
    vm: SettingsViewModel = viewModel(),
) {
    val state by vm.uiState.collectAsState()

    LaunchedEffect(Unit) {
        vm.refreshPrompts()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // ── API Key ──────────────────────────────────────────────────────
            SectionHeader("API Key")
            OutlinedTextField(
                value = state.apiKey,
                onValueChange = { vm.setApiKey(it) },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("GEMINI_API_KEY") },
                singleLine = true,
                isError = state.apiKeyError != null,
                supportingText = {
                    if (state.apiKeyError != null) {
                        Text(
                            text = state.apiKeyError!!,
                            color = MaterialTheme.colorScheme.error,
                        )
                    } else {
                        Text(
                            text = "Your API key is stored locally on this device only.",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
                visualTransformation = if (state.apiKeyVisible) {
                    VisualTransformation.None
                } else {
                    PasswordVisualTransformation()
                },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                trailingIcon = {
                    IconButton(onClick = { vm.toggleApiKeyVisibility() }) {
                        Icon(
                            imageVector = if (state.apiKeyVisible) {
                                Icons.Filled.VisibilityOff
                            } else {
                                Icons.Filled.Visibility
                            },
                            contentDescription = if (state.apiKeyVisible) "Hide" else "Show",
                        )
                    }
                },
            )

            HorizontalDivider()

            // ── Recording ────────────────────────────────────────────────────
            SectionHeader("Recording")
            ToggleRow(
                label = "Save Audio",
                description = "Keep WAV recordings of AI responses",
                checked = state.saveAudio,
                onToggle = { vm.setSaveAudio(it) },
            )

            HorizontalDivider()

            // ── Display ──────────────────────────────────────────────────────
            SectionHeader("Display")
            ToggleRow(
                label = "Show Thinking",
                description = "Display the model's reasoning process",
                checked = state.showThinking,
                onToggle = { vm.setShowThinking(it) },
            )

            HorizontalDivider()

            // ── Voice ─────────────────────────────────────────────────────────
            SectionHeader("Default Voice")
            var voiceExpanded by remember { mutableStateOf(false) }
            val voiceTextField = remember { mutableStateOf(state.defaultVoice) }
            
            ExposedDropdownMenuBox(
                expanded = voiceExpanded,
                onExpandedChange = { voiceExpanded = it }
            ) {
                OutlinedTextField(
                    value = state.defaultVoice,
                    onValueChange = {},
                    readOnly = true,
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = voiceExpanded) },
                    modifier = Modifier
                        .menuAnchor()
                        .fillMaxWidth()
                )
                ExposedDropdownMenu(
                    expanded = voiceExpanded,
                    onDismissRequest = { voiceExpanded = false }
                ) {
                    state.availableVoices.forEach { voice ->
                        DropdownMenuItem(
                            text = { Text(voice) },
                            onClick = {
                                vm.setDefaultVoice(voice)
                                voiceExpanded = false
                            }
                        )
                    }
                }
            }

            HorizontalDivider()

            // ── Prompt ───────────────────────────────────────────────────────
            SectionHeader("Default Prompt")
            var promptExpanded by remember { mutableStateOf(false) }
            
            ExposedDropdownMenuBox(
                expanded = promptExpanded,
                onExpandedChange = { promptExpanded = it }
            ) {
                OutlinedTextField(
                    value = state.defaultPrompt,
                    onValueChange = {},
                    readOnly = true,
                    trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = promptExpanded) },
                    modifier = Modifier
                        .menuAnchor()
                        .fillMaxWidth()
                )
                ExposedDropdownMenu(
                    expanded = promptExpanded,
                    onDismissRequest = { promptExpanded = false }
                ) {
                    state.availablePrompts.forEach { prompt ->
                        DropdownMenuItem(
                            text = { Text(prompt) },
                            onClick = {
                                vm.setDefaultPrompt(prompt)
                                promptExpanded = false
                            }
                        )
                    }
                }
            }
            Text(
                text = "Select the default prompt for new conversations.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            HorizontalDivider()

            // Footer disclaimer
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    text = "This project is not affiliated with Google or the Gemini API.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    text = "github.com/daryltucker",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.primary.copy(alpha = 0.6f),
                )
            }
        }
    }
}

// ── Reusable components ──────────────────────────────────────────────────────

@Composable
private fun SectionHeader(title: String) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleMedium,
        color = MaterialTheme.colorScheme.primary,
    )
}

@Composable
private fun ToggleRow(
    label: String,
    description: String,
    checked: Boolean,
    onToggle: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(text = label, style = MaterialTheme.typography.bodyLarge)
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Switch(checked = checked, onCheckedChange = onToggle)
    }
}
