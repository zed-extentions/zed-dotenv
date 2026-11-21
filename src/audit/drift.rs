//! Schema drift analysis and sanitized template generation (FR-11, FR-12).

use std::collections::HashMap;

use super::parser::parse_dotenv;
use super::secrets::{
    classify_secret_pattern, is_encrypted_value, is_placeholder_value, is_safe_default_value,
    is_sensitive_key,
};
use super::types::{
    DriftItem, DriftReport, EnvEntry, ParseReport, QuoteStyle, RawLine, TemplateLeak,
};

/// Compares a parsed `.env` report against a parsed `.env.example` template report (FR-11).
pub fn compare_env_files(env_report: &ParseReport, example_report: &ParseReport) -> DriftReport {
    // Effective (last) entry per key in each file, preserving order of first appearance.
    let mut env_map: HashMap<&str, &EnvEntry> = HashMap::new();
    let mut env_order: Vec<&str> = Vec::new();
    for entry in &env_report.entries {
        if !env_map.contains_key(entry.key.as_str()) {
            env_order.push(entry.key.as_str());
        }
        env_map.insert(entry.key.as_str(), entry);
    }

    let mut example_map: HashMap<&str, &EnvEntry> = HashMap::new();
    let mut example_order: Vec<&str> = Vec::new();
    for entry in &example_report.entries {
        if !example_map.contains_key(entry.key.as_str()) {
            example_order.push(entry.key.as_str());
        }
        example_map.insert(entry.key.as_str(), entry);
    }

    let mut missing_keys = Vec::new();
    let mut empty_keys = Vec::new();
    let mut synced_count = 0usize;

    for &key in &example_order {
        let ex_entry = example_map[key];
        match env_map.get(key) {
            None => {
                missing_keys.push(DriftItem {
                    key: key.to_string(),
                    env_line: None,
                    example_line: Some(ex_entry.line),
                });
            }
            Some(env_entry) => {
                if env_entry.value.trim().is_empty() && !ex_entry.value.trim().is_empty() {
                    empty_keys.push(DriftItem {
                        key: key.to_string(),
                        env_line: Some(env_entry.line),
                        example_line: Some(ex_entry.line),
                    });
                } else {
                    synced_count += 1;
                }
            }
        }
    }

    let mut extra_keys = Vec::new();
    for &key in &env_order {
        if !example_map.contains_key(key) {
            let env_entry = env_map[key];
            extra_keys.push(DriftItem {
                key: key.to_string(),
                env_line: Some(env_entry.line),
                example_line: None,
            });
        }
    }

    let mut template_leaks = Vec::new();
    for &key in &example_order {
        let ex_entry = example_map[key];
        if is_encrypted_value(&ex_entry.key, &ex_entry.value) {
            continue;
        }
        if let Some(secret_kind) = classify_secret_pattern(&ex_entry.key, &ex_entry.value) {
            template_leaks.push(TemplateLeak {
                key: ex_entry.key.clone(),
                example_line: ex_entry.line,
                reason: format!("matches live credential pattern ({secret_kind})"),
            });
        } else if is_sensitive_key(&ex_entry.key)
            && !ex_entry.value.trim().is_empty()
            && !is_placeholder_value(&ex_entry.value)
            && !is_safe_default_value(&ex_entry.key, &ex_entry.value)
        {
            template_leaks.push(TemplateLeak {
                key: ex_entry.key.clone(),
                example_line: ex_entry.line,
                reason: "sensitive key has a non-placeholder value in template".to_string(),
            });
        }
    }

    DriftReport {
        missing_keys,
        extra_keys,
        empty_keys,
        template_leaks,
        synced_count,
    }
}

/// Generates a sanitized `.env.example` template from `.env` contents (FR-12).
///
/// Comments and blank lines are preserved verbatim. Safe non-secret defaults
/// (`PORT=3000`, `NODE_ENV=development`, booleans, `localhost` URLs) are kept,
/// while sensitive or non-default values are replaced with `<YOUR_KEY>`.
pub fn generate_template(content: &str) -> String {
    let report = parse_dotenv(content, false);
    let mut out = String::new();

    for line in &report.lines {
        match line {
            RawLine::Blank => out.push('\n'),
            RawLine::Comment(comment) => {
                out.push_str(comment);
                out.push('\n');
            }
            RawLine::Malformed { raw, .. } => {
                out.push_str("# [malformed] ");
                out.push_str(raw);
                out.push('\n');
            }
            RawLine::Entry(entry) => {
                if entry.has_export {
                    out.push_str("export ");
                }
                out.push_str(&entry.key);
                out.push('=');
                if entry.value.is_empty() {
                    // Preserve empty value
                } else if is_safe_default_value(&entry.key, &entry.value) {
                    match entry.quote_style {
                        QuoteStyle::Unquoted => out.push_str(&entry.value),
                        QuoteStyle::Single => {
                            out.push('\'');
                            out.push_str(&entry.value);
                            out.push('\'');
                        }
                        QuoteStyle::Double => {
                            out.push('"');
                            out.push_str(&entry.value);
                            out.push('"');
                        }
                    }
                } else {
                    out.push_str(&format!("<YOUR_{}>", entry.key));
                }
                if let Some(comment) = &entry.inline_comment {
                    out.push(' ');
                    out.push_str(comment);
                }
                out.push('\n');
            }
        }
    }

    out
}
