// Capability detection and caching for Gemini API response modalities.
//
// On first use with Auto mode, probes TEXT+AUDIO; on rejection falls back to AUDIO-only.
// Results are cached in ~/.config/gemini-audio/capabilities.json.
//
// Cache file is human-readable JSON — entries are keyed by a hash but contain enough
// context (key_hint + model_id) that you can identify and delete individual entries:
//
//   {
//     "a3f2b1c4d8e9a1b2": {
//       "key_hint": "AIza...xyz1",
//       "model_id": "models/gemini-2.5-flash-native-audio-preview-12-2025",
//       "supports_text": false,
//       "reason": "modality_not_supported",
//       "detected_at": "2026-03-08T12:00:00+00:00"
//     }
//   }

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;

// ── OutputTextMode ────────────────────────────────────────────────────────────

/// Controls whether TEXT modality is attempted, driven by `GEMINI_OUTPUT_TEXT`.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputTextMode {
    /// Default (env unset or `=1`): probe API once, cache result, auto-fallback.
    Auto,
    /// `GEMINI_OUTPUT_TEXT=0` or `GEMINI_OUTPUT_TEXT=`: force AUDIO-only, skip probe.
    ForceAudio,
}

impl OutputTextMode {
    pub fn from_env() -> Self {
        match std::env::var("GEMINI_OUTPUT_TEXT").as_deref() {
            Ok("0") | Ok("") => Self::ForceAudio,
            _ => Self::Auto,
        }
    }
}

// ── Cache types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityEntry {
    /// Human-readable key hint: first 4 + "..." + last 4 chars of the API key.
    /// Lets you identify which key this entry belongs to without storing the key itself.
    pub key_hint: String,
    /// Full model ID — useful when you have multiple entries across models.
    pub model_id: String,
    /// Whether this key+model supports `responseModalities: [AUDIO, TEXT]`.
    pub supports_text: bool,
    /// Why `supports_text` is false, if applicable.
    /// Values: "probed_ok" | "modality_not_supported" | "tier_restriction" | "force_audio"
    pub reason: String,
    /// RFC 3339 timestamp of when this entry was written.
    pub detected_at: String,
}

pub type CapabilityCache = HashMap<String, CapabilityEntry>;

// ── Hashing ───────────────────────────────────────────────────────────────────

/// FNV-1a 64-bit hash — deterministic across runs, no extra dependency.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

/// Stable cache key for a given API key + model ID combination.
pub fn cache_key(api_key: &str, model_id: &str) -> String {
    format!("{:016x}", fnv1a(&format!("{}:{}", api_key, model_id)))
}

/// Human-readable hint: first 4 chars + "..." + last 4 chars.
pub fn key_hint(api_key: &str) -> String {
    let n = api_key.len();
    if n <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &api_key[..4], &api_key[n - 4..])
}

// ── Env helpers ───────────────────────────────────────────────────────────────

/// Read the active API key from the environment (same precedence as client.rs).
pub fn current_api_key() -> Option<String> {
    std::env::var("GEMINI_API_KEY").ok().filter(|k| !k.is_empty())
}

pub fn current_model_id() -> String {
    std::env::var("GEMINI_MODEL_ID")
        .unwrap_or_else(|_| crate::config::MODEL_ID.to_string())
}

/// Returns `(cache_key, key_hint, model_id)` for the current environment, or None if no key.
pub fn current_cache_coords() -> Option<(String, String, String)> {
    let key = current_api_key()?;
    let model = current_model_id();
    Some((cache_key(&key, &model), key_hint(&key), model))
}

// ── Disk I/O ──────────────────────────────────────────────────────────────────

fn cache_path() -> Option<PathBuf> {
    directories_next::BaseDirs::new()
        .map(|b| b.config_dir().join("gemini-audio").join("capabilities.json"))
}

pub fn load_cache() -> CapabilityCache {
    cache_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_cache(cache: &CapabilityCache) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn write_cache_entry(ck: &str, hint: &str, model: &str, supports_text: bool, reason: &str) {
    let mut cache = load_cache();
    cache.insert(ck.to_string(), CapabilityEntry {
        key_hint: hint.to_string(),
        model_id: model.to_string(),
        supports_text,
        reason: reason.to_string(),
        detected_at: chrono::Utc::now().to_rfc3339(),
    });
    save_cache(&cache);
    info!(
        key_hint = %hint,
        model = %model,
        supports_text,
        reason,
        "Cached capability result"
    );
}

// ── Error classification ──────────────────────────────────────────────────────

/// Returns true if a JSON error frame (code + message) indicates TEXT modality was rejected —
/// either due to account tier or model capability. Both cases fall back to AUDIO-only.
///
/// Patterns are intentionally liberal. Unknown errors are NOT matched — those
/// propagate normally and do not poison the capability cache.
pub fn is_modality_error(code: i32, message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("modali")
        || msg.contains("response_modalities")
        || msg.contains("text modality")
        || (code == 400 && msg.contains("not support"))
        || (code == 400 && msg.contains("unsupported"))
        || (code == 403 && msg.contains("modali"))
}

/// Returns true if a WebSocket-level error (close frame or transport error) indicates
/// TEXT modality rejection during setup. The server sometimes closes the connection at the
/// protocol level with "invalid argument" rather than sending a JSON error body.
///
/// Only applied when we are actively probing (mode_from_cache.is_none()), so the
/// "invalid argument" match is safe — it won't mask unrelated errors during normal operation.
pub fn is_modality_ws_error(error_debug: &str) -> bool {
    let s = error_debug.to_lowercase();
    s.contains("invalid argument")
        || s.contains("modali")
        || s.contains("response_modalities")
}

/// Classify the error into a human-readable cache `reason` string.
pub fn modality_error_reason(code: i32, message: &str) -> &'static str {
    let msg = message.to_lowercase();
    if code == 403 || msg.contains("quota") || msg.contains("billing") || msg.contains("tier") {
        "tier_restriction"
    } else {
        "modality_not_supported"
    }
}

/// Determine what modalities to request, consulting the cache.
///
/// Returns `(modalities, known_supports_text)`:
/// - `modalities` — the Vec to pass to setup
/// - `known_supports_text` — `Some(bool)` if resolved from cache or env, `None` if a probe is needed
pub fn resolve_modalities(
    mode: &OutputTextMode,
) -> (Vec<String>, Option<bool>) {
    if *mode == OutputTextMode::ForceAudio {
        return (vec!["AUDIO".to_string()], Some(false));
    }

    let Some((ck, _hint, _model)) = current_cache_coords() else {
        // No API key in env — try AUDIO+TEXT anyway, let the server reject it
        return (vec!["AUDIO".to_string(), "TEXT".to_string()], None);
    };

    let cache = load_cache();
    if let Some(entry) = cache.get(&ck) {
        let mods = if entry.supports_text {
            vec!["AUDIO".to_string(), "TEXT".to_string()]
        } else {
            vec!["AUDIO".to_string()]
        };
        return (mods, Some(entry.supports_text));
    }

    // Cache miss — probe with TEXT+AUDIO
    (vec!["AUDIO".to_string(), "TEXT".to_string()], None)
}
