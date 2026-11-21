//! Markdown renderers for audit, template, mask and matrix output (FR-2, FR-3, FR-12, FR-13, FR-16).

use std::collections::{HashMap, HashSet};

use super::drift::{compare_env_files, generate_template};
use super::parser::parse_dotenv;
use super::paths::is_template_path;
use super::secrets::{
    classify_secret_pattern, is_encrypted_value, is_safe_default_value, is_sensitive_key,
    mask_value,
};
use super::types::{DotenvSettings, DriftReport, EnvEntry, ParseReport};

/// Renders the full environment-file audit report (FR-2..FR-11).
pub fn render_audit_report(
    env_path: &str,
    env_content: &str,
    example: Option<(&str, &str)>,
    settings: &DotenvSettings,
    worktree_root: Option<&str>,
) -> String {
    let env_report = parse_dotenv(env_content, is_template_path(env_path));
    let example_parsed = example.map(|(path, content)| (path, parse_dotenv(content, true)));
    let drift = example_parsed
        .as_ref()
        .map(|(_, ex_rep)| compare_env_files(&env_report, ex_rep));

    let total_errors =
        env_report.error_count() + example_parsed.as_ref().map_or(0, |(_, r)| r.error_count());
    let total_warnings = env_report.warning_count()
        + example_parsed
            .as_ref()
            .map_or(0, |(_, r)| r.warning_count());
    let drift_clean = drift.as_ref().is_none_or(DriftReport::is_clean);

    let overall_status = if total_errors > 0 {
        "FAIL (syntax errors detected)"
    } else if !drift_clean || total_warnings > 0 {
        "WARN (drift or warnings detected)"
    } else {
        "PASS (clean)"
    };

    let mut out = String::new();
    out.push_str("## dotenv & Secret Linter\n\n");

    out.push_str("### Summary\n");
    out.push_str(&format!("- status: **{overall_status}**\n"));
    out.push_str(&format!(
        "- target: `{env_path}` ({} variable(s), {} error(s), {} warning(s), {} info)\n",
        env_report.entries.len(),
        env_report.error_count(),
        env_report.warning_count(),
        env_report.info_count()
    ));
    match &example_parsed {
        Some((ex_path, ex_rep)) => {
            out.push_str(&format!(
                "- template: `{ex_path}` ({} variable(s))\n",
                ex_rep.entries.len()
            ));
        }
        None => {
            out.push_str(
                "- template: `(none found — run `zed-dotenv-lsp task` template to generate .env.example)`\n",
            );
        }
    }
    if let Some(root) = worktree_root {
        out.push_str(&format!("- worktree: `{root}`\n"));
    }
    out.push('\n');

    // Section 2: Syntax Diagnostics
    out.push_str("### Syntax Diagnostics\n");
    let ex_diags = example_parsed
        .as_ref()
        .map(|(p, r)| (*p, &r.diagnostics[..]))
        .unwrap_or(("", &[]));
    if env_report.diagnostics.is_empty() && ex_diags.1.is_empty() {
        out.push_str("No syntax or secret-pattern issues detected.\n\n");
    } else {
        out.push_str("~~~\n");
        for diag in &env_report.diagnostics {
            out.push_str(&format!(
                "{}:{}: [{}] {}: {}\n",
                env_path,
                diag.line,
                diag.severity.label(),
                diag.code,
                diag.message
            ));
        }
        for diag in ex_diags.1 {
            out.push_str(&format!(
                "{}:{}: [{}] {}: {}\n",
                ex_diags.0,
                diag.line,
                diag.severity.label(),
                diag.code,
                diag.message
            ));
        }
        out.push_str("~~~\n\n");
    }

    // Section 3: Drift vs Example
    out.push_str("### Drift vs Example\n");
    match (&example_parsed, &drift) {
        (Some((ex_path, _)), Some(d)) => {
            if d.is_clean() {
                out.push_str(&format!(
                    "All {} key(s) in `{env_path}` are in sync with `{ex_path}`.\n\n",
                    d.synced_count
                ));
            } else {
                out.push_str(&format!(
                    "- synced keys: `{}`\n- missing in `{env_path}`: `{}`\n- extra (undocumented) in `{env_path}`: `{}`\n- empty in `{env_path}`: `{}`\n- potential secret leaks in `{ex_path}`: `{}`\n\n",
                    d.synced_count,
                    d.missing_keys.len(),
                    d.extra_keys.len(),
                    d.empty_keys.len(),
                    d.template_leaks.len()
                ));
                out.push_str("~~~\n");
                for item in &d.missing_keys {
                    out.push_str(&format!(
                        "[MISSING] {} (defined at {}:{})\n",
                        item.key,
                        ex_path,
                        item.example_line.unwrap_or(0)
                    ));
                }
                for item in &d.extra_keys {
                    out.push_str(&format!(
                        "[EXTRA]   {} (defined at {}:{}, missing from {})\n",
                        item.key,
                        env_path,
                        item.env_line.unwrap_or(0),
                        ex_path
                    ));
                }
                for item in &d.empty_keys {
                    out.push_str(&format!(
                        "[EMPTY]   {} (empty at {}:{}, non-empty in {}:{})\n",
                        item.key,
                        env_path,
                        item.env_line.unwrap_or(0),
                        ex_path,
                        item.example_line.unwrap_or(0)
                    ));
                }
                for leak in &d.template_leaks {
                    out.push_str(&format!(
                        "[LEAK]    {} ({}:{}: {})\n",
                        leak.key, ex_path, leak.example_line, leak.reason
                    ));
                }
                out.push_str("~~~\n\n");
            }
        }
        _ => {
            out.push_str(
                "No `.env.example` / `.env.sample` / `.env.template` file was found for comparison.\n\n",
            );
        }
    }

    // Section 4: Masked Variables Summary (always masked to protect secrets)
    out.push_str("### Masked Variables\n");
    if env_report.entries.is_empty() {
        out.push_str("(no valid key-value entries)\n");
    } else {
        out.push_str("~~~\n");
        for entry in &env_report.entries {
            let masked = mask_value(&entry.value, settings.reveal_edges && !settings.mask_values);
            let char_len = entry.value.chars().count();
            let sensitivity = if is_encrypted_value(&entry.key, &entry.value) {
                "[ENCRYPTED: dotenvx/vault]"
            } else if classify_secret_pattern(&entry.key, &entry.value).is_some() {
                "secret-token"
            } else if is_sensitive_key(&entry.key) {
                "sensitive"
            } else {
                "standard"
            };
            out.push_str(&format!(
                "L{:02}: {}={} (len {}, {}, {})\n",
                entry.line,
                entry.key,
                masked,
                char_len,
                entry.quote_style.as_str(),
                sensitivity
            ));
        }
        out.push_str("~~~\n");
    }

    out
}

