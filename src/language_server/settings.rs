//! JSONC settings resolution and language-server command resolution (FR-1, FR-14).

use std::collections::HashSet;

use super::paths::normalize_worktree_path;
use super::types::{
    DotenvSettings, ServerCommandSpec, EXAMPLE_CANDIDATE_NAMES, FALLBACK_SERVER_BINARIES,
    LANGUAGE_SERVER_ID,
};

/// Returns ordered worktree-relative candidate paths for `.env.example` discovery (FR-2).
pub fn example_candidates(env_path: &str, configured_example: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    let mut push_unique = |path: String| {
        let normalized = normalize_worktree_path(&path);
        if seen.insert(normalized.clone()) {
            candidates.push(normalized);
        }
    };

    if let Some(custom) = configured_example {
        if !custom.trim().is_empty() {
            push_unique(custom.to_string());
        }
    }

    let normalized_env = normalize_worktree_path(env_path);
    if let Some((dir, _file)) = normalized_env.rsplit_once('/') {
        for &name in EXAMPLE_CANDIDATE_NAMES {
            push_unique(format!("{dir}/{name}"));
        }
    }

    for &name in EXAMPLE_CANDIDATE_NAMES {
        push_unique(name.to_string());
    }

    candidates
}

/// Strips `//` line comments, `/* ... */` block comments, and trailing commas from JSONC text (FR-14).
pub fn strip_jsonc(input: &str) -> String {
    let mut without_comments = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(ch) = chars.next() {
        if in_string {
            without_comments.push(ch);
            if ch == '\\' {
                if let Some(escaped) = chars.next() {
                    without_comments.push(escaped);
                }
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            without_comments.push(ch);
            continue;
        }

        if ch == '/' {
            if let Some(&'/') = chars.peek() {
                let _ = chars.next();
                for next_ch in chars.by_ref() {
                    if next_ch == '\n' {
                        without_comments.push('\n');
                        break;
                    }
                }
                continue;
            } else if let Some(&'*') = chars.peek() {
                let _ = chars.next();
                while let Some(next_ch) = chars.next() {
                    if next_ch == '*' && chars.peek() == Some(&'/') {
                        let _ = chars.next();
                        break;
                    }
                }
                continue;
            }
        }

        without_comments.push(ch);
    }

    // Remove trailing commas before `}` or `]` outside strings
    let mut out = String::with_capacity(without_comments.len());
    let mut iter = without_comments.chars().peekable();
    let mut in_str = false;

    while let Some(ch) = iter.next() {
        if in_str {
            out.push(ch);
            if ch == '\\' {
                if let Some(escaped) = iter.next() {
                    out.push(escaped);
                }
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }

        if ch == '"' {
            in_str = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = iter.clone();
            let mut is_trailing = false;
            while let Some(&next_ch) = lookahead.peek() {
                if next_ch.is_whitespace() {
                    let _ = lookahead.next();
                } else {
                    if next_ch == '}' || next_ch == ']' {
                        is_trailing = true;
                    }
                    break;
                }
            }
            if is_trailing {
                continue;
            }
        }

        out.push(ch);
    }

    out
}

/// Extracts a boolean field for any of `keys` from cleaned JSON text.
fn extract_json_bool(cleaned_json: &str, keys: &[&str]) -> Option<bool> {
    for &key in keys {
        let pattern = format!("\"{key}\"");
        let mut search_from = 0;
        while let Some(pos) = cleaned_json[search_from..].find(&pattern) {
            let after_key = &cleaned_json[search_from + pos + pattern.len()..];
            let trimmed = after_key.trim_start();
            if let Some(after_colon) = trimmed.strip_prefix(':') {
                let val_str = after_colon.trim_start();
                if val_str.starts_with("true") {
                    return Some(true);
                } else if val_str.starts_with("false") {
                    return Some(false);
                }
            }
            search_from += pos + pattern.len();
        }
    }
    None
}

/// Extracts a string field for any of `keys` from cleaned JSON text.
fn extract_json_string(cleaned_json: &str, keys: &[&str]) -> Option<String> {
    for &key in keys {
        let pattern = format!("\"{key}\"");
        let mut search_from = 0;
        while let Some(pos) = cleaned_json[search_from..].find(&pattern) {
            let after_key = &cleaned_json[search_from + pos + pattern.len()..];
            let trimmed = after_key.trim_start();
            if let Some(after_colon) = trimmed.strip_prefix(':') {
                let val_str = after_colon.trim_start();
                if let Some(after_quote) = val_str.strip_prefix('"') {
                    let mut extracted = String::new();
                    let mut chars = after_quote.chars();
                    while let Some(ch) = chars.next() {
                        if ch == '\\' {
                            if let Some(esc) = chars.next() {
                                extracted.push(esc);
                            }
                        } else if ch == '"' {
                            return Some(extracted);
                        } else {
                            extracted.push(ch);
                        }
                    }
                }
            }
            search_from += pos + pattern.len();
        }
    }
    None
}

/// Resolves `DotenvSettings` from `.vscode/settings.json` (JSONC) and Zed `lsp.dotenv-lsp.settings` JSON (FR-14).
///
/// Zed `lsp.dotenv-lsp.settings` always takes highest precedence over `.vscode/settings.json`.
pub fn parse_dotenv_settings(
    vscode_jsonc: Option<&str>,
    zed_settings_json: Option<&str>,
) -> DotenvSettings {
    let mut settings = DotenvSettings::default();

    let cleaned_zed = zed_settings_json.map(strip_jsonc);
    if let Some(zed_str) = cleaned_zed.as_deref() {
        if let Some(import_vscode) = extract_json_bool(zed_str, &["importVscodeSettings"]) {
            settings.import_vscode_settings = import_vscode;
        }
    }

    if settings.import_vscode_settings {
        if let Some(vscode_raw) = vscode_jsonc {
            let cleaned_vscode = strip_jsonc(vscode_raw);
            if let Some(v) = extract_json_bool(
                &cleaned_vscode,
                &["dotenv.enableAutocompletion", "enableAutocompletion"],
            ) {
                settings.enable_autocompletion = v;
            }
            if let Some(v) = extract_json_bool(
                &cleaned_vscode,
                &["dotenv.maskValues", "dotenv.MaskValues", "maskValues"],
            ) {
                settings.mask_values = v;
            }
            if let Some(v) =
                extract_json_bool(&cleaned_vscode, &["dotenv.revealEdges", "revealEdges"])
            {
                settings.reveal_edges = v;
            }
            if let Some(path) =
                extract_json_string(&cleaned_vscode, &["dotenv.examplePath", "examplePath"])
            {
                if !path.trim().is_empty() {
                    settings.example_path = Some(normalize_worktree_path(&path));
                }
            }
        }
    }

    if let Some(zed_str) = cleaned_zed.as_deref() {
        if let Some(v) = extract_json_bool(
            zed_str,
            &["enableAutocompletion", "dotenv.enableAutocompletion"],
        ) {
            settings.enable_autocompletion = v;
        }
        if let Some(v) = extract_json_bool(
            zed_str,
            &["maskValues", "dotenv.maskValues", "dotenv.MaskValues"],
        ) {
            settings.mask_values = v;
        }
        if let Some(v) = extract_json_bool(zed_str, &["revealEdges", "dotenv.revealEdges"]) {
            settings.reveal_edges = v;
        }
        if let Some(path) = extract_json_string(zed_str, &["examplePath", "dotenv.examplePath"]) {
            if !path.trim().is_empty() {
                settings.example_path = Some(normalize_worktree_path(&path));
            }
        }
    }

    settings
}

/// Resolves the language server command to launch `dotenv-lsp` (FR-1).
///
/// Order of resolution:
/// 1. User-configured binary path in `.zed/settings.json` (`lsp.dotenv-lsp.binary.path`).
/// 2. `zed-dotenv-lsp` on `PATH` (via injected `which` closure).
/// 3. `dotenv-lsp` or `dotenv-linter` on `PATH` (via injected `which` closure).
/// 4. Actionable installation error explaining how to install `zed-dotenv-lsp` / `dotenv-lsp` / `dotenv-linter` or configure `lsp.dotenv-lsp.binary`.
pub fn resolve_server_command(
    custom_path: Option<String>,
    custom_args: Option<Vec<String>>,
    custom_env: Option<Vec<(String, String)>>,
    which: &dyn Fn(&str) -> Option<String>,
) -> Result<ServerCommandSpec, String> {
    let env = custom_env.unwrap_or_default();

    if let Some(path) = custom_path.filter(|p| !p.trim().is_empty()) {
        return Ok(ServerCommandSpec {
            command: path,
            args: custom_args.unwrap_or_default(),
            env,
        });
    }

    for &candidate in FALLBACK_SERVER_BINARIES {
        if let Some(found_path) = which(candidate) {
            let default_args = match &custom_args {
                Some(args) => args.clone(),
                None => Vec::new(),
            };
            return Ok(ServerCommandSpec {
                command: found_path,
                args: default_args,
                env,
            });
        }
    }

    Err(format!(
        "could not find `{}`, `{}`, or `{}` on PATH; install a dotenv language server \
         (e.g. `cargo install zed-dotenv-lsp`, `cargo install dotenv-lsp` or `cargo install dotenv-linter`) or configure \
         `lsp.{LANGUAGE_SERVER_ID}.binary.path` in `.zed/settings.json`",
        FALLBACK_SERVER_BINARIES[0], FALLBACK_SERVER_BINARIES[1], FALLBACK_SERVER_BINARIES[2]
    ))
}
