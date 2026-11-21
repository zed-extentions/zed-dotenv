# Requirements Specification — zed-dotenv (dotenv & Secret Linter)

- Version: v1.0.0 · Status: **Requirements Approved & Implemented (G1–G4 Complete, G5 Ready)**
- Date: 2026-11-10 · Reference: [mikestead/vscode-dotenv](https://github.com/mikestead/vscode-dotenv) (MIT)
- Priority notation: MUST (Required for v1) / SHOULD (Recommended) / LATER (Backlog)

## 1. Objectives

Provide production-grade environment variable (`.env`) autocompletion, hover/audit secret masking, strict `.env` syntax linting, `.env.example` schema drift detection, and sanitized template generation for Zed via `dotenv-lsp` and the `/dotenv-check` slash command.

## 2. Functional Requirements

| ID | Requirement | Priority | Verification |
|---|---|---|---|
| FR-1 | Launch `zed-dotenv-lsp` (or `dotenv-lsp` / fallback `dotenv-linter`) via `language_server_command` for `.env` diagnostics, hover masking, and code actions, honoring custom `lsp.dotenv-lsp.binary` (`path`, `arguments`, `env`) overrides in `.zed/settings.json` and returning an actionable installation error when none of the binaries are found on `PATH` | MUST | G2, G5 |
| FR-2 | `/dotenv-check [env-path] [example-path]` (and `/dotenv-check check [env-path] [example-path]`) — Audit a workspace `.env` file (default `.env`) for syntax issues and compare keys against an example template (auto-discovering `.env.example`, `.env.sample`, or `.env.template` in the same directory or worktree root when `[example-path]` is omitted) | MUST | G2, G5 |
| FR-3 | Mask sensitive values (`***` or length-annotated `*** (len N)`) in all slash command outputs and previews by default, guaranteeing that raw secret plaintext never leaks into Assistant panel reports | MUST | G2, G5 |
| FR-4 | Parse `.env` files line-by-line preserving 1-based line numbers, `export ` prefixes, single-quoted literals, double-quoted strings with escape sequences (`\"`, `\\`, `\n`, `\r`, `\t`, `\$`), and unquoted values with inline `#` comment stripping | MUST | G2 |
| FR-5 | Emit structured syntax diagnostic `DOTENV001` (`Error`) for non-comment, non-blank lines missing an `=` assignment separator | MUST | G2 |
| FR-6 | Emit structured syntax diagnostic `DOTENV002` (`Error`) for invalid environment variable identifiers (keys must match `[A-Za-z_][A-Za-z0-9_]*`) | MUST | G2 |
| FR-7 | Emit structured syntax diagnostic `DOTENV003` (`Error`) for unclosed single or double quotes in values, or unexpected non-comment trailing characters after a closing quote | MUST | G2 |
| FR-8 | Emit structured syntax diagnostic `DOTENV004` (`Warning`) for duplicate key definitions within the same `.env` file, reporting both the current line and the first definition line | MUST | G2 |
| FR-9 | Emit structured syntax diagnostic `DOTENV005` (`Warning`) for leading/trailing whitespace around `=` (e.g., `KEY = value`) or unquoted internal whitespace in values | MUST | G2 |
| FR-10 | Emit structured security diagnostic `DOTENV006` (`Warning` in `.env.example` / template files, `Info` in `.env` files) when high-risk live secret formats are detected in plaintext (`AKIA...` / `ASIA...` AWS keys, `ghp_...` / `github_pat_...` GitHub tokens, `sk-...` / `sk_live_...` API secret keys, `xox[baprs]-...` Slack tokens, `AIza...` Google keys, or `-----BEGIN ... PRIVATE KEY-----` PEM blocks) | MUST | G2 |
| FR-11 | Compute schema drift between `.env` and `.env.example` (`.env.sample` / `.env.template`), reporting missing keys, extra (undocumented) keys, empty keys where the template provides a non-empty example, and leaked real secrets committed inside the template file | MUST | G2, G5 |
| FR-12 | `/dotenv-check template [env-path]` (alias `export [env-path]`) — Generate a sanitized `.env.example` skeleton from `[env-path]` (default `.env`), preserving comments, `export ` prefixes, and blank lines while replacing sensitive values with `<YOUR_KEY>` placeholders and retaining safe non-secret defaults (`PORT`, `NODE_ENV`, booleans, `localhost` URLs) | MUST | G2, G5 |
| FR-13 | `/dotenv-check mask [env-path]` — Render all `KEY=*** (len N)` entries from `[env-path]` (default `.env`) with value lengths, quote styles, and secret-pattern classifications without exposing plaintext values | MUST | G2, G5 |
| FR-14 | Parse `.vscode/settings.json` (JSONC with `//` and `/* */` comments and trailing commas) and `.zed/settings.json` (`lsp.dotenv-lsp.settings`) to resolve `dotenv.*` settings (`enableAutocompletion`, `maskValues`, `examplePath`, `importVscodeSettings`), forward them via `language_server_initialization_options` and `language_server_workspace_configuration`, and provide `/dotenv-check` argument completions and collapsible `SlashCommandOutputSection` ranges | MUST | G2, G5 |
| FR-15 | Auto-download platform-appropriate standalone `zed-dotenv-lsp` binary from `zed-extentions/zed-dotenv` GitHub Releases on first use when no custom binary or `PATH` binary is found, caching the executable directory (`zed-dotenv-lsp-*`) and reusing cached binaries on subsequent launches | MUST | G2, G3, G5 |
| FR-16 | `/dotenv-check matrix [env-files...]` — Multi-environment matrix audit comparing multiple `.env*` files simultaneously (defaulting to probing `.env.example`, `.env`, `.env.local`, `.env.development`, `.env.staging`, `.env.production`, `.env.test`), rendering a Markdown matrix table with masked values and summarizing inconsistent keys | MUST | G2, G5 |
| FR-17 | `/dotenv-check scan <src-file> [env-path]` — Source-to-env reference scanner inspecting a single source file (directories are unsupported: the Zed extension API cannot list them) across JavaScript/TypeScript (`process.env`, `import.meta.env`), Rust (`std::env::var`, `env::var`, `env!`, `option_env!`), Python (`os.environ`, `os.getenv`), and Go (`os.Getenv`, `os.LookupEnv`) with comment exclusion, reporting undeclared variable references | MUST | G2, G5 |
| FR-18 | `/dotenv-check http <http-path> [env-path]` — Cross-extension `.http` readiness check validating `{{$dotenv KEY}}` and `{{$dotenv %KEY}}` tokens against `.env` and `.env.example` before running HTTP requests | MUST | G2, G5 |
| FR-19 | Support `.env.vault` and `dotenvx` encrypted value classification (`encrypted:`, `vlt_`, `DOTENV_VAULT_*`), classifying them as `[ENCRYPTED: dotenvx/vault]` without triggering false positive `DOTENV006` plaintext leak diagnostics or unintended template sanitization | MUST | G2, G5 |
| FR-20 | Declarative `dotenv` language definition mirroring upstream `mikestead/vscode-dotenv` (`languages/dotenv/config.toml`: `.env*` / `.flaskenv` file list, `#` line comments, bracket/quote pairs; `highlights.scm` via `pnx/tree-sitter-dotenv` grammar pinned in `extension.toml`), and attach `dotenv-lsp` to the `dotenv` and `env` languages | MUST | G3, G5 |
| FR-21 | Standalone `zed-dotenv-lsp` native stdio sidecar daemon (`crates/dotenv-lsp`) hosting Tier 2 handlers with Content-Length JSON-RPC framing (Closes #12) | MUST | G2, G5 |
| FR-22 | Live LSP diagnostics (`textDocument/publishDiagnostics`) for DOTENV001-DOTENV006 emitted on `textDocument/didOpen`, `textDocument/didChange`, and `textDocument/didSave` (Closes #10) | MUST | G2, G5 |
| FR-23 | Native LSP Hover (`textDocument/hover`) showing masked value preview, character length, quote style, classification, and encryption status without exposing plaintext secrets (Closes #13) | MUST | G2, G5 |
| FR-24 | Native QuickFix Code Actions (`textDocument/codeAction`) for duplicate key removal (DOTENV004), whitespace normalization (DOTENV005), and live secret sanitization with placeholder (DOTENV006) (Closes #11) | MUST | G2, G5 |

## 3. Non-Functional Requirements

| ID | Requirement | Priority | Verification |
|---|---|---|---|
| NFR-1 | Zero external crate dependencies for WASM extension (`std` + `zed_extension_api = "0.7.0"` only); `crates/dotenv-lsp` sidecar uses lightweight `serde` and `serde_json` only | MUST | `Cargo.toml` check |
| NFR-2 | Strict tier isolation: the Tier 2 modules in `src/` must not import `zed_extension_api` (pure `std` only) | MUST | G2 (`cargo test`) |
| NFR-3 | Forbid `unsafe` Rust (`[lints.rust] unsafe_code = "forbid"`) | MUST | CI (`cargo clippy`) |
| NFR-4 | Compile cleanly to `wasm32-wasip2` release target with zero warnings | MUST | G3 (`cargo build --target wasm32-wasip2 --release`) |

## 4. Traceability Matrix

| Requirement ID | Implementation Module | Unit Test Coverage (`#[cfg(test)]`) |
|---|---|---|
| FR-1 | `src/lib.rs` (`language_server_command`), the Tier 2 modules in `src/` (`resolve_server_command`) | `audit_tests::resolves_custom_and_fallback_lsp_commands`, `tests::lsp_command_and_id_validation` |
| FR-2 | the Tier 2 modules in `src/` (`example_candidates`) | `audit_tests::example_candidates_prioritizes_configured_then_same_dir_then_root` |
| FR-3 | the Tier 2 modules in `src/` (`mask_value`, `render_audit_report`, `render_masked_preview`) | `audit_tests::mask_value_never_exposes_plaintext_secrets` |
| FR-4 | the Tier 2 modules in `src/` (`parse_dotenv`) | `audit_tests::lexer_parses_quotes_escapes_exports_and_comments` |
| FR-5 | the Tier 2 modules in `src/` (`parse_dotenv` -> `DOTENV001`) | `audit_tests::linter_reports_dotenv001_missing_equals` |
| FR-6 | the Tier 2 modules in `src/` (`parse_dotenv` -> `DOTENV002`) | `audit_tests::linter_reports_dotenv002_invalid_key_identifier` |
| FR-7 | the Tier 2 modules in `src/` (`parse_dotenv` -> `DOTENV003`) | `audit_tests::linter_reports_dotenv003_unclosed_quotes_and_trailing_chars` |
| FR-8 | the Tier 2 modules in `src/` (`parse_dotenv` -> `DOTENV004`) | `audit_tests::linter_reports_dotenv004_duplicate_keys` |
| FR-9 | the Tier 2 modules in `src/` (`parse_dotenv` -> `DOTENV005`) | `audit_tests::linter_reports_dotenv005_whitespace_around_equals_and_unquoted_spaces` |
| FR-10 | the Tier 2 modules in `src/` (`classify_secret_pattern` -> `DOTENV006`) | `audit_tests::linter_reports_dotenv006_secret_patterns_in_env_and_template` |
| FR-11 | the Tier 2 modules in `src/` (`compare_env_files`, `DriftReport`) | `audit_tests::drift_analyzer_detects_missing_extra_empty_and_template_leaks` |
| FR-12 | the Tier 2 modules in `src/` (`generate_template`, `render_template_report`) | `audit_tests::template_generator_preserves_safe_defaults_and_sanitizes_secrets` |
| FR-13 | the Tier 2 modules in `src/` (`render_masked_preview`) | `audit_tests::masked_preview_renders_lengths_and_classifications` |
| FR-14 | the Tier 2 modules in `src/` (`strip_jsonc`, `parse_dotenv_settings`), `src/lib.rs` (LSP config hooks) | `audit_tests::jsonc_and_vscode_settings_resolver` |
| FR-15 | `src/lib.rs` (`auto_download_dotenv_lsp`), the Tier 2 modules in `src/` (`select_dotenv_lsp_asset`, `cached_lsp_binary_rel_path`, `find_cached_lsp_binary`) | `audit_tests::select_dotenv_lsp_asset_and_cached_lsp_binary_rel_path`, `audit_tests::find_cached_lsp_binary_resolves_offline_directory` |
| FR-16 | the Tier 2 modules in `src/` (`render_matrix_report`, `DEFAULT_MATRIX_CANDIDATES`) | `audit_tests::matrix_report_builds_markdown_table_and_reports_inconsistencies` |
| FR-17 | the Tier 2 modules in `src/` (`extract_source_env_refs`, `render_scan_report`, `SourceEnvRef`) | `audit_tests::extract_source_env_refs_across_languages_and_ignores_comments`, `audit_tests::render_scan_report_detects_declared_and_undeclared_keys` |
| FR-18 | the Tier 2 modules in `src/` (`extract_http_dotenv_refs`, `render_http_readiness_report`) | `audit_tests::extract_http_dotenv_refs_and_readiness_report` |
| FR-19 | the Tier 2 modules in `src/` (`is_encrypted_value`, `classify_secret_pattern`, `render_masked_preview`, `compare_env_files`, `is_safe_default_value`) | `audit_tests::encrypted_value_classification_and_dotenv_vault_support` |
| FR-20 | `languages/dotenv/config.toml`, `languages/dotenv/highlights.scm`, `extension.toml` (`[grammars.dotenv]`, `language_servers.dotenv-lsp.languages`) | Declarative only (no Rust logic); verified by G3 build and G5 manual check |
| FR-21 | `crates/dotenv-lsp/src/main.rs` (`DotenvLspServer`, `read_lsp_message`, `write_lsp_message`) | `crates/dotenv-lsp/src/main.rs::tests::test_lsp_read_write_framing`, `test_initialize_handshake` |
| FR-22 | `crates/dotenv-lsp/src/main.rs` (`compute_publish_diagnostics`) | `crates/dotenv-lsp/src/main.rs::tests::test_diagnostics_publish_dotenv001_to_dotenv006` |
| FR-23 | `crates/dotenv-lsp/src/main.rs` (`compute_hover`) | `crates/dotenv-lsp/src/main.rs::tests::test_hover_masking_no_plaintext_leak` |
| FR-24 | `crates/dotenv-lsp/src/main.rs` (`compute_code_actions`), `extension.toml` (`code_action_kinds`) | `crates/dotenv-lsp/src/main.rs::tests::test_code_actions_duplicate_key_and_secret` |
| NFR-1..NFR-4 | `Cargo.toml`, `crates/dotenv-lsp/Cargo.toml`, `src/lib.rs`, the Tier 2 modules in `src/` | Verified by `cargo test --all`, `cargo clippy --all-targets -- -D warnings`, `cargo build --target wasm32-wasip2 --release` |
