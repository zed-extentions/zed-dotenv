//! `$NAME` / `${NAME}` completion candidates for the native language server.

use super::parser::parse_dotenv;

/// Return variable names from this document for a `$NAME` or `${NAME}` reference.
/// The cursor position uses UTF-16 code units, as required by LSP.
#[allow(dead_code)] // Used by the native LSP, not the WASM extension.
pub fn completion_candidates(content: &str, line: usize, character: usize) -> Vec<String> {
    let Some(current) = content.lines().nth(line) else {
        return Vec::new();
    };
    let mut units = 0;
    let mut end = None;
    for (index, ch) in current.char_indices() {
        if units == character {
            end = Some(index);
            break;
        }
        units += ch.len_utf16();
        if units > character {
            return Vec::new();
        }
    }
    let Some(end) = end.or_else(|| (units == character).then_some(current.len())) else {
        return Vec::new();
    };
    let prefix_text = &current[..end];
    let Some(dollar) = prefix_text.rfind('$') else {
        return Vec::new();
    };
    let reference = &prefix_text[dollar + 1..];
    let prefix = reference.strip_prefix('{').unwrap_or(reference);
    if !prefix
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Vec::new();
    }
    // Only offer references in values, never in comments or literal single quotes.
    let before = &prefix_text[..dollar];
    let Some((_, rhs)) = before.split_once('=') else {
        return Vec::new();
    };
    if rhs.trim_start().starts_with('\'') || rhs.contains(" #") || rhs.contains("\t#") {
        return Vec::new();
    }
    let mut candidates: Vec<String> = parse_dotenv(content, false)
        .entries
        .into_iter()
        .map(|entry| entry.key)
        .filter(|key| key.starts_with(prefix))
        .collect();
    candidates.sort();
    candidates.dedup();
    candidates
}
