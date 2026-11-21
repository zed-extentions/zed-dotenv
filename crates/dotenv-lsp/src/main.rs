//! `zed-dotenv-lsp` — Standalone stdio JSON-RPC Language Server for `.env` files.
//!
//! Exposes native LSP Diagnostics (`DOTENV001`..`DOTENV006`), Hover value masking,
//! and QuickFix Code Actions via stdio framing.
//!
//! Pure domain logic is hosted in the Tier 2 modules under `src/` (Tier 2).

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};

// Tier 2: pure `std` modules (never import `zed_extension_api` from them).
#[allow(dead_code, unused_imports)]
#[path = "../../../src/language_server/assets.rs"]
mod assets;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/language_server/completion.rs"]
mod completion;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/audit/drift.rs"]
mod drift;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/http/mod.rs"]
mod http;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/audit/parser.rs"]
mod parser;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/audit/paths.rs"]
mod paths;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/reports/mod.rs"]
mod report;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/source_scan/mod.rs"]
mod scan;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/security/mod.rs"]
mod secrets;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/language_server/settings.rs"]
mod settings;
#[allow(dead_code, unused_imports)]
#[path = "../../../src/audit/types.rs"]
mod types;

#[allow(unused_imports)]
pub use assets::*;
#[allow(unused_imports)]
pub use completion::*;
#[allow(unused_imports)]
pub use drift::*;
#[allow(unused_imports)]
pub use http::*;
#[allow(unused_imports)]
pub use parser::*;
#[allow(unused_imports)]
pub use paths::*;
#[allow(unused_imports)]
pub use report::*;
#[allow(unused_imports)]
pub use scan::*;
#[allow(unused_imports)]
pub use secrets::*;
#[allow(unused_imports)]
pub use settings::*;
#[allow(unused_imports)]
pub use types::*;

#[cfg(test)]
#[path = "../../../src/tests/mod.rs"]
mod audit_tests;

/// Minimal JSON-RPC 2.0 message reader with `Content-Length` framing.
pub fn read_lsp_message<R: BufRead>(reader: &mut R) -> io::Result<Option<String>> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            // EOF reached
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            // Empty line separates headers from payload
            break;
        }

        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            let len: usize = rest
                .trim()
                .parse()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            content_length = Some(len);
        }
    }

    let Some(length) = content_length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing Content-Length header",
        ));
    };

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let text =
        String::from_utf8(body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(text))
}

/// Minimal JSON-RPC 2.0 message writer with `Content-Length` framing.
pub fn write_lsp_message<W: Write>(writer: &mut W, value: &Value) -> io::Result<()> {
    let payload = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)?;
    writer.flush()
}

pub struct DotenvLspServer {
    documents: HashMap<String, String>,
    settings: DotenvSettings,
}

