# PLAN — zed-dotenv (Two-Phase Development Plan)

Reference Format / Concept: [mikestead/vscode-dotenv](https://github.com/mikestead/vscode-dotenv), [dotenv-linter/dotenv-linter](https://github.com/dotenv-linter/dotenv-linter) (MIT)
Target: Zed extension (`dotenv-lsp`), Rust/WASM (`wasm32-wasip2`, `zed_extension_api = "0.7.0"`).

---

## 1. Architectural Direction: Zero VSCode Dependency + Zed-Native Evolution

`zed-dotenv` has **zero runtime or packaging dependency on any VSCode extension**.
- **Phase 1 (Baseline — Completed v1.0)**: Self-contained pure Rust `.env` lexer, syntax/security linter (`DOTENV001`..`DOTENV006`), `.env.example` schema drift analyzer, template generator, and standalone `dotenv-lsp` / `dotenv-linter` language server bridge.
- **Phase 2 (Zed-Native Evolution — Planned v2.0)**: Evolve beyond static `.env` file linting into an AI-ready environment governance tool integrated with Zed's Assistant panel and `zed-http`.

---

## 2. Phase 1: Zero-VSCode-Dependency Baseline Implementation (v1.0 — Completed)

### 2.1 Scope & Milestones
1. **Pure `std` `.env` Lexer & Linter (the Tier 2 modules in `src/`)** — *[Completed]*
   - Line-by-line lexer preserving 1-based line numbers, `export ` prefixes, single/double/unquoted values, escape sequences (`\"`, `\\`, `\n`, `\r`, `\t`, `\$`), and inline `#` comments.
   - Diagnostic rules:
     - `DOTENV001` (`Error`): Missing `=` separator.
     - `DOTENV002` (`Error`): Invalid variable identifier (`[A-Za-z_][A-Za-z0-9_]*`).
     - `DOTENV003` (`Error`): Unclosed quote or unexpected trailing characters after closing quote.
     - `DOTENV004` (`Warning`): Duplicate key definition (with first-defined line reference).
     - `DOTENV005` (`Warning`): Whitespace around `=` or unquoted multi-word spaces.
     - `DOTENV006` (`Warning` / `Info`): Live secret pattern detection (`AKIA`/`ASIA` AWS keys, `ghp_`/`github_pat_` GitHub tokens, `sk-` API keys, Slack `xox*`, Google `AIza*`, PEM private keys, and embedded URI passwords).
2. **Schema Drift Analyzer (`.env` vs `.env.example`)** — *[Completed]*
   - Auto-discover `.env.example`, `.env.sample`, or `.env.template` in the target directory or worktree root.
   - Report missing keys, undocumented extra keys, empty required keys, and leaked real secrets inside `.env.example`.
3. **`/dotenv-check` Slash Command Modes** — *[Completed]*
   - `/dotenv-check [env-path] [example-path]`: Full syntax audit + drift report with strict `***` secret masking.
   - `/dotenv-check template [env-path]` (alias `export`): Generate sanitized `.env.example` skeleton preserving comments and safe defaults (`PORT`, `NODE_ENV`, booleans, `localhost` URLs) while replacing secrets with `<YOUR_KEY>`.
   - `/dotenv-check mask [env-path]`: Render `KEY=*** (len N, quote-style, classification)` preview.
4. **Standalone `dotenv-lsp` Host Wiring (`src/lib.rs`)** — *[Completed]*
   - Resolve `lsp.dotenv-lsp.binary` overrides in `.zed/settings.json` -> `worktree.which("dotenv-lsp")` -> `worktree.which("dotenv-linter")`.
   - Forward workspace settings via `language_server_initialization_options` and `language_server_workspace_configuration`.

### 2.2 Module Structure
```text
zed-dotenv/
├── Cargo.toml          # Package zed_dotenv, zed_extension_api = "0.7.0", cdylib
├── extension.toml      # id="dotenv-lsp", slash_commands.dotenv-check, language_servers.dotenv-lsp
├── ARCHITECTURE.md     # Runtime delta and pipeline architecture
├── REQUIREMENTS.md     # FR-1..FR-14 / NFR-1..NFR-4 single source of truth
├── PLAN.md             # This two-phase development plan
├── README.md           # Usage, lint rules, LSP configuration, constraints, and G5 verification
├── languages/dotenv/   # Declarative dotenv language: config.toml, highlights.scm (FR-20)
└── src/
    ├── AGENTS.md       # Tier 1 / Tier 2 isolation guide
    ├── lib.rs          # Tier 1: Extension trait implementation & host tests
    ├── audit/           # Tier 2: types.rs (models), paths.rs (key/path helpers), parser.rs (lexer/linter), drift.rs (drift & template)
    ├── http/            # Tier 2: .http {{$dotenv KEY}} readiness check
    ├── language_server/ # Tier 2: assets.rs (release asset selection & cache path), completion.rs ($NAME completion), settings.rs (JSONC settings & server command)
    ├── reports/         # Tier 2: Markdown renderers (audit, template, mask, matrix)
    ├── security/        # Tier 2: secret classification, sensitivity heuristics, masking
    ├── source_scan/     # Tier 2: source-to-env reference scanner
    └── tests/           # Host unit tests for the Tier 2 modules (`audit_tests`)
```

---

## 3. Phase 2: Zed-Native Evolution Plan (v2.0 — Completed)

Rather than mimicking VSCode UI panels, Phase 2 leverages Zed's native strengths (GitHub Release binary auto-provisioning, Assistant context sections, and cross-extension synergy):

| Milestone ID | Feature | Description & Zed-Native Value | Status |
|---|---|---|---|
| **P2-1** | **Zero-Setup `zed-dotenv-lsp` Auto-Provisioning** (`FR-15`) | Use `latest_github_release("zed-extentions/zed-dotenv", ...)` + `download_file` in `src/lib.rs` to automatically download and cache the standalone `zed-dotenv-lsp` binary per platform (`current_platform()`) with offline cached fallback. | **Completed** |
| **P2-2** | **Multi-Environment Matrix Audit (`/dotenv-check matrix`)** (`FR-16`) | Compare `.env`, `.env.development`, `.env.staging`, and `.env.production` simultaneously in a single Markdown matrix table so AI agents can immediately spot missing keys across deployment tiers. | **Completed** |
| **P2-3** | **Source-to-Env Reference Scanner (`/dotenv-check scan <src-file>`)** (`FR-17`) | Scan source files (`process.env.X`, `import.meta.env.X`, `std::env::var("X")`, `os.environ["X"]`, `os.Getenv("X")`) against `.env` and `.env.example` to detect code references to undeclared environment variables. | **Completed** |
| **P2-4** | **Cross-Extension Synergy with `zed-http`** (`FR-18`) | Validate `{{$dotenv KEY}}` and `{{$dotenv %KEY}}` references inside `.http` files against workspace `.env` files (`Click-free API readiness check`). | **Completed** |
| **FR-19** | **Encrypted Value Classification** | Recognize `.env.vault` and `dotenvx` encrypted values (`encrypted:...`, `vlt_...`, `DOTENV_VAULT_*`) as `[ENCRYPTED: dotenvx/vault]` without false positive `DOTENV006` plaintext leak warnings. | **Completed** |

---

## 4. Constraints & Testing Strategy

- **Constraints**: No custom Webview UI (`zed_extension_api` 0.7.0 ceiling); direct file reads use `Worktree::read_text_file` (worktree-relative).
- **Testing Gate (G2/G3)**:
  - `src/tests/mod.rs`: 30 host unit tests covering lexer, linter, drift, templates, matrix, scanner, http readiness, asset selection/cache resolution, and encrypted values.
  - `src/lib.rs #[cfg(test)]`: 13 pipeline and section-range unit tests covering check, template, mask, matrix, scan, http, and completions.
  - Every Phase 2 milestone (`P2-*`) has corresponding `FR-*` entries in `REQUIREMENTS.md` and comprehensive unit tests.
