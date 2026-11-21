//! Source-to-env reference scanner (FR-17).

use std::collections::HashSet;

use super::parser::parse_dotenv;
use super::paths::is_valid_env_key;
use super::types::SourceEnvRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternKind {
    DotIdent,
    BracketQuote,
    ParenQuote,
}

fn extract_ident_token(after_needle: &str) -> Option<String> {
    let mut end = 0;
    for (i, c) in after_needle.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let token = &after_needle[..end];
    if is_valid_env_key(token) {
        Some(token.to_string())
    } else {
        None
    }
}

fn extract_quoted_token(after_needle: &str) -> Option<String> {
    let trimmed = after_needle.trim_start();
    let quote = trimmed.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let after_quote = &trimmed[quote.len_utf8()..];
    let end_idx = after_quote.find(quote)?;
    let token = &after_quote[..end_idx];
    if is_valid_env_key(token) {
        Some(token.to_string())
    } else {
        None
    }
}

fn extract_pattern_refs_guarded(
    line: &str,
    line_num: usize,
    needle: &str,
    disallowed_prefix: &str,
    kind: PatternKind,
    pattern_name: &'static str,
    refs: &mut Vec<SourceEnvRef>,
) {
    let mut search_from = 0;
    while let Some(pos) = line[search_from..].find(needle) {
        let abs_pos = search_from + pos;
        if !disallowed_prefix.is_empty()
            && abs_pos >= disallowed_prefix.len()
            && &line[abs_pos - disallowed_prefix.len()..abs_pos] == disallowed_prefix
        {
            search_from = abs_pos + needle.len();
            continue;
        }
        let after_needle = &line[abs_pos + needle.len()..];
        let maybe_key = match kind {
            PatternKind::DotIdent => extract_ident_token(after_needle),
            PatternKind::BracketQuote | PatternKind::ParenQuote => {
                extract_quoted_token(after_needle)
            }
        };

        if let Some(key) = maybe_key {
            let key_len = key.len();
            refs.push(SourceEnvRef {
                line: line_num,
                key,
                pattern: pattern_name,
            });
            search_from = abs_pos + needle.len() + key_len;
        } else {
            search_from = abs_pos + needle.len();
        }
    }
}

/// Extracts environment variable references from source content across JS/TS, Rust, Python, and Go (FR-17).
pub fn extract_source_env_refs(src_content: &str) -> Vec<SourceEnvRef> {
    let mut refs = Vec::new();

    for (idx, raw_line) in src_content.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let trimmed = line.trim();

        // Skip single-line comments in JS/TS/Rust/Go/Python
        if trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
        {
            continue;
        }

        // 1. JavaScript / TypeScript: `process.env.KEY` and `process.env["KEY"]`
        extract_pattern_refs_guarded(
            line,
            line_num,
            "process.env.",
            "",
            PatternKind::DotIdent,
            "process.env",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "process.env[",
            "",
            PatternKind::BracketQuote,
            "process.env[]",
            &mut refs,
        );

        // 2. JavaScript / TypeScript: `import.meta.env.KEY` and `import.meta.env["KEY"]`
        extract_pattern_refs_guarded(
            line,
            line_num,
            "import.meta.env.",
            "",
            PatternKind::DotIdent,
            "import.meta.env",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "import.meta.env[",
            "",
            PatternKind::BracketQuote,
            "import.meta.env[]",
            &mut refs,
        );

        // 3. Rust: `std::env::var("KEY")`, `env::var("KEY")`, `env!("KEY")`, `option_env!("KEY")`
        extract_pattern_refs_guarded(
            line,
            line_num,
            "std::env::var(",
            "",
            PatternKind::ParenQuote,
            "std::env::var",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "env::var(",
            "std::",
            PatternKind::ParenQuote,
            "env::var",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "env!(",
            "",
            PatternKind::ParenQuote,
            "env!",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "option_env!(",
            "",
            PatternKind::ParenQuote,
            "option_env!",
            &mut refs,
        );

        // 4. Python: `os.environ["KEY"]`, `os.environ.get("KEY")`, `os.getenv("KEY")`
        extract_pattern_refs_guarded(
            line,
            line_num,
            "os.environ[",
            "",
            PatternKind::BracketQuote,
            "os.environ[]",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "os.environ.get(",
            "",
            PatternKind::ParenQuote,
            "os.environ.get",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "os.getenv(",
            "",
            PatternKind::ParenQuote,
            "os.getenv",
            &mut refs,
        );

        // 5. Go: `os.Getenv("KEY")`, `os.LookupEnv("KEY")`
        extract_pattern_refs_guarded(
            line,
            line_num,
            "os.Getenv(",
            "",
            PatternKind::ParenQuote,
            "os.Getenv",
            &mut refs,
        );
        extract_pattern_refs_guarded(
            line,
            line_num,
            "os.LookupEnv(",
            "",
            PatternKind::ParenQuote,
            "os.LookupEnv",
            &mut refs,
        );
    }

    refs
}

