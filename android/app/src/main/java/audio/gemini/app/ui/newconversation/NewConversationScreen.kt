package audio.gemini.app.ui.newconversation

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewConversationScreen(
    onBack: () -> Unit,
    onStartConversation: (voice: String, prompt: String, context: android.content.Context) -> Unit,
    vm: NewConversationViewModel = viewModel(),
) {
    val state by vm.uiState.collectAsState()
    var voiceExpanded by remember { mutableStateOf(false) }
    var promptExpanded by remember { mutableStateOf(false) }
    val context = LocalContext.current

    // Refresh prompts and voices when screen becomes visible
    LaunchedEffect(Unit) {
        vm.refresh()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("New Conversation") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
            )
        },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = { onStartConversation(state.selectedVoice, state.selectedPrompt, context) },
                text = { Text("Start") },
                icon = {}
            )
        },
    ) { padding ->
        if (state.isLoading) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center,
            ) {
                CircularProgressIndicator()
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(24.dp),
            ) {
                // ── Voice Selection ─────────────────────────────────────────────
                ExposedDropdownMenuBox(
                    expanded = voiceExpanded,
                    onExpandedChange = { voiceExpanded = it }
                ) {
                    OutlinedTextField(
                        value = state.selectedVoice,
                        onValueChange = {},
                        readOnly = true,
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = voiceExpanded) },
                        modifier = Modifier
                            .menuAnchor()
                            .fillMaxWidth(),
                        label = { Text("Voice") }
                    )
                    ExposedDropdownMenu(
                        expanded = voiceExpanded,
                        onDismissRequest = { voiceExpanded = false }
                    ) {
                        state.availableVoices.forEach { voice ->
                            DropdownMenuItem(
                                text = { Text(voice) },
                                onClick = {
                                    vm.selectVoice(voice)
                                    voiceExpanded = false
                                }
                            )
                        }
                    }
                }

                // ── Prompt Selection ────────────────────────────────────────────
                ExposedDropdownMenuBox(
                    expanded = promptExpanded,
                    onExpandedChange = { promptExpanded = it }
                ) {
                    OutlinedTextField(
                        value = state.selectedPrompt,
                        onValueChange = {},
                        readOnly = true,
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = promptExpanded) },
                        modifier = Modifier
                            .menuAnchor()
                            .fillMaxWidth(),
                        label = { Text("Prompt") }
                    )
                    ExposedDropdownMenu(
                        expanded = promptExpanded,
                        onDismissRequest = { promptExpanded = false }
                    ) {
                        state.availablePrompts.forEach { prompt ->
                            DropdownMenuItem(
                                text = { Text(prompt) },
                                onClick = {
                                    vm.selectPrompt(prompt)
                                    promptExpanded = false
                                }
                            )
                        }
                    }
                }

                Spacer(Modifier.weight(1f))
            }
        }
    }
}
