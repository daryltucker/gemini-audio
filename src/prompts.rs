// Prompt management system
//
// Search order:
//   1. ./prompts/<id>.md                        (cwd-local, takes precedence)
//   2. ~/.config/gemini-audio/prompts/<id>.md  (user config, fallback)
//
// On first run, "default" is created in the user config dir if it doesn't exist.

use crate::error::{GeminiAudioError, Result};
use std::path::PathBuf;
use std::fs;

const DEFAULT_PROMPT_CONTENT: &str = "\
You are a helpful voice assistant. Respond conversationally and concisely. \
Always speak your answer aloud — never respond silently.\
";

/// Manages prompts from the user config dir, with a bundled fallback.
pub struct PromptManager {
    /// Primary: ~/.config/gemini-audio/prompts/
    user_dir: PathBuf,
    /// Fallback: ./prompts/ relative to cwd (for dev / repo use)
    bundled_dir: PathBuf,
}

impl PromptManager {
    /// Create a new prompt manager.
    ///
    /// `user_dir`    — `~/.config/gemini-audio/prompts/`
    /// `bundled_dir` — `./prompts/` (cwd-relative)
    pub fn new(user_dir: PathBuf, bundled_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&user_dir)
            .map_err(|e| GeminiAudioError::Configuration(
                format!("Failed to create user prompts directory: {}", e)
            ))?;

        Ok(Self { user_dir, bundled_dir })
    }

    /// Ensure `default.md` exists in the user config dir.
    /// Creates it with a starter prompt if missing. Called once at startup.
    pub fn ensure_default(&self) -> Result<()> {
        let default_file = self.user_dir.join("default.md");
        if !default_file.exists() {
            fs::write(&default_file, DEFAULT_PROMPT_CONTENT)
                .map_err(|e| GeminiAudioError::FileIO(
                    format!("Failed to create default prompt: {}", e)
                ))?;
        }
        Ok(())
    }

    /// Load a prompt by ID.
    ///
    /// Searches user dir first, then bundled dir.
    pub fn load_prompt(&self, prompt_id: &str) -> Result<String> {
        // Reject any path separators — prompt IDs are plain names only.
        if prompt_id.contains('/') || prompt_id.contains('\\') || prompt_id.contains("..") {
            return Err(GeminiAudioError::InvalidInput(
                format!("Invalid prompt ID '{}': must be a plain name with no path components", prompt_id)
            ));
        }

        let filename = format!("{}.md", prompt_id);

        // Local ./prompts/ takes precedence — lets a project directory override user defaults.
        let bundled_path = self.bundled_dir.join(&filename);
        if bundled_path.exists() {
            let content = fs::read_to_string(&bundled_path)
                .map_err(|e| GeminiAudioError::FileIO(
                    format!("Failed to read prompt '{}': {}", prompt_id, e)
                ))?;
            return Ok(content.trim().to_string());
        }

        // Fall back to user config dir (~/.config/gemini-audio/prompts/).
        let user_path = self.user_dir.join(&filename);
        if user_path.exists() {
            let content = fs::read_to_string(&user_path)
                .map_err(|e| GeminiAudioError::FileIO(
                    format!("Failed to read prompt '{}': {}", prompt_id, e)
                ))?;
            return Ok(content.trim().to_string());
        }

        Err(GeminiAudioError::InvalidInput(
            format!("Prompt '{}' not found (checked {} and {})",
                prompt_id,
                bundled_path.display(),
                user_path.display(),
            )
        ))
    }

    /// List all available prompt IDs (user dir + bundled, deduplicated, sorted).
    pub fn list_prompts(&self) -> Result<Vec<String>> {
        let mut seen = std::collections::HashSet::new();
        let mut prompts = Vec::new();

        for dir in [&self.user_dir, &self.bundled_dir] {
            if !dir.exists() {
                continue;
            }
            let entries = fs::read_dir(dir)
                .map_err(|e| GeminiAudioError::FileIO(
                    format!("Failed to read prompts directory: {}", e)
                ))?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if seen.insert(stem.to_string()) {
                            prompts.push(stem.to_string());
                        }
                    }
                }
            }
        }

        prompts.sort();
        Ok(prompts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_manager() -> (TempDir, TempDir, PromptManager) {
        let user_tmp = TempDir::new().unwrap();
        let bundled_tmp = TempDir::new().unwrap();
        let mgr = PromptManager::new(
            user_tmp.path().to_path_buf(),
            bundled_tmp.path().to_path_buf(),
        ).unwrap();
        (user_tmp, bundled_tmp, mgr)
    }

    #[test]
    fn test_ensure_default_creates_file() {
        let (user_tmp, _bundled, mgr) = make_manager();
        mgr.ensure_default().unwrap();
        assert!(user_tmp.path().join("default.md").exists());
        let content = mgr.load_prompt("default").unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_ensure_default_idempotent() {
        let (_user, _bundled, mgr) = make_manager();
        mgr.ensure_default().unwrap();
        mgr.ensure_default().unwrap(); // second call must not error
    }

    #[test]
    fn test_bundled_dir_takes_priority() {
        let (user_tmp, bundled_tmp, mgr) = make_manager();
        fs::write(user_tmp.path().join("foo.md"), "user version").unwrap();
        fs::write(bundled_tmp.path().join("foo.md"), "bundled version").unwrap();
        assert_eq!(mgr.load_prompt("foo").unwrap(), "bundled version");
    }

    #[test]
    fn test_user_fallback() {
        let (user_tmp, _bundled, mgr) = make_manager();
        fs::write(user_tmp.path().join("bar.md"), "user only").unwrap();
        assert_eq!(mgr.load_prompt("bar").unwrap(), "user only");
    }

    #[test]
    fn test_path_traversal_rejected() {
        let (_user, _bundled, mgr) = make_manager();
        assert!(mgr.load_prompt("../etc/passwd").is_err());
        assert!(mgr.load_prompt("foo/bar").is_err());
    }

    #[test]
    fn test_list_prompts_deduplicates() {
        let (user_tmp, bundled_tmp, mgr) = make_manager();
        fs::write(user_tmp.path().join("alpha.md"), "a").unwrap();
        fs::write(bundled_tmp.path().join("alpha.md"), "a-bundled").unwrap();
        fs::write(bundled_tmp.path().join("beta.md"), "b").unwrap();
        let list = mgr.list_prompts().unwrap();
        assert_eq!(list, vec!["alpha", "beta"]);
    }
}