/// Renders the sanitized template report (FR-12).
pub fn render_template_report(
    env_path: &str,
    env_content: &str,
    worktree_root: Option<&str>,
) -> String {
    let report = parse_dotenv(env_content, false);
    let template = generate_template(env_content);
    let sanitized_count = report
        .entries
        .iter()
        .filter(|e| !e.value.is_empty() && !is_safe_default_value(&e.key, &e.value))
        .count();
    let preserved_count = report.entries.len().saturating_sub(sanitized_count);

    let mut out = String::new();
    out.push_str("## dotenv Template Generator\n\n");
    out.push_str("### Summary\n");
    out.push_str(&format!("- source: `{env_path}`\n"));
    out.push_str(&format!(
        "- variables: `{}` total (`{}` sanitized to `<YOUR_KEY>`, `{}` safe defaults preserved)\n",
        report.entries.len(),
        sanitized_count,
        preserved_count
    ));
    if let Some(root) = worktree_root {
        out.push_str(&format!("- worktree: `{root}`\n"));
    }
    out.push('\n');

    out.push_str("### Generated Template\n");
    out.push_str("~~~dotenv\n");
    out.push_str(&template);
    if !template.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("~~~\n");

    if !report.diagnostics.is_empty() {
        out.push_str("\n### Syntax Diagnostics\n");
        out.push_str("~~~\n");
        for diag in &report.diagnostics {
            out.push_str(&format!(
                "{}:{}: [{}] {}: {}\n",
                env_path,
                diag.line,
                diag.severity.label(),
                diag.code,
                diag.message
            ));
        }
        out.push_str("~~~\n");
    }

    out
}