/// Renders the source-code reference scan report (FR-17).
pub fn render_scan_report(
    src_path: &str,
    src_content: &str,
    env_path: &str,
    env_content: &str,
    example: Option<(&str, &str)>,
    worktree_root: Option<&str>,
) -> String {
    let refs = extract_source_env_refs(src_content);
    let env_report = parse_dotenv(env_content, false);
    let example_report = example.map(|(p, c)| (p, parse_dotenv(c, true)));

    let env_keys: HashSet<&str> = env_report.entries.iter().map(|e| e.key.as_str()).collect();
    let ex_keys: HashSet<&str> = example_report
        .as_ref()
        .map(|(_, r)| r.entries.iter().map(|e| e.key.as_str()).collect())
        .unwrap_or_default();

    let mut undeclared: Vec<(&SourceEnvRef, &'static str)> = Vec::new();
    let mut declared: Vec<&SourceEnvRef> = Vec::new();

    for r in &refs {
        if env_keys.contains(r.key.as_str()) {
            declared.push(r);
        } else if ex_keys.contains(r.key.as_str()) {
            undeclared.push((r, "missing in target env, defined in template"));
        } else {
            undeclared.push((r, "missing in both target env and template"));
        }
    }

    let status = if undeclared.is_empty() {
        "PASS (all references declared)"
    } else {
        "FAIL (undeclared environment variable references detected)"
    };

    let mut out = String::new();
    out.push_str("## dotenv Source Reference Scanner\n\n");

    out.push_str("### Summary\n");
    out.push_str(&format!("- status: **{status}**\n"));
    out.push_str(&format!("- source: `{src_path}`\n"));
    out.push_str(&format!("- target: `{env_path}`\n"));
    if let Some((ex_path, _)) = &example_report {
        out.push_str(&format!("- template: `{ex_path}`\n"));
    }
    if let Some(root) = worktree_root {
        out.push_str(&format!("- worktree: `{root}`\n"));
    }
    out.push_str(&format!(
        "- references: `{}` reference(s) (`{}` declared, `{}` undeclared)\n\n",
        refs.len(),
        declared.len(),
        undeclared.len()
    ));

    out.push_str("### Undeclared Environment Variables\n");
    if undeclared.is_empty() {
        out.push_str(&format!(
            "No undeclared variables detected. All references in `{src_path}` are defined in `{env_path}`.\n\n"
        ));
    } else {
        out.push_str("~~~\n");
        for (r, reason) in &undeclared {
            out.push_str(&format!(
                "{}:{}: [UNDECLARED] `{}` (via `{}`) — {}\n",
                src_path, r.line, r.key, r.pattern, reason
            ));
        }
        out.push_str("~~~\n\n");
    }

    out.push_str("### Declared Environment Variables\n");
    if declared.is_empty() {
        out.push_str("(no declared variable references found)\n\n");
    } else {
        out.push_str("| Line | Pattern | Key | Status in Target |\n");
        out.push_str("|---|---|---|---|\n");
        for r in &declared {
            out.push_str(&format!(
                "| L{:02} | `{}` | {} | ✅ Declared in `{}` |\n",
                r.line, r.pattern, r.key, env_path
            ));
        }
        out.push('\n');
    }

    out
}
