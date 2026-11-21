//! Line-by-line `.env` lexer and strict syntax linter (FR-4..FR-10).

use std::collections::HashMap;

use super::paths::is_valid_env_key;
use super::secrets::classify_secret_pattern;
use super::types::{DiagnosticIssue, EnvEntry, ParseReport, QuoteStyle, RawLine, Severity};

/// Parses and lints a `.env` or `.env.example` document line-by-line (FR-4..FR-10).
pub fn parse_dotenv(content: &str, is_template: bool) -> ParseReport {
    let mut entries = Vec::new();
    let mut lines = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen_keys: HashMap<String, usize> = HashMap::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let trimmed = line.trim();

        if trimmed.is_empty() {
            lines.push(RawLine::Blank);
            continue;
        }
        if trimmed.starts_with('#') {
            lines.push(RawLine::Comment(line.to_string()));
            continue;
        }

        // Check for optional `export ` prefix
        let (has_export, after_export) = if let Some(rest) = trimmed.strip_prefix("export") {
            if rest.starts_with([' ', '\t']) {
                (true, rest.trim_start())
            } else {
                (false, line)
            }
        } else {
            (false, line)
        };

        // DOTENV001: Missing `=` separator
        let Some((raw_key, raw_rhs)) = after_export.split_once('=') else {
            diagnostics.push(DiagnosticIssue {
                line: line_num,
                severity: Severity::Error,
                code: "DOTENV001",
                message: format!("missing `=` separator in line `{trimmed}`"),
                key: None,
            });
            lines.push(RawLine::Malformed {
                line: line_num,
                raw: line.to_string(),
            });
            continue;
        };

        let key_leading_ws = !has_export && raw_key.starts_with([' ', '\t']);
        let key_trailing_ws = raw_key.ends_with([' ', '\t']);
        let key = raw_key.trim().to_string();

        // DOTENV002: Invalid variable identifier
        if !is_valid_env_key(&key) {
            diagnostics.push(DiagnosticIssue {
                line: line_num,
                severity: Severity::Error,
                code: "DOTENV002",
                message: format!(
                    "invalid variable identifier `{key}` (must match `[A-Za-z_][A-Za-z0-9_]*`)"
                ),
                key: None,
            });
            lines.push(RawLine::Malformed {
                line: line_num,
                raw: line.to_string(),
            });
            continue;
        };

        let rhs_leading_ws = raw_rhs.starts_with([' ', '\t']);
        let rhs_trimmed_start = raw_rhs.trim_start();

        // DOTENV005: Whitespace around `=` or leading space before key
        if key_leading_ws
            || key_trailing_ws
            || (rhs_leading_ws && !rhs_trimmed_start.starts_with('#'))
        {
            diagnostics.push(DiagnosticIssue {
                line: line_num,
                severity: Severity::Warning,
                code: "DOTENV005",
                message: format!(
                    "whitespace around `=` for `{key}`; use `{key}=<value>` without spaces around `=`"
                ),
                key: Some(key.clone()),
            });
        }

        // Parse right-hand side value according to quote style
        let parsed_rhs = if let Some(after_open) = rhs_trimmed_start.strip_prefix('\'') {
            parse_single_quoted_rhs(&key, line_num, after_open, &mut diagnostics)
        } else if let Some(after_open) = rhs_trimmed_start.strip_prefix('"') {
            parse_double_quoted_rhs(&key, line_num, after_open, &mut diagnostics)
        } else {
            Ok(parse_unquoted_rhs(
                &key,
                line_num,
                rhs_trimmed_start,
                &mut diagnostics,
            ))
        };

        let Some((value, quote_style, inline_comment)) = parsed_rhs.ok() else {
            lines.push(RawLine::Malformed {
                line: line_num,
                raw: line.to_string(),
            });
            continue;
        };

        // DOTENV004: Duplicate key definition
        if let Some(&prev_line) = seen_keys.get(&key) {
            diagnostics.push(DiagnosticIssue {
                line: line_num,
                severity: Severity::Warning,
                code: "DOTENV004",
                message: format!(
                    "duplicate key `{key}` (previously defined on line {prev_line}; later definition overrides earlier value)"
                ),
                key: Some(key.clone()),
            });
        } else {
            seen_keys.insert(key.clone(), line_num);
        }

        // DOTENV006: High-risk secret format in plaintext
        if let Some(secret_kind) = classify_secret_pattern(&key, &value) {
            let (severity, advice) = if is_template {
                (
                    Severity::Warning,
                    "replace live secret in template file with a safe placeholder",
                )
            } else {
                (
                    Severity::Info,
                    "ensure this `.env` file is excluded in `.gitignore`",
                )
            };
            diagnostics.push(DiagnosticIssue {
                line: line_num,
                severity,
                code: "DOTENV006",
                message: format!(
                    "high-risk secret pattern ({secret_kind}) detected in `{key}`; {advice}"
                ),
                key: Some(key.clone()),
            });
        }

        let entry = EnvEntry {
            line: line_num,
            key,
            value,
            quote_style,
            has_export,
            inline_comment,
        };
        entries.push(entry.clone());
        lines.push(RawLine::Entry(entry));
    }

    ParseReport {
        entries,
        lines,
        diagnostics,
    }
}