/// Renders the masked preview report (FR-13).
pub fn render_masked_preview(
    env_path: &str,
    env_content: &str,
    settings: &DotenvSettings,
    worktree_root: Option<&str>,
) -> String {
    let report = parse_dotenv(env_content, is_template_path(env_path));
    let sensitive_count = report
        .entries
        .iter()
        .filter(|e| {
            is_sensitive_key(&e.key)
                || classify_secret_pattern(&e.key, &e.value).is_some()
                || is_encrypted_value(&e.key, &e.value)
        })
        .count();

    let mut out = String::new();
    out.push_str("## dotenv Masked Preview\n\n");
    out.push_str("### Summary\n");
    out.push_str(&format!("- target: `{env_path}`\n"));
    out.push_str(&format!(
        "- variables: `{}` total (`{}` sensitive/secret keys)\n",
        report.entries.len(),
        sensitive_count
    ));
    if let Some(root) = worktree_root {
        out.push_str(&format!("- worktree: `{root}`\n"));
    }
    out.push('\n');

    out.push_str("### Masked Variables\n");
    if report.entries.is_empty() {
        out.push_str("(no valid key-value entries)\n");
    } else {
        out.push_str("~~~\n");
        for entry in &report.entries {
            let masked = mask_value(&entry.value, settings.reveal_edges);
            let char_len = entry.value.chars().count();
            let classification = if is_encrypted_value(&entry.key, &entry.value) {
                "[ENCRYPTED: dotenvx/vault]".to_string()
            } else if let Some(kind) = classify_secret_pattern(&entry.key, &entry.value) {
                format!("secret-token: {kind}")
            } else if is_sensitive_key(&entry.key) {
                "sensitive-key".to_string()
            } else {
                "standard".to_string()
            };
            out.push_str(&format!(
                "L{:02}: {}={} (len {}, {}, {})\n",
                entry.line,
                entry.key,
                masked,
                char_len,
                entry.quote_style.as_str(),
                classification
            ));
        }
        out.push_str("~~~\n");
    }

    if !report.diagnostics.is_empty() {
        out.push_str("\n### Syntax Diagnostics\n");
        out.push_str("~~~\n");
        for diag in &report.diagnostics {
            out.push_str(&format!(
                "{}:{}: [{}] {}: {}\n",
                env_path,
                diag.line,
                diag.severity.label(),
                diag.code,
                diag.message
            ));
        }
        out.push_str("~~~\n");
    }

    out
}