impl Default for DotenvLspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl DotenvLspServer {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            settings: DotenvSettings::default(),
        }
    }

    pub fn handle_message(&mut self, raw_json: &str) -> Option<Value> {
        let parsed: Value = serde_json::from_str(raw_json).ok()?;
        let method = parsed.get("method").and_then(Value::as_str);
        let id = parsed.get("id").cloned();

        match method {
            Some("initialize") => {
                if let Some(options) = parsed.pointer("/params/initializationOptions") {
                    self.apply_settings(options);
                }
                let id = id.unwrap_or(Value::Number(1.into()));
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "capabilities": {
                            "textDocumentSync": 1, // Full sync
                            "hoverProvider": true,
                            "completionProvider": { "triggerCharacters": ["$", "{"] },
                            "codeActionProvider": {
                                "codeActionKinds": ["quickfix", "source.fixAll"]
                            }
                        },
                        "serverInfo": {
                            "name": "zed-dotenv-lsp",
                            "version": "0.1.0"
                        }
                    }
                }))
            }
            Some("initialized") => None,
            Some("workspace/didChangeConfiguration") => {
                if let Some(settings) = parsed.pointer("/params/settings") {
                    self.apply_settings(settings);
                }
                None
            }
            Some("shutdown") => {
                let id = id.unwrap_or(Value::Null);
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": null
                }))
            }
            Some("textDocument/didOpen") => {
                if let Some(params) = parsed.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let (Some(uri), Some(text)) = (
                            doc.get("uri").and_then(Value::as_str),
                            doc.get("text").and_then(Value::as_str),
                        ) {
                            self.documents.insert(uri.to_string(), text.to_string());
                            return Some(self.compute_publish_diagnostics(uri, text));
                        }
                    }
                }
                None
            }
            Some("textDocument/didChange") => {
                if let Some(params) = parsed.get("params") {
                    let uri = params
                        .get("textDocument")
                        .and_then(|d| d.get("uri"))
                        .and_then(Value::as_str);
                    let changes = params.get("contentChanges").and_then(Value::as_array);
                    if let (Some(uri), Some(changes)) = (uri, changes) {
                        if let Some(first_change) = changes.first() {
                            if let Some(text) = first_change.get("text").and_then(Value::as_str) {
                                self.documents.insert(uri.to_string(), text.to_string());
                                return Some(self.compute_publish_diagnostics(uri, text));
                            }
                        }
                    }
                }
                None
            }
            Some("textDocument/didSave") => {
                if let Some(params) = parsed.get("params") {
                    let uri = params
                        .get("textDocument")
                        .and_then(|d| d.get("uri"))
                        .and_then(Value::as_str);
                    if let Some(uri) = uri {
                        if let Some(text) = self.documents.get(uri).cloned() {
                            return Some(self.compute_publish_diagnostics(uri, &text));
                        }
                    }
                }
                None
            }
            Some("textDocument/didClose") => {
                if let Some(params) = parsed.get("params") {
                    if let Some(uri) = params
                        .get("textDocument")
                        .and_then(|d| d.get("uri"))
                        .and_then(Value::as_str)
                    {
                        self.documents.remove(uri);
                    }
                }
                None
            }
            Some("textDocument/completion") => {
                let id = id?;
                let params = parsed.get("params")?;
                let uri = params.pointer("/textDocument/uri")?.as_str()?;
                let line = params.pointer("/position/line")?.as_u64()? as usize;
                let character = params.pointer("/position/character")?.as_u64()? as usize;
                let items: Vec<Value> = if self.settings.enable_autocompletion {
                    self.documents
                        .get(uri)
                        .map(|text| {
                            let prefix_len = text
                                .lines()
                                .nth(line)
                                .map(|line_text| {
                                    let prefix: Vec<u16> =
                                        line_text.encode_utf16().take(character).collect();
                                    prefix.iter().rev().take_while(|unit| {
                                matches!(**unit, 48..=57 | 65..=90 | 95 | 97..=122)
                            }).count()
                                })
                                .unwrap_or(0);
                            completion_candidates(text, line, character).into_iter()
                            .map(|key| json!({"label": key, "kind": 6, "textEdit": {
                                "range": {"start": {"line":line,"character":character-prefix_len},
                                          "end": {"line":line,"character":character}},
                                "newText": key
                            }})).collect()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                Some(json!({"jsonrpc":"2.0", "id":id, "result":items}))
            }
            Some("textDocument/hover") => {
                let id = id?;
                let params = parsed.get("params")?;
                let uri = params.get("textDocument")?.get("uri")?.as_str()?;
                let position = params.get("position")?;
                let line_0_based = position.get("line")?.as_u64()? as usize;
                let col_0_based = position.get("character")?.as_u64()? as usize;

                let result = self.compute_hover(uri, line_0_based, col_0_based);
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                }))
            }
            Some("textDocument/codeAction") => {
                let id = id?;
                let params = parsed.get("params")?;
                let uri = params.get("textDocument")?.get("uri")?.as_str()?;
                let range = params.get("range")?;
                let context = params.get("context")?;

                let actions = self.compute_code_actions(uri, range, context);
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": actions
                }))
            }
            _ => {
                // If it's a request with an ID that we don't handle, return empty/null or method not found
                id.map(|id| {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    })
                })
            }
        }
    }

    /// Treat each configuration payload as a full replacement, restoring defaults
    /// for removed settings. Missing or malformed fields never enable disclosure.
    fn apply_settings(&mut self, payload: &Value) {
        let config = payload.get("dotenv").unwrap_or(payload);
        let defaults = DotenvSettings::default();
        self.settings = DotenvSettings {
            enable_autocompletion: config
                .get("enableAutocompletion")
                .and_then(Value::as_bool)
                .unwrap_or(defaults.enable_autocompletion),
            mask_values: config
                .get("maskValues")
                .and_then(Value::as_bool)
                .unwrap_or(defaults.mask_values),
            reveal_edges: config
                .get("revealEdges")
                .and_then(Value::as_bool)
                .unwrap_or(defaults.reveal_edges),
            example_path: config
                .get("examplePath")
                .and_then(Value::as_str)
                .map(str::to_owned),
            import_vscode_settings: defaults.import_vscode_settings,
        };
    }

    pub fn compute_publish_diagnostics(&self, uri: &str, text: &str) -> Value {
        let is_template =
            uri.contains(".example") || uri.contains(".sample") || uri.contains(".template");
        let report = parse_dotenv(text, is_template);
        let lines: Vec<&str> = text.lines().collect();

        let mut diagnostics = Vec::new();

        for issue in report.diagnostics {
            let line_0_based = issue.line.saturating_sub(1);
            let line_len = lines
                .get(line_0_based)
                .map(|l| l.encode_utf16().count())
                .unwrap_or(0);

            let severity = match issue.severity {
                Severity::Error => 1,
                Severity::Warning => 2,
                Severity::Info => 3,
            };

            diagnostics.push(json!({
                "range": {
                    "start": { "line": line_0_based, "character": 0 },
                    "end": { "line": line_0_based, "character": line_len }
                },
                "severity": severity,
                "code": issue.code,
                "source": "dotenv-lsp",
                "message": issue.message
            }));
        }

        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": diagnostics
            }
        })
    }

    pub fn compute_hover(
        &self,
        uri: &str,
        line_0_based: usize,
        _col_0_based: usize,
    ) -> Option<Value> {
        let text = self.documents.get(uri)?;
        let line_1_based = line_0_based + 1;
        let is_template =
            uri.contains(".example") || uri.contains(".sample") || uri.contains(".template");
        let report = parse_dotenv(text, is_template);

        let entry = report.entries.iter().find(|e| e.line == line_1_based)?;

        let masked = hover_value(&entry.key, &entry.value, &self.settings);
        let quote_label = match entry.quote_style {
            QuoteStyle::Single => "Single ('... ')",
            QuoteStyle::Double => "Double (\"...\")",
            QuoteStyle::Unquoted => "Unquoted",
        };

        let secret_classification = classify_secret_pattern(&entry.key, &entry.value);
        let is_enc = is_encrypted_value(&entry.key, &entry.value);

        let mut markdown = format!(
            "### \u{1f511} `.env` Variable: `{}`\n\n\
            - **Value**: `{}`\n\
            - **Length**: {} characters\n\
            - **Quote Style**: {}\n",
            entry.key,
            masked,
            entry.value.len(),
            quote_label
        );

        if let Some(classification) = secret_classification {
            markdown.push_str(&format!(
                "- **Pattern**: \u{26a0}\u{fe0f} `{classification}`\n"
            ));
        }

        if is_enc {
            markdown.push_str("- **Status**: \u{1f512} `[ENCRYPTED: dotenvx/vault]`\n");
        }

        if entry.has_export {
            markdown.push_str("- **Export**: `export` keyword active\n");
        }

        Some(json!({
            "contents": {
                "kind": "markdown",
                "value": markdown
            }
        }))
    }

    pub fn compute_code_actions(&self, uri: &str, range: &Value, context: &Value) -> Vec<Value> {
        let mut actions = Vec::new();
        let Some(diagnostics) = context.get("diagnostics").and_then(Value::as_array) else {
            return actions;
        };
        let Some(text) = self.documents.get(uri) else {
            return actions;
        };
        let lines: Vec<&str> = text.lines().collect();
        let parsed = parse_dotenv(text, false);
        let start = range
            .pointer("/start/line")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let end = range
            .pointer("/end/line")
            .and_then(Value::as_u64)
            .unwrap_or(start);

        for diag in diagnostics {
            if diag
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|s| s != "dotenv-lsp")
            {
                continue;
            }
            let Some(line) = diag.pointer("/range/start/line").and_then(Value::as_u64) else {
                continue;
            };
            if line < start || line > end {
                continue;
            }
            let Some(content) = lines.get(line as usize).copied() else {
                continue;
            };
            let code = diag.get("code").and_then(Value::as_str).unwrap_or_default();
            // A client can send diagnostics from an earlier document version.
            if !parsed
                .diagnostics
                .iter()
                .any(|issue| issue.line == line as usize + 1 && issue.code == code)
            {
                continue;
            }
            let end_character = content.encode_utf16().count();
            let (title, replacement, edit_end) = match code {
                "DOTENV004" => {
                    let has_next_line = (line as usize + 1) < lines.len() || text.ends_with('\n');
                    let edit_end = if has_next_line {
                        json!({"line": line + 1, "character": 0})
                    } else {
                        json!({"line": line, "character": end_character})
                    };
                    (
                        format!("Remove duplicate definition on line {}", line + 1),
                        String::new(),
                        edit_end,
                    )
                }
                "DOTENV005" => {
                    let Some((key, value)) = content.split_once('=') else {
                        continue;
                    };
                    if key == key.trim() && value == value.trim_start() {
                        continue;
                    }
                    (
                        format!("Remove spaces around '=' for `{}`", key.trim()),
                        format!("{}={}", key.trim(), value.trim_start()),
                        json!({"line": line, "character": end_character}),
                    )
                }
                "DOTENV006" => {
                    let Some((key, _)) = content.split_once('=') else {
                        continue;
                    };
                    let Some(entry) = parsed
                        .entries
                        .iter()
                        .find(|entry| entry.line == line as usize + 1)
                    else {
                        continue;
                    };
                    let clean = &entry.key;
                    let comment = entry
                        .inline_comment
                        .as_deref()
                        .and_then(|comment| {
                            content.rfind(comment).map(|index| {
                                let prefix = &content[..index];
                                let start = prefix.trim_end_matches([' ', '\t']).len();
                                &content[start..]
                            })
                        })
                        .unwrap_or("");
                    (
                        format!("Replace live secret in `{clean}` with placeholder"),
                        format!("{key}=<YOUR_{clean}>{comment}"),
                        json!({"line": line, "character": end_character}),
                    )
                }
                _ => continue,
            };
            actions.push(json!({"title": title, "kind": "quickfix", "isPreferred": true, "diagnostics": [diag], "edit": {"changes": {uri: [{"range": {"start": {"line": line, "character": 0}, "end": edit_end}, "newText": replacement}]}}}));
        }
        actions
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("task") {
        if let Err(error) = run_task_cli(args.collect()) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    let mut server = DotenvLspServer::new();

    while let Ok(Some(msg)) = read_lsp_message(&mut reader) {
        if let Some(resp) = server.handle_message(&msg) {
            let _ = write_lsp_message(&mut writer, &resp);
        }
    }
}

