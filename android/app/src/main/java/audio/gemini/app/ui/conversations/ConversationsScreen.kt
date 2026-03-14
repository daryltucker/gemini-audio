package audio.gemini.app.ui.conversations

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Code
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import java.text.SimpleDateFormat
import java.util.*

/**
 * Main screen: list of past conversations with a FAB to start a new one.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConversationsScreen(
    onSettingsClick: () -> Unit,
    onPromptsClick: () -> Unit,
    onNewConversation: () -> Unit,
    onConversationClick: (ULong) -> Unit,
    vm: ConversationsViewModel = viewModel(),
) {
    val state by vm.uiState.collectAsState()
    var showDeleteDialog by remember { mutableStateOf<ULong?>(null) }
    var showMenu by remember { mutableStateOf(false) }
    
    // Refresh conversations when screen becomes visible
    LaunchedEffect(Unit) {
        vm.loadConversations()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Gemini Audio") },
                actions = {
                    Box {
                        IconButton(onClick = { showMenu = true }) {
                            Icon(Icons.Filled.Settings, contentDescription = "Menu")
                        }
                        DropdownMenu(
                            expanded = showMenu,
                            onDismissRequest = { showMenu = false }
                        ) {
                            DropdownMenuItem(
                                text = { Text("Settings") },
                                onClick = {
                                    showMenu = false
                                    onSettingsClick()
                                },
                                leadingIcon = {
                                    Icon(Icons.Filled.Settings, contentDescription = null)
                                }
                            )
                            DropdownMenuItem(
                                text = { Text("Prompts") },
                                onClick = {
                                    showMenu = false
                                    onPromptsClick()
                                },
                                leadingIcon = {
                                    Icon(Icons.Filled.Code, contentDescription = null)
                                }
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
            )
        },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = onNewConversation,
                icon = { Icon(Icons.Filled.Add, contentDescription = "New") },
                text = { Text("New") },
            )
        },
    ) { padding ->
        when {
            state.isLoading -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    CircularProgressIndicator()
                }
            }
            state.error != null -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(
                            text = "Error loading conversations",
                            color = MaterialTheme.colorScheme.error,
                        )
                        TextButton(onClick = { vm.loadConversations() }) {
                            Text("Retry")
                        }
                    }
                }
            }
            state.conversations.isEmpty() -> {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(padding),
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
                            text = "No conversations yet",
                            style = MaterialTheme.typography.titleLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(8.dp))
                        Text(
                            text = "Tap New to start talking with Gemini",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            textAlign = TextAlign.Center,
                        )
                    }
                }
            }
            else -> {
                LazyColumn(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(padding),
                    contentPadding = PaddingValues(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(state.conversations) { conversation ->
                        ConversationItem(
                            conversation = conversation,
                            onClick = { onConversationClick(conversation.id) },
                            onDelete = { showDeleteDialog = conversation.id },
                        )
                    }
                }
            }
        }

        // Delete confirmation dialog
        showDeleteDialog?.let { id ->
            AlertDialog(
                onDismissRequest = { showDeleteDialog = null },
                title = { Text("Delete Conversation?") },
                text = { Text("Are you sure you want to delete this conversation? This cannot be undone.") },
                confirmButton = {
                    TextButton(
                        onClick = {
                            vm.deleteConversation(id)
                            showDeleteDialog = null
                        },
                    ) {
                        Text("Delete", color = MaterialTheme.colorScheme.error)
                    }
                },
                dismissButton = {
                    TextButton(onClick = { showDeleteDialog = null }) {
                        Text("Cancel")
                    }
                },
            )
        }
    }
}

@Composable
private fun ConversationItem(
    conversation: ConversationItem,
    onClick: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = formatTimestamp(conversation.timestamp),
                    style = MaterialTheme.typography.titleMedium,
                )
                Text(
                    text = conversation.preview,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = "${conversation.turnCount} turn${if (conversation.turnCount != 1) "s" else ""}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            IconButton(onClick = onDelete) {
                Icon(Icons.Filled.Delete, contentDescription = "Delete")
            }
        }
    }
}

private fun formatTimestamp(timestamp: String): String {
    return try {
        // Parse ISO 8601 format: "2026-03-13T10:30:00Z"
        val inputFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.getDefault())
        val outputFormat = SimpleDateFormat("MMM d, yyyy 'at' h:mm a", Locale.getDefault())
        val date = inputFormat.parse(timestamp)
        date?.let { outputFormat.format(it) } ?: timestamp
    } catch (e: Exception) {
        timestamp
    }
}
