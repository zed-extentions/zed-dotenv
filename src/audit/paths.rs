//! Path and key helpers shared by the audit, report, scan and settings modules.

use super::types::DEFAULT_ENV_PATH;

/// Normalizes a worktree-relative path (converts `\` to `/` and strips leading `./` or `/`).
pub fn normalize_worktree_path(raw: &str) -> String {
    let slash = raw.trim().replace('\\', "/");
    let stripped = slash
        .strip_prefix("./")
        .or_else(|| slash.strip_prefix('/'))
        .unwrap_or(&slash);
    if stripped.is_empty() {
        DEFAULT_ENV_PATH.to_string()
    } else {
        stripped.to_string()
    }
}

/// Returns `true` if `path` looks like a template/example file (`.env.example`, `.env.sample`, `.env.template`).
pub fn is_template_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".example")
        || lower.ends_with(".sample")
        || lower.ends_with(".template")
        || lower.contains(".env.example")
        || lower.contains(".env.sample")
        || lower.contains(".env.template")
}

/// Validates whether `key` is a legal POSIX/dotenv variable identifier (`[A-Za-z_][A-Za-z0-9_]*`).
pub fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