fn run_task_cli(args: Vec<String>) -> Result<(), String> {
    use crate::{DotenvSettings, Subcommand};

    let Some(action) = args.first() else {
        return Err(
            "usage: zed-dotenv-lsp task <check|mask|template|matrix|scan|http> [paths...]".into(),
        );
    };
    let paths = &args[1..];
    let default_env = || DEFAULT_ENV_PATH.to_string();
    let one_path = || paths.first().cloned().unwrap_or_else(default_env);
    let subcommand = match action.as_str() {
        "check" | "audit" => Subcommand::Check {
            env_path: one_path(),
            example_path: paths.get(1).cloned(),
        },
        "mask" => Subcommand::Mask {
            env_path: one_path(),
        },
        "template" => Subcommand::Template {
            env_path: one_path(),
        },
        "matrix" => Subcommand::Matrix {
            files: paths.to_vec(),
        },
        "scan" => {
            let src_path = paths
                .first()
                .ok_or("scan requires a source-file path")?
                .clone();
            Subcommand::Scan {
                src_path,
                env_path: paths.get(1).cloned().unwrap_or_else(default_env),
            }
        }
        "http" => {
            let http_path = paths
                .first()
                .ok_or("http requires an HTTP-file path")?
                .clone();
            Subcommand::Http {
                http_path,
                env_path: paths.get(1).cloned().unwrap_or_else(default_env),
            }
        }
        _ => {
            return Err(format!(
            "unknown task action `{action}`; expected check, mask, template, matrix, scan, or http"
        ))
        }
    };
    let settings = DotenvSettings::default();
    let read = |path: &str| {
        std::fs::read_to_string(path).map_err(|error| format!("could not read `{path}`: {error}"))
    };
    let find_example = |env_path: &str| {
        example_candidates(env_path, settings.example_path.as_deref())
            .into_iter()
            .find_map(|path| read(&path).ok().map(|content| (path, content)))
    };

    let output = match &subcommand {
        Subcommand::Check {
            env_path,
            example_path,
        } => {
            let env = read(env_path)?;
            let example = match example_path {
                Some(path) => Some((path.clone(), read(path)?)),
                None => find_example(env_path),
            };
            let example_ref = example
                .as_ref()
                .map(|(path, content)| (path.as_str(), content.as_str()));
            render_audit_report(env_path, &env, example_ref, &settings, None)
        }
        Subcommand::Template { env_path } => {
            render_template_report(env_path, &read(env_path)?, None)
        }
        Subcommand::Mask { env_path } => {
            render_masked_preview(env_path, &read(env_path)?, &settings, None)
        }
        Subcommand::Matrix { files } => {
            let candidates = if files.is_empty() {
                DEFAULT_MATRIX_CANDIDATES
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                files.clone()
            };
            let contents: Vec<(String, String)> = candidates
                .into_iter()
                .filter_map(|p| read(&p).ok().map(|c| (p, c)))
                .collect();
            let refs: Vec<(&str, &str)> = contents
                .iter()
                .map(|(p, c)| (p.as_str(), c.as_str()))
                .collect();
            render_matrix_report(&refs, true)
        }
        Subcommand::Scan { src_path, env_path } => {
            let src = read(src_path)?;
            let env = read(env_path)?;
            render_scan_report(src_path, &src, env_path, &env, None, None)
        }
        Subcommand::Http {
            http_path,
            env_path,
        } => {
            let http = read(http_path)?;
            let env = read(env_path)?;
            render_http_readiness_report(http_path, &http, env_path, &env, None, None)
        }
    };
    print!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_read_write_framing() {
        let msg = json!({"jsonrpc": "2.0", "id": 1, "result": "ok"});
        let mut buf = Vec::new();
        write_lsp_message(&mut buf, &msg).unwrap();

        let mut reader = BufReader::new(&buf[..]);
        let read = read_lsp_message(&mut reader).unwrap().expect("message");
        let parsed: Value = serde_json::from_str(&read).unwrap();
        assert_eq!(parsed["result"], "ok");
    }

    #[test]
    fn test_initialize_handshake() {
        let mut server = DotenvLspServer::new();
        let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = server.handle_message(init_req).expect("response");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["capabilities"]["hoverProvider"], true);
        assert_eq!(resp["result"]["serverInfo"]["name"], "zed-dotenv-lsp");
    }

    #[test]
    fn test_diagnostics_publish_dotenv001_to_dotenv006() {
        let mut server = DotenvLspServer::new();
        let open_req = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///project/.env",
                    "text": "INVALID_NO_EQUALS\nFOO = bar\nDUP=1\nDUP=2\nAPI_KEY=AKIAIOSFODNN7EXAMPLE\n"
                }
            }
        });

        let resp = server
            .handle_message(&open_req.to_string())
            .expect("diagnostics emitted");
        assert_eq!(resp["method"], "textDocument/publishDiagnostics");
        let diags = resp["params"]["diagnostics"].as_array().expect("array");

        let codes: Vec<&str> = diags.iter().filter_map(|d| d["code"].as_str()).collect();
        assert!(
            codes.contains(&"DOTENV001"),
            "Expected DOTENV001 missing '='"
        );
        assert!(
            codes.contains(&"DOTENV005"),
            "Expected DOTENV005 whitespace"
        );
        assert!(
            codes.contains(&"DOTENV004"),
            "Expected DOTENV004 duplicate key"
        );
        assert!(
            codes.contains(&"DOTENV006"),
            "Expected DOTENV006 secret pattern"
        );
    }

    #[test]
    fn test_hover_masking_no_plaintext_leak() {
        let mut server = DotenvLspServer::new();
        let secret = "AKIAIOSFODNN7EXAMPLE";
        let open_req = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///project/.env",
                    "text": format!("AWS_SECRET_KEY={secret}\n")
                }
            }
        });
        server.handle_message(&open_req.to_string());

        let hover_req = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///project/.env" },
                "position": { "line": 0, "character": 5 }
            }
        });

        let resp = server
            .handle_message(&hover_req.to_string())
            .expect("hover response");
        let contents = resp["result"]["contents"]["value"]
            .as_str()
            .expect("markdown");

        // CRITICAL SECURITY ASSERTION: Raw plaintext secret must NEVER appear in hover
        assert!(
            !contents.contains(secret),
            "Security leak: Hover contains plaintext secret!"
        );
        assert!(
            contents.contains("AKIA***MPLE") || contents.contains("***"),
            "Expected masked value"
        );
        assert!(contents.contains("20 characters"));
    }

    #[test]
    fn test_code_actions_duplicate_key_and_secret() {
        let mut server = DotenvLspServer::new();
        let open_req = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///project/.env.example",
                    "text": "AWS_KEY=AKIAIOSFODNN7EXAMPLE\n"
                }
            }
        });
        server.handle_message(&open_req.to_string());

        let ca_req = json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": "file:///project/.env.example" },
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } },
                "context": {
                    "diagnostics": [
                        {
                            "code": "DOTENV006",
                            "message": "Secret in template",
                            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 28 } }
                        }
                    ]
                }
            }
        });

        let resp = server
            .handle_message(&ca_req.to_string())
            .expect("code actions");
        let actions = resp["result"].as_array().expect("actions array");
        assert!(
            !actions.is_empty(),
            "Expected at least 1 QuickFix code action"
        );
        let title = actions[0]["title"].as_str().unwrap();
        assert!(title.contains("Replace live secret"));
    }
    #[test]
    fn diagnostics_and_final_line_edits_use_utf16_positions() {
        let mut server = DotenvLspServer::new();
        let uri = "file:///test/.env";
        let text = "A=1\r\nA=??";
        server.documents.insert(uri.into(), text.into());
        let diagnostics = server.compute_publish_diagnostics(uri, text);
        let duplicate = diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "DOTENV004")
            .unwrap()
            .clone();
        assert_eq!(duplicate["range"]["end"]["character"], 4);
        let range = json!({"start":{"line":1,"character":0},"end":{"line":1,"character":4}});
        let actions = server.compute_code_actions(uri, &range, &json!({"diagnostics":[duplicate]}));
        let edit = &actions[0]["edit"]["changes"][uri][0];
        assert_eq!(edit["range"]["end"], json!({"line":1,"character":4}));
        assert_eq!(edit["newText"], "");
    }

    #[test]
    fn whitespace_fix_preserves_quoted_value_and_comment() {
        let mut server = DotenvLspServer::new();
        let uri = "file:///test/.env";
        let text = "export KEY = \"hello world\" # note\r\n";
        server.documents.insert(uri.into(), text.into());
        let diagnostics = server.compute_publish_diagnostics(uri, text);
        let diag = diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "DOTENV005")
            .unwrap()
            .clone();
        let range = json!({"start":{"line":0,"character":0},"end":{"line":0,"character":10}});
        let actions = server.compute_code_actions(uri, &range, &json!({"diagnostics":[diag]}));
        let edit = &actions[0]["edit"]["changes"][uri][0];
        assert_eq!(edit["newText"], "export KEY=\"hello world\" # note");
        assert_eq!(edit["range"]["end"]["line"], 0);
    }

    #[test]
    fn secret_quick_fix_preserves_export_comment_and_line_ending() {
        let mut server = DotenvLspServer::new();
        let uri = "file:///project/.env.example";
        let text = "export\tAWS_KEY=\"AKIAIOSFODNN7EXAMPLE\"  # 운영 😀\r\nNEXT=ok\r\n";
        let open = json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"text":text}}});
        let published = server.handle_message(&open.to_string()).unwrap();
        let diagnostic = published["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|diag| diag["code"] == "DOTENV006")
            .unwrap();
        let request = json!({"jsonrpc":"2.0","id":5,"method":"textDocument/codeAction","params":{
            "textDocument":{"uri":uri},
            "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},
            "context":{"diagnostics":[diagnostic]}
        }});
        let response = server.handle_message(&request.to_string()).unwrap();
        let edit = &response["result"][0]["edit"]["changes"][uri][0];
        assert_eq!(edit["newText"], "export\tAWS_KEY=<YOUR_AWS_KEY>  # 운영 😀");
        let line = text.lines().next().unwrap();
        assert_eq!(
            edit["range"]["end"]["character"],
            line.encode_utf16().count()
        );
        assert_eq!(edit["range"]["end"]["line"], 0);
    }

    #[test]
    fn code_actions_ignore_stale_foreign_and_out_of_range_diagnostics() {
        let mut server = DotenvLspServer::new();
        let uri = "file:///project/.env";
        server.documents.insert(uri.into(), "A=1\nA=2\nB=3".into());
        let range = json!({"start":{"line":1,"character":0},"end":{"line":1,"character":3}});
        let stale = json!({"code":"DOTENV006","source":"dotenv-lsp","range":{"start":{"line":1,"character":0}}});
        let foreign = json!({"code":"DOTENV004","source":"other-lsp","range":{"start":{"line":1,"character":0}}});
        let outside = json!({"code":"DOTENV004","source":"dotenv-lsp","range":{"start":{"line":2,"character":0}}});
        let actions = server.compute_code_actions(
            uri,
            &range,
            &json!({"diagnostics":[stale,foreign,outside]}),
        );
        assert!(actions.is_empty());
    }

    #[test]
    fn completion_respects_settings_and_exposes_names_only() {
        let mut server = DotenvLspServer::new();
        let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"initializationOptions":{"dotenv":{"enableAutocompletion":false}}}});
        let response = server.handle_message(&init.to_string()).unwrap();
        assert!(response["result"]["capabilities"]["completionProvider"].is_object());
        let uri = "file:///project/.env";
        let open = json!({"method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"text":"API_KEY=secret\nREF=${API"}}});
        server.handle_message(&open.to_string());
        let request = json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":9}}});
        assert_eq!(
            server.handle_message(&request.to_string()).unwrap()["result"],
            json!([])
        );
        server.handle_message(&json!({"method":"workspace/didChangeConfiguration","params":{"settings":{"dotenv":{"enableAutocompletion":true}}}}).to_string());
        let response = server.handle_message(&request.to_string()).unwrap();
        assert_eq!(response["result"][0]["label"], "API_KEY");
        assert_eq!(
            response["result"][0]["textEdit"]["range"]["start"]["character"],
            6
        );
        assert_eq!(
            response["result"][0]["textEdit"]["range"]["end"]["character"],
            9
        );
        assert!(!response.to_string().contains("secret"));
    }

    #[test]
    fn initialize_and_live_settings_affect_hover_safely() {
        let mut server = DotenvLspServer::new();
        let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"initializationOptions":{"dotenv":{"maskValues":false,"enableAutocompletion":false,"revealEdges":true}}}});
        server.handle_message(&initialize.to_string());
        assert!(!server.settings.enable_autocompletion);
        let uri = "file:///project/.env";
        let open = json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"text":"NODE_ENV=production\nAPI_TOKEN=ghp_1234567890abcdefghijkl\n"}}});
        server.handle_message(&open.to_string());
        let value = server.compute_hover(uri, 0, 0).unwrap().to_string();
        assert!(value.contains("production"));
        let secret = server.compute_hover(uri, 1, 0).unwrap().to_string();
        assert!(!secret.contains("ghp_1234567890abcdefghijkl"));
        assert!(secret.contains("***"));
        let change = json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{"settings":{"dotenv":{"maskValues":true,"revealEdges":true}}}});
        assert!(server.handle_message(&change.to_string()).is_none());
        let value = server.compute_hover(uri, 0, 0).unwrap().to_string();
        assert!(value.contains("pr***on"));
        assert!(!value.contains("production"));
        assert!(server.settings.enable_autocompletion);
        let invalid = json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{"settings":{"dotenv":{"maskValues":"false","revealEdges":null}}}});
        server.handle_message(&invalid.to_string());
        assert!(server.settings.mask_values);
        assert!(!server.settings.reveal_edges);
        assert!(server
            .compute_hover(uri, 0, 0)
            .unwrap()
            .to_string()
            .contains("***"));
    }
}
