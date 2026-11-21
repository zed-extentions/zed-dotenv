//! Secret classification, sensitivity heuristics, encrypted values and masking (FR-3, FR-10, FR-19).

/// Checks whether a key or value represents an encrypted `dotenvx` / `.env.vault` ciphertext (FR-19).
pub fn is_encrypted_value(key: &str, value: &str) -> bool {
    let trimmed = value.trim();
    let upper_key = key.to_ascii_uppercase();
    trimmed.starts_with("encrypted:")
        || trimmed.starts_with("vlt_")
        || upper_key.starts_with("DOTENV_VAULT_")
        || upper_key == "DOTENV_VAULT"
}

/// Checks whether a variable value is an explicit bracketed placeholder (`<YOUR_KEY>`, `${VAR}`).
fn is_bracket_placeholder(trimmed: &str) -> bool {
    (trimmed.starts_with('<') && trimmed.ends_with('>'))
        || (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with("${") && trimmed.ends_with('}'))
}

/// Checks whether a variable value appears to be a non-sensitive placeholder in `.env.example`.
pub fn is_placeholder_value(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || is_bracket_placeholder(trimmed) {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    const PLACEHOLDER_TOKENS: &[&str] = &[
        "your_",
        "your-",
        "changeme",
        "change_me",
        "change-me",
        "placeholder",
        "replace_me",
        "replace-me",
        "sample",
        "dummy",
        "todo",
        "tbd",
        "xxx",
        "***",
        "<your",
        "insert_",
        "none",
        "null",
    ];
    PLACEHOLDER_TOKENS.iter().any(|tok| lower.contains(tok))
}

/// Detects well-known live credential / secret token formats in plaintext (FR-10, `DOTENV006`).
pub fn classify_secret_pattern(key: &str, value: &str) -> Option<&'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() || is_bracket_placeholder(trimmed) || is_encrypted_value(key, value) {
        return None;
    }

    // 1. PEM Private Key header
    if trimmed.contains("-----BEGIN ") && trimmed.contains("PRIVATE KEY-----") {
        return Some("PEM Private Key");
    }

    // 2. AWS Access Key ID (AKIA / ASIA + 16 uppercase alphanumeric characters)
    if (trimmed.starts_with("AKIA") || trimmed.starts_with("ASIA"))
        && trimmed.len() == 20
        && trimmed
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Some("AWS Access Key ID (AKIA/ASIA)");
    }

    if is_placeholder_value(trimmed) {
        return None;
    }

    // 3. GitHub Personal / App / Fine-grained tokens
    if (trimmed.starts_with("ghp_")
        || trimmed.starts_with("gho_")
        || trimmed.starts_with("ghu_")
        || trimmed.starts_with("ghs_")
        || trimmed.starts_with("ghr_")
        || trimmed.starts_with("github_pat_"))
        && trimmed.len() >= 16
    {
        return Some("GitHub Access Token");
    }

    // 4. OpenAI / Anthropic / Stripe secret keys (`sk-...`, `sk_live_...`, `rk_live_...`)
    if (trimmed.starts_with("sk-")
        || trimmed.starts_with("sk_live_")
        || trimmed.starts_with("rk_live_"))
        && trimmed.len() >= 16
    {
        return Some("API Secret Key (sk-*/sk_live_*)");
    }

    // 5. Slack tokens (`xoxb-`, `xoxp-`, `xoxa-`, `xoxr-`, `xoxs-`)
    if (trimmed.starts_with("xoxb-")
        || trimmed.starts_with("xoxp-")
        || trimmed.starts_with("xoxa-")
        || trimmed.starts_with("xoxr-")
        || trimmed.starts_with("xoxs-"))
        && trimmed.len() >= 16
    {
        return Some("Slack Token (xox*)");
    }

    // 6. Google API Key (`AIza` + 35 chars)
    if trimmed.starts_with("AIza") && trimmed.len() >= 35 {
        return Some("Google API Key (AIza*)");
    }

    // 7. Database URL with embedded password (`postgres://user:pass@host`)
    let lower_key = key.to_ascii_lowercase();
    if (lower_key.ends_with("_url") || lower_key.ends_with("_uri") || lower_key.contains("dsn"))
        && trimmed.contains("://")
    {
        if let Some(after_scheme) = trimmed.split("://").nth(1) {
            if let Some((userinfo, _host)) = after_scheme.split_once('@') {
                if let Some((_user, pass)) = userinfo.split_once(':') {
                    if !pass.is_empty()
                        && !is_placeholder_value(pass)
                        && pass != "postgres"
                        && pass != "password"
                        && pass != "root"
                    {
                        return Some("Connection URI with embedded credentials");
                    }
                }
            }
        }
    }

    None
}