/// Renders the multi-environment comparison matrix report (FR-16).
pub fn render_matrix_report(files: &[(&str, &str)], mask_values: bool) -> String {
    let parsed_files: Vec<(&str, ParseReport)> = files
        .iter()
        .map(|(path, content)| (*path, parse_dotenv(content, is_template_path(path))))
        .collect();

    let mut out = String::new();
    out.push_str("## dotenv Multi-Environment Matrix\n\n");

    if parsed_files.is_empty() {
        out.push_str(
            "### Summary\n- environments: `0` files\n\n(no environment files to compare)\n",
        );
        return out;
    }

    // Collect all unique keys in order of first appearance
    let mut all_keys: Vec<String> = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();
    for (_, rep) in &parsed_files {
        for entry in &rep.entries {
            if seen_keys.insert(entry.key.clone()) {
                all_keys.push(entry.key.clone());
            }
        }
    }

    // Map each file's keys
    let file_maps: Vec<HashMap<&str, &EnvEntry>> = parsed_files
        .iter()
        .map(|(_, rep)| {
            let mut map = HashMap::new();
            for entry in &rep.entries {
                map.insert(entry.key.as_str(), entry);
            }
            map
        })
        .collect();

    // Determine cell status for each key and each file
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum CellStatus {
        Set(String),
        Empty,
        Missing,
        SecretPattern(&'static str),
    }

    impl CellStatus {
        fn as_label(&self) -> String {
            match self {
                Self::Set(s) => format!("✅ Set ({s})"),
                Self::Empty => "⚪ Empty".to_string(),
                Self::Missing => "❌ Missing".to_string(),
                Self::SecretPattern(_) => "⚠️ Secret Pattern".to_string(),
            }
        }
    }

    let mut key_cells: Vec<(String, Vec<CellStatus>)> = Vec::new();
    let mut inconsistent_keys = Vec::new();

    for key in &all_keys {
        let mut row_statuses = Vec::new();
        for (file_idx, (_path, _)) in files.iter().enumerate() {
            let status = match file_maps[file_idx].get(key.as_str()) {
                None => CellStatus::Missing,
                Some(entry) => {
                    if entry.value.trim().is_empty() {
                        CellStatus::Empty
                    } else if let Some(pattern) = classify_secret_pattern(&entry.key, &entry.value)
                    {
                        CellStatus::SecretPattern(pattern)
                    } else if is_encrypted_value(&entry.key, &entry.value) {
                        CellStatus::Set("[ENCRYPTED]".to_string())
                    } else if mask_values {
                        CellStatus::Set("***".to_string())
                    } else {
                        CellStatus::Set(mask_value(&entry.value, false))
                    }
                }
            };
            row_statuses.push(status);
        }

        // Check consistency: consistent only if all files have the exact same label
        let first_label = row_statuses[0].as_label();
        let is_consistent = row_statuses.iter().all(|s| s.as_label() == first_label);
        if !is_consistent {
            inconsistent_keys.push((key.clone(), row_statuses.clone()));
        }

        key_cells.push((key.clone(), row_statuses));
    }

    let consistent_count = all_keys.len().saturating_sub(inconsistent_keys.len());

    // Section 1: Summary
    out.push_str("### Summary\n");
    let env_names: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
    out.push_str(&format!(
        "- environments: `{}` file(s) ({})\n",
        files.len(),
        env_names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str(&format!("- total unique keys: `{}`\n", all_keys.len()));
    out.push_str(&format!("- consistent keys: `{consistent_count}`\n"));
    out.push_str(&format!(
        "- inconsistent keys: `{}`\n\n",
        inconsistent_keys.len()
    ));

    // Section 2: Environment Matrix table
    out.push_str("### Environment Matrix\n");
    // Table header
    out.push_str("| Key |");
    for (path, _) in files {
        out.push_str(&format!(" {path} |"));
    }
    out.push('\n');
    out.push_str("|---|");
    for _ in files {
        out.push_str("---|");
    }
    out.push('\n');

    for (key, statuses) in &key_cells {
        out.push_str(&format!("| {key} |"));
        for st in statuses {
            out.push_str(&format!(" {} |", st.as_label()));
        }
        out.push('\n');
    }
    out.push('\n');

    // Section 3: Inconsistent Keys
    out.push_str("### Inconsistent Keys\n");
    if inconsistent_keys.is_empty() {
        out.push_str(&format!(
            "All {} key(s) are consistently defined across all {} environment file(s).\n\n",
            all_keys.len(),
            files.len()
        ));
    } else {
        out.push_str("~~~\n");
        for (key, statuses) in &inconsistent_keys {
            let mut missing_in = Vec::new();
            let mut empty_in = Vec::new();
            let mut set_in = Vec::new();
            let mut secret_in = Vec::new();

            for (idx, (path, _)) in files.iter().enumerate() {
                match &statuses[idx] {
                    CellStatus::Missing => missing_in.push(*path),
                    CellStatus::Empty => empty_in.push(*path),
                    CellStatus::Set(_) => set_in.push(*path),
                    CellStatus::SecretPattern(_) => secret_in.push(*path),
                }
            }

            let mut parts = Vec::new();
            if !missing_in.is_empty() {
                parts.push(format!("Missing in {}", missing_in.join(", ")));
            }
            if !empty_in.is_empty() {
                parts.push(format!("Empty in {}", empty_in.join(", ")));
            }
            if !set_in.is_empty() {
                parts.push(format!("Set in {}", set_in.join(", ")));
            }
            if !secret_in.is_empty() {
                parts.push(format!("Secret Pattern in {}", secret_in.join(", ")));
            }

            out.push_str(&format!("[DRIFT] {key}: {}\n", parts.join("; ")));
        }
        out.push_str("~~~\n\n");
    }

    out
}
