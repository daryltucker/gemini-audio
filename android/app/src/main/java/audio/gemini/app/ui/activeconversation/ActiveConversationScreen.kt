package audio.gemini.app.ui.activeconversation

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MicOff
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.foundation.layout.safeDrawingPadding

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActiveConversationScreen(
    conversationId: ULong,
    selectedVoice: String = "Fenrir",
    selectedPrompt: String = "default",
    onBack: () -> Unit,
    vm: ActiveConversationViewModel = viewModel(
        factory = ActiveConversationViewModelFactory(
            application = LocalContext.current.applicationContext as android.app.Application,
            initialVoice = selectedVoice,
            initialPrompt = selectedPrompt
        )
    ),
) {
    val state by vm.uiState.collectAsState()
    val lazyListState = rememberLazyListState()
    val context = LocalContext.current
    
    var hasAudioPermission by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == 
            PackageManager.PERMISSION_GRANTED
        )
    }
    
    val permissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { isGranted ->
        hasAudioPermission = isGranted
    }

    // Auto-scroll to bottom when new messages arrive
    LaunchedEffect(state.turns.size) {
        if (state.turns.isNotEmpty()) {
            lazyListState.animateScrollToItem(state.turns.size - 1)
        }
    }

    // Load conversation on mount
    LaunchedEffect(conversationId) {
        if (conversationId > 0UL) {
            vm.loadConversation(conversationId)
        }
    }
    
    fun handleToggleListening() {
        if (hasAudioPermission) {
            when {
                state.isListening -> {
                    // User was recording - stop recording, wait for Gemini's response
                    vm.stopListening()
                }
                state.isProcessing -> {
                    // Gemini was speaking - interrupt (barge-in)
                    vm.interruptAndStartListening()
                }
                else -> {
                    // Idle - start listening
                    vm.startListening()
                }
            }
        } else {
            permissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
        }
    }

    Scaffold(
        modifier = Modifier.safeDrawingPadding(),
        topBar = {
            TopAppBar(
                title = { Text("Conversation") },
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
        bottomBar = {
            ConversationControls(
                isListening = state.isListening,
                isProcessing = state.isProcessing,
                hasPermission = hasAudioPermission,
                onToggleListening = { handleToggleListening() },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            // Transcript area
            LazyColumn(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                state = lazyListState,
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                items(state.turns) { turn ->
                    TurnItem(turn)
                }
                
                // Live user transcript - show while actively listening OR processing (until turn is saved)
                // NOTE: Don't use isListening as the only condition - that makes transcript disappear
                // when user stops talking. Show if there's content AND we're in an active turn.
                if (state.userTranscript.isNotEmpty() && (state.isListening || state.isProcessing)) {
                    item {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.End,
                        ) {
                            Surface(
                                shape = RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp),
                                color = MaterialTheme.colorScheme.primary,
                            ) {
                                Text(
                                    text = state.userTranscript,
                                    color = MaterialTheme.colorScheme.onPrimary,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                                )
                            }
                        }
                    }
                }
                
                // Thinking (internal thought) - only show if enabled in settings
                if (state.showThinking && state.isProcessing && state.thinking.isNotEmpty()) {
                    item {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.Start,
                        ) {
                            Surface(
                                shape = RoundedCornerShape(16.dp, 16.dp, 16.dp, 4.dp),
                                color = MaterialTheme.colorScheme.tertiaryContainer,
                            ) {
                                Text(
                                    text = "Thinking: ${state.thinking}",
                                    color = MaterialTheme.colorScheme.onTertiaryContainer,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                                    fontStyle = FontStyle.Italic,
                                )
                            }
                        }
                    }
                }
                
                // Live assistant transcript (voice response) - show after thinking
                if (state.isProcessing && state.assistantTranscript.isNotEmpty()) {
                    item {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.Start,
                        ) {
                            Surface(
                                shape = RoundedCornerShape(16.dp, 16.dp, 16.dp, 4.dp),
                                color = MaterialTheme.colorScheme.surfaceVariant,
                            ) {
                                Text(
                                    text = state.assistantTranscript,
                                    color = MaterialTheme.colorScheme.onSurface,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                                )
                            }
                        }
                    }
                }

                if (state.turns.isEmpty() && state.userTranscript.isEmpty()) {
                    item {
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(top = 100.dp),
                            contentAlignment = Alignment.Center,
                        ) {
                            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                Text(
                                    text = "◆",
                                    style = MaterialTheme.typography.displayLarge,
                                    color = MaterialTheme.colorScheme.primary,
                                )
                                Spacer(Modifier.height(16.dp))
                                Text(
                                    text = "Start speaking",
                                    style = MaterialTheme.typography.titleLarge,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                                Spacer(Modifier.height(8.dp))
                                Text(
                                    text = "Tap the microphone to begin",
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }
            }

            // Error display
            state.error?.let { error ->
                Snackbar(
                    modifier = Modifier.padding(16.dp),
                    action = {
                        TextButton(onClick = { vm.clearError() }) {
                            Text("Dismiss")
                        }
                    },
                ) {
                    Text(error)
                }
            }
        }
    }
}

@Composable
private fun TurnItem(turn: ConversationTurn) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // User message
        if (turn.userText.isNotEmpty()) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                Surface(
                    shape = RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp),
                    color = MaterialTheme.colorScheme.primary,
                ) {
                    Text(
                        text = turn.userText,
                        color = MaterialTheme.colorScheme.onPrimary,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    )
                }
            }
        }

        // Assistant message
        if (turn.assistantText.isNotEmpty()) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.Start,
            ) {
                Surface(
                    shape = RoundedCornerShape(16.dp, 16.dp, 16.dp, 4.dp),
                    color = MaterialTheme.colorScheme.surfaceVariant,
                ) {
                    Text(
                        text = turn.assistantText,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    )
                }
            }
        }

        // Recording indicator
        if (turn.hasRecording) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                Icon(
                    Icons.Filled.PlayArrow,
                    contentDescription = "Recording available",
                    modifier = Modifier
                        .size(24.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.2f))
                        .padding(4.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
    }
}

@Composable
private fun ConversationControls(
    isListening: Boolean,
    isProcessing: Boolean,
    hasPermission: Boolean,
    onToggleListening: () -> Unit,
) {
    Surface(
        tonalElevation = 8.dp,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val buttonColor = when {
                !hasPermission -> MaterialTheme.colorScheme.tertiary
                isListening -> MaterialTheme.colorScheme.error
                isProcessing -> MaterialTheme.colorScheme.secondary
                else -> MaterialTheme.colorScheme.primary
            }

            val icon = when {
                !hasPermission -> Icons.Filled.MicOff
                isListening -> Icons.Filled.Mic
                isProcessing -> Icons.Filled.Stop
                else -> Icons.Filled.Mic
            }

            val contentDescription = when {
                !hasPermission -> "Grant microphone permission"
                isListening -> "Stop listening"
                isProcessing -> "Processing..."
                else -> "Start listening"
            }

            FloatingActionButton(
                onClick = onToggleListening,
                containerColor = buttonColor,
            ) {
                if (isProcessing) {
                    CircularProgressIndicator(
                        color = MaterialTheme.colorScheme.onPrimary,
                        modifier = Modifier.size(24.dp),
                    )
                } else {
                    Icon(icon, contentDescription = contentDescription)
                }
            }

            Spacer(Modifier.width(16.dp))

            Text(
                text = when {
                    !hasPermission -> "Tap to grant microphone permission"
                    isListening -> "Listening..."
                    isProcessing -> "Processing..."
                    else -> "Tap to speak"
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
