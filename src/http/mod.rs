//! `.http` `{{$dotenv KEY}}` readiness check (FR-18).

use std::collections::{HashMap, HashSet};

use super::parser::parse_dotenv;
use super::paths::is_valid_env_key;
use super::types::SourceEnvRef;

/// Extracts `{{$dotenv KEY}}` and `{{$dotenv %KEY}}` tokens from `.http` file contents (FR-18).
pub fn extract_http_dotenv_refs(http_content: &str) -> Vec<SourceEnvRef> {
    let mut refs = Vec::new();

    for (idx, raw_line) in http_content.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let trimmed = line.trim();

        // Skip comments in RFC 7230 / .http files (# or //)
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        let mut search_from = 0;
        while let Some(open_pos) = line[search_from..].find("{{") {
            let abs_open = search_from + open_pos;
            let after_open = &line[abs_open + 2..];
            let Some(close_idx) = after_open.find("}}") else {
                break;
            };
            let token_body = after_open[..close_idx].trim();
            if let Some(rest) = token_body.strip_prefix("$dotenv") {
                let rest_trimmed = rest.trim_start();
                let key_candidate = rest_trimmed
                    .strip_prefix('%')
                    .unwrap_or(rest_trimmed)
                    .trim();
                if is_valid_env_key(key_candidate) {
                    refs.push(SourceEnvRef {
                        line: line_num,
                        key: key_candidate.to_string(),
                        pattern: "{{$dotenv}}",
                    });
                }
            }
            search_from = abs_open + 2 + close_idx + 2;
        }
    }

    refs
}

/// Renders the `.http` file `{{$dotenv KEY}}` pre-flight readiness report (FR-18).
pub fn render_http_readiness_report(
    http_path: &str,
    http_content: &str,
    env_path: &str,
    env_content: &str,
    example: Option<(&str, &str)>,
    worktree_root: Option<&str>,
) -> String {
    let refs = extract_http_dotenv_refs(http_content);
    let env_report = parse_dotenv(env_content, false);
    let example_report = example.map(|(p, c)| (p, parse_dotenv(c, true)));

    let mut env_map = HashMap::new();
    for entry in &env_report.entries {
        env_map.insert(entry.key.as_str(), entry);
    }
    let ex_keys: HashSet<&str> = example_report
        .as_ref()
        .map(|(_, r)| r.entries.iter().map(|e| e.key.as_str()).collect())
        .unwrap_or_default();

    #[derive(Debug, PartialEq, Eq)]
    enum HttpVarStatus {
        Ready,
        Empty,
        MissingInEnv(bool), // bool: true if in example
    }

    let mut ready_count = 0usize;
    let mut empty_count = 0usize;
    let mut missing_count = 0usize;
    let mut ref_statuses = Vec::new();

    for r in &refs {
        let status = match env_map.get(r.key.as_str()) {
            Some(entry) => {
                if entry.value.trim().is_empty() {
                    empty_count += 1;
                    HttpVarStatus::Empty
                } else {
                    ready_count += 1;
                    HttpVarStatus::Ready
                }
            }
            None => {
                missing_count += 1;
                HttpVarStatus::MissingInEnv(ex_keys.contains(r.key.as_str()))
            }
        };
        ref_statuses.push((r, status));
    }

    let overall_status = if missing_count > 0 {
        "FAIL (missing variables required by .http file)"
    } else if empty_count > 0 {
        "WARN (empty variables in .env)"
    } else {
        "PASS (all .http variables ready)"
    };

    let mut out = String::new();
    out.push_str("## dotenv HTTP Readiness Check\n\n");

    out.push_str("### Summary\n");
    out.push_str(&format!("- status: **{overall_status}**\n"));
    out.push_str(&format!("- target: `{http_path}`\n"));
    out.push_str(&format!("- env: `{env_path}`\n"));
    if let Some((ex_path, _)) = &example_report {
        out.push_str(&format!("- template: `{ex_path}`\n"));
    }
    if let Some(root) = worktree_root {
        out.push_str(&format!("- worktree: `{root}`\n"));
    }
    out.push_str(&format!(
        "- variables: `{}` reference(s) (`{}` ready, `{}` empty, `{}` missing)\n\n",
        refs.len(),
        ready_count,
        empty_count,
        missing_count
    ));

    out.push_str("### HTTP Variable Readiness\n");
    if refs.is_empty() {
        out.push_str(&format!(
            "No `{{{{$dotenv ...}}}}` references found in `{http_path}`.\n\n"
        ));
    } else {
        out.push_str("| Line | Key | Status in Target |\n");
        out.push_str("|---|---|---|\n");
        for (r, status) in &ref_statuses {
            let label = match status {
                HttpVarStatus::Ready => "✅ Ready (masked ***)".to_string(),
                HttpVarStatus::Empty => "⚪ Empty in .env".to_string(),
                HttpVarStatus::MissingInEnv(true) => {
                    "❌ Missing in .env (present in template)".to_string()
                }
                HttpVarStatus::MissingInEnv(false) => "❌ Missing in .env".to_string(),
            };
            out.push_str(&format!("| L{:02} | {} | {label} |\n", r.line, r.key));
        }
        out.push('\n');
    }

    if missing_count > 0 || empty_count > 0 {
        out.push_str("### Missing or Empty Variables\n");
        out.push_str("~~~\n");
        for (r, status) in &ref_statuses {
            match status {
                HttpVarStatus::MissingInEnv(in_ex) => {
                    let note = if *in_ex {
                        " (present in template; populate into target env)"
                    } else {
                        ""
                    };
                    out.push_str(&format!(
                        "{}:{}: [MISSING] `{{{{$dotenv {}}}}}`, missing from `{}`{}\n",
                        http_path, r.line, r.key, env_path, note
                    ));
                }
                HttpVarStatus::Empty => {
                    out.push_str(&format!(
                        "{}:{}: [EMPTY] `{{{{$dotenv {}}}}}`, defined but empty in `{}`\n",
                        http_path, r.line, r.key, env_path
                    ));
                }
                HttpVarStatus::Ready => {}
            }
        }
        out.push_str("~~~\n\n");
    }

    out
}