fn parse_single_quoted_rhs(
    key: &str,
    line_num: usize,
    after_open: &str,
    diagnostics: &mut Vec<DiagnosticIssue>,
) -> Result<(String, QuoteStyle, Option<String>), ()> {
    let Some(close_idx) = after_open.find('\'') else {
        diagnostics.push(DiagnosticIssue {
            line: line_num,
            severity: Severity::Error,
            code: "DOTENV003",
            message: format!("unclosed single quote in value for `{key}`"),
            key: Some(key.to_string()),
        });
        return Err(());
    };

    let value = after_open[..close_idx].to_string();
    let trailing = after_open[close_idx + 1..].trim();
    if trailing.is_empty() {
        Ok((value, QuoteStyle::Single, None))
    } else if trailing.starts_with('#') {
        Ok((value, QuoteStyle::Single, Some(trailing.to_string())))
    } else {
        diagnostics.push(DiagnosticIssue {
            line: line_num,
            severity: Severity::Error,
            code: "DOTENV003",
            message: format!(
                "unexpected trailing characters after closing single quote for `{key}`"
            ),
            key: Some(key.to_string()),
        });
        Err(())
    }
}

fn parse_double_quoted_rhs(
    key: &str,
    line_num: usize,
    after_open: &str,
    diagnostics: &mut Vec<DiagnosticIssue>,
) -> Result<(String, QuoteStyle, Option<String>), ()> {
    let mut unescaped = String::new();
    let mut chars = after_open.char_indices();
    let mut close_byte_idx = None;

    while let Some((idx, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, next_ch)) = chars.next() {
                match next_ch {
                    'n' => unescaped.push('\n'),
                    'r' => unescaped.push('\r'),
                    't' => unescaped.push('\t'),
                    '"' => unescaped.push('"'),
                    '\\' => unescaped.push('\\'),
                    '$' => unescaped.push('$'),
                    other => {
                        unescaped.push('\\');
                        unescaped.push(other);
                    }
                }
            } else {
                // Trailing backslash right before end of line -> unclosed quote
                break;
            }
        } else if ch == '"' {
            close_byte_idx = Some(idx);
            break;
        } else {
            unescaped.push(ch);
        }
    }

    let Some(close_idx) = close_byte_idx else {
        diagnostics.push(DiagnosticIssue {
            line: line_num,
            severity: Severity::Error,
            code: "DOTENV003",
            message: format!("unclosed double quote in value for `{key}`"),
            key: Some(key.to_string()),
        });
        return Err(());
    };

    let trailing = after_open[close_idx + 1..].trim();
    if trailing.is_empty() {
        Ok((unescaped, QuoteStyle::Double, None))
    } else if trailing.starts_with('#') {
        Ok((unescaped, QuoteStyle::Double, Some(trailing.to_string())))
    } else {
        diagnostics.push(DiagnosticIssue {
            line: line_num,
            severity: Severity::Error,
            code: "DOTENV003",
            message: format!(
                "unexpected trailing characters after closing double quote for `{key}`"
            ),
            key: Some(key.to_string()),
        });
        Err(())
    }
}

fn parse_unquoted_rhs(
    key: &str,
    line_num: usize,
    rhs: &str,
    diagnostics: &mut Vec<DiagnosticIssue>,
) -> (String, QuoteStyle, Option<String>) {
    if rhs.starts_with('#') {
        return (String::new(), QuoteStyle::Unquoted, Some(rhs.to_string()));
    }

    // Find inline comment preceded by whitespace (` #`)
    let mut comment_split = None;
    let bytes = rhs.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i] == b'#' && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
            comment_split = Some(i);
            break;
        }
    }

    let (raw_val, inline_comment) = match comment_split {
        Some(idx) => (rhs[..idx].trim_end(), Some(rhs[idx..].trim().to_string())),
        None => (rhs.trim_end(), None),
    };

    // DOTENV005: Unquoted internal whitespace
    if raw_val.contains([' ', '\t']) {
        diagnostics.push(DiagnosticIssue {
            line: line_num,
            severity: Severity::Warning,
            code: "DOTENV005",
            message: format!(
                "unquoted whitespace in value for `{key}`; wrap multi-word values in quotes (`{key}=\"...\"`)"
            ),
            key: Some(key.to_string()),
        });
    }

    (raw_val.to_string(), QuoteStyle::Unquoted, inline_comment)
}
