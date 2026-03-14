package audio.gemini.app.data.models

data class Prompt(
    val name: String,
    val content: String,
    val isBundled: Boolean,  // Read-only if true
) {
    // For display in dropdowns
    val displayName: String
        get() = if (isBundled) "$name (bundled)" else name
}