/// Returns `true` if `key` name indicates a sensitive secret, token, password, or credential.
pub fn is_sensitive_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    const SENSITIVE_MARKERS: &[&str] = &[
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "PASSWD",
        "PWD",
        "PRIVATE",
        "CREDENTIAL",
        "API_KEY",
        "APIKEY",
        "ACCESS_KEY",
        "AUTH",
        "JWT",
        "SESSION",
        "COOKIE",
        "WEBHOOK",
        "SIGNING",
        "ENCRYPTION",
        "CERT",
        "DATABASE_URL",
        "REDIS_URL",
        "MONGO_URI",
        "DSN",
    ];
    SENSITIVE_MARKERS.iter().any(|m| upper.contains(m)) || upper.ends_with("_KEY")
}

/// Returns `true` if `value` is a localhost URL or loopback host without embedded `@` credentials.
fn is_credential_free_localhost(lower: &str) -> bool {
    if lower.contains('@') {
        return false;
    }
    if matches!(lower, "localhost" | "127.0.0.1" | "0.0.0.0" | "::1") {
        return true;
    }
    if let Some((_scheme, rest)) = lower.split_once("://") {
        return rest.starts_with("localhost")
            || rest.starts_with("127.0.0.1")
            || rest.starts_with("0.0.0.0");
    }
    false
}

/// Returns `true` if a key-value pair is a safe non-secret configuration default
/// that can be preserved verbatim when generating `.env.example` (FR-12, FR-19).
pub fn is_safe_default_value(key: &str, value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || is_placeholder_value(trimmed) || is_encrypted_value(key, trimmed) {
        return true;
    }
    if classify_secret_pattern(key, trimmed).is_some() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    // Credential-free localhost URLs (e.g. `postgres://localhost:5432/app`, `http://localhost:3000`)
    if is_credential_free_localhost(&lower) {
        return true;
    }
    if is_sensitive_key(key) {
        return false;
    }
    // Booleans
    if matches!(
        lower.as_str(),
        "true" | "false" | "0" | "1" | "yes" | "no" | "on" | "off"
    ) {
        return true;
    }
    // Numeric ports / timeouts / counts
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Standard environment / log level constants
    matches!(
        lower.as_str(),
        "development"
            | "production"
            | "staging"
            | "test"
            | "testing"
            | "local"
            | "debug"
            | "info"
            | "warn"
            | "warning"
            | "error"
            | "trace"
            | "utf-8"
            | "utf8"
            | "utc"
    )
}

/// Masks a value so raw secret plaintext is never exposed in Assistant outputs (FR-3).
pub fn mask_value(value: &str, reveal_edges: bool) -> String {
    if value.is_empty() {
        return "(empty)".to_string();
    }
    let char_count = value.chars().count();
    if reveal_edges && char_count >= 8 {
        let chars: Vec<char> = value.chars().collect();
        let prefix: String = chars[..2].iter().collect();
        let suffix: String = chars[char_count - 2..].iter().collect();
        format!("{prefix}***{suffix}")
    } else {
        "***".to_string()
    }
}

/// Formats a hover value according to settings while keeping secrets masked.
/// Only known safe defaults can be exposed when masking is disabled.
#[allow(dead_code)] // Used by the native LSP; the same domain module is compiled into WASM.
pub fn hover_value(key: &str, value: &str, settings: &crate::DotenvSettings) -> String {
    let safe = !is_sensitive_key(key)
        && classify_secret_pattern(key, value).is_none()
        && !is_encrypted_value(key, value)
        && is_safe_default_value(key, value);
    if safe && !settings.mask_values {
        if value.is_empty() {
            "(empty)".to_string()
        } else {
            value.to_string()
        }
    } else {
        mask_value(value, safe && settings.reveal_edges)
    }
}
