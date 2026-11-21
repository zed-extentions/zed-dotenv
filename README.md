# zed-dotenv — dotenv & Secret Linter for Zed (`dotenv-lsp`)

An environment variable (`.env`) linter, `.env.example` schema drift analyzer, sanitized template generator, secret masker, and language server extension for Zed.
A pure Rust/WASM (`wasm32-wasip2`) implementation inspired by [mikestead/vscode-dotenv](https://github.com/mikestead/vscode-dotenv) (MIT), [dotenv-linter/dotenv-linter](https://github.com/dotenv-linter/dotenv-linter) (MIT), and [irongeek/vscode-env](https://github.com/irongeek/vscode-env) (MIT) — with **zero dependency on VSCode extensions**.

> Zed provides basic shell-style highlighting for `.env` files out of the box, but lacks schema drift detection against `.env.example`, accidental secret-commit guards, sanitized template generation, and `.vscode/settings.json` (`dotenv.*`) compatibility — this extension fills that gap.

## Demo

Recorded in the real Zed editor (`v1.28.1`) by the [`Feature GIFs`](.github/workflows/demos.yml) workflow (`ci/scenarios.py`); JSON-RPC and CLI traces verify the behavior shown in each recording.

These recordings show Zed using synthetic example data. Any token visible in the editor is a fake test value.

### Comparison with Inspired-by VS Code Extensions

| VS Code Upstream (`mikestead/vscode-dotenv` & `irongeek/vscode-env`, MIT) | Zed (`zed-dotenv`, `v1.28.1`) | Notes |
|---|---|---|
| ![vscode-dotenv](https://raw.githubusercontent.com/mikestead/vscode-dotenv/master/images/screenshot.png)<br>![vscode-env highlighting](https://raw.githubusercontent.com/irongeek/vscode-env/master/images/highlighting.gif) | ![Diagnostics](docs/media/lsp-diagnostics.gif)<br>![Hover](docs/media/lsp-hover.gif) | Syntax highlighting in VS Code vs. native Zed LSP diagnostics (`DOTENV001`–`DOTENV006`), masked secret hover, `Ctrl+.` Quick Fix, and CLI task auditing in `zed-dotenv`. |

### Diagnostics

Duplicate keys and missing `=` are flagged inline as you type, and listed in the diagnostics panel.

![Diagnostics](docs/media/lsp-diagnostics.gif)

### Hover

Hovering a secret shows its key, length, and detected pattern, with the value masked.

![Hover](docs/media/lsp-hover.gif)

### Quick Fix

`Ctrl+.` offers "Remove duplicate definition" and applies the edit.

![Quick Fix](docs/media/lsp-quick-fix.gif)

### Audit task

`dotenv: Audit current environment` runs syntax checks and `.env.example` drift detection.

![Audit task](docs/media/task-audit.gif)

### Mask task

`dotenv: Mask current environment` prints every variable with its value masked.

![Mask task](docs/media/task-mask.gif)

### Template task

`dotenv: Generate sanitized template` replaces secrets with placeholders.

![Template task](docs/media/task-template.gif)

### Matrix task

`dotenv: Compare environment files` shows which keys differ across `.env*` files.

![Matrix task](docs/media/task-matrix.gif)

## 🚀 Editor Native Palette Experience (No Assistant Required)
Zed's headless WASM architecture delegates editor interactions to native palettes rather than the Assistant panel:
* **Code Action Palette (`Ctrl+.` / `Cmd+.`)**: Trigger QuickFixes directly on `.env` lines to strip duplicate keys, replace plaintext secrets with safe placeholders, or format whitespace.
* **Diagnostics Panel (`Ctrl+Shift+M` / `F8`)**: View all syntax, duplicate, and secret warnings with jump-to-line navigation.
* **Inline Hover**: Hover over masked environment variables to inspect key length and classification without leaking raw plaintext secrets.


## Feature Parity with `vscode-dotenv` / `dotenv-linter` / `Envellisense`

### `.env` Syntax Linting & Secret Auditing (Phase 1 — Implemented)
- [x] Line-by-line `.env` lexer supporting `export ` prefix, single/double/unquoted values, escape sequences (`\"`, `\\`, `\n`, `\r`, `\t`, `\$`), CRLF/LF normalization, and inline `#` comments
- [x] `DOTENV001` (`Error`): Missing `=` assignment separator on non-comment, non-blank lines
- [x] `DOTENV002` (`Error`): Invalid environment variable identifier (`[A-Za-z_][A-Za-z0-9_]*`)
- [x] `DOTENV003` (`Error`): Unclosed single/double quotes or unexpected non-comment trailing characters after closing quote
- [x] `DOTENV004` (`Warning`): Duplicate key definitions citing both the current line and the first definition line
- [x] `DOTENV005` (`Warning`): Leading/trailing whitespace around `=` or unquoted multi-word internal spaces
- [x] `DOTENV006` (`Warning` in templates, `Info` in `.env`): Live secret token detection (`AKIA`/`ASIA` AWS keys, `ghp_`/`github_pat_` GitHub tokens, `sk-`/`sk_live_` API keys, Slack `xox*`, Google `AIza*`, PEM private keys, and connection URIs with embedded passwords)

### Audit and editor integration

- Use Zed Tasks for drift analysis, sanitized templates, masked previews, matrix comparisons, source scans, and HTTP readiness checks.
- The language server provides diagnostics, hover, completion, and Quick Fixes.
- Workspace settings can be imported from `.vscode/settings.json`, with Zed settings taking precedence.
- The extension downloads the standalone language server from GitHub Releases when needed.
- Encrypted dotenvx/vault values are recognized.

## Prerequisites

Install `zed-dotenv-lsp` on `PATH` (for example, `cargo install --path crates/dotenv-lsp`) to run the Task templates. Editor LSP integration can also use a configured binary or automatically download one from GitHub Releases. The downloaded extension cache is not necessarily on the terminal PATH.

### Language server binary resolution order

When Zed starts `dotenv-lsp`, `src/lib.rs` resolves the executable in the following deterministic order:

1. **`.zed/settings.json` override (`lsp.dotenv-lsp.binary.path`)** — always takes highest precedence.
2. **`PATH` lookup for `zed-dotenv-lsp`** (`worktree.which("zed-dotenv-lsp")`).
3. **`PATH` lookup for `dotenv-lsp`** (`worktree.which("dotenv-lsp")`).
4. **`PATH` lookup for `dotenv-linter`** (`worktree.which("dotenv-linter")`).
5. **Auto-download standalone `zed-dotenv-lsp`** from `zed-extentions/zed-dotenv` GitHub Releases for `current_platform()` (`linux-x86_64`, `darwin-arm64`, `darwin-x86_64`, `windows-x86_64`) with offline cached binary reuse (`zed-dotenv-lsp-*`).
6. **Actionable install error** — if offline and no cached binary exists, returns instructions on manual installation or setting `lsp.dotenv-lsp.binary.path`.

```json
{
  "lsp": {
    "dotenv-lsp": {
      "binary": {
        "path": "/custom/path/to/dotenv-lsp",
        "arguments": ["--stdio"]
      }
    }
  }
}
```

## Usage

Open a dotenv file and use `task: spawn` to select an audit, mask, template, matrix, scan, or HTTP readiness task. Tasks invoke `zed-dotenv-lsp task` and require the executable on `PATH`. The task templates specify their input paths in `languages/dotenv/tasks.json`.

### 2. Diagnostic Rule Catalog (`DOTENV001`–`DOTENV006`)

| Rule ID | Default Severity | Condition Detected | Example Trigger |
|---|---|---|---|
| `DOTENV001` | `ERROR` | Non-comment, non-empty line missing `=` separator | `JUST_A_WORD` |
| `DOTENV002` | `ERROR` | Key does not match `[A-Za-z_][A-Za-z0-9_]*` (or empty key) | `123_KEY=val`, `MY-KEY=val` |
| `DOTENV003` | `ERROR` | Unclosed quote (`'` or `"`) or trailing non-comment text after closing quote | `KEY="unclosed`, `KEY="val" extra` |
| `DOTENV004` | `WARN` | Key defined multiple times in the same file (later value overrides earlier) | `PORT=3000` ... `PORT=4000` |
| `DOTENV005` | `WARN` | Whitespace around `=` or unquoted multi-word spaces | `KEY = value`, `MSG=hello world` |
| `DOTENV006` | `WARN` (in `.env.example`)<br>`INFO` (in `.env`) | Live credential signature (`AKIA*`, `ghp_*`, `github_pat_*`, `sk-*`, `sk_live_*`, `xox*`, `AIza*`, `-----BEGIN * PRIVATE KEY-----`, URI `user:password@host`) | `AWS_KEY=AKIAIOSFODNN7EXAMPLE` |

### 4. Configuration & `.vscode/settings.json` Compatibility

Configure `dotenv-lsp` in `.zed/settings.json`:

```json
{
  "lsp": {
    "dotenv-lsp": {
      "settings": {
        "enableAutocompletion": true,
        "maskValues": true,
        "revealEdges": false,
        "examplePath": ".env.example",
        "importVscodeSettings": true
      }
    }
  }
}
```

| Setting Key (`.zed/settings.json`) | `.vscode/settings.json` Equivalent | Type | Default | Description |
|---|---|---|---|---|
| `enableAutocompletion` | `dotenv.enableAutocompletion` | `bool` | `true` | Enables environment variable completion in supported languages |
| `maskValues` | `dotenv.maskValues` / `dotenv.MaskValues` | `bool` | `true` | Masks variable values as `***` in hover previews and reports |
| `revealEdges` | `dotenv.revealEdges` | `bool` | `false` | When `false`, masks values strictly as `***`; when `true` (and non-secret), shows `ab***yz` |
| `examplePath` | `dotenv.examplePath` | `string` | `".env.example"` | Default template file path used when `[example-path]` is omitted |
| `importVscodeSettings` | — | `bool` | `true` | Automatically reads `.vscode/settings.json` (JSONC comments and trailing commas supported) as fallback below `.zed/settings.json` |

## Constraints

- **No Custom Sidebar Webview / Inline Text Decorations**: Zed's extension API (`0.7.0`) does not expose VSCode's `TextEditorDecorationType` (used by `vscode-dotenv` for in-buffer value cloaking) or custom webviews. Secret masking is provided in Task output and `dotenv-lsp` hover responses.
- **`scan` Takes a Single File, Not a Directory**: `scan` analyzes exactly one source file per invocation; directory paths such as `src/` or `.` are rejected up front, and any other unreadable path fails with an error. Run the task once per file.
- **Worktree-Relative Text File Reads**: `Worktree::read_text_file` reads UTF-8 files relative to the open project root; absolute filesystem paths outside the worktree are rejected with an explanatory error.

## Installation (Dev Extension)

1. Open Zed Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`) → `zed: install dev extension`
2. Select this repository directory (`zed-dotenv`)
3. Open a project folder containing `.env`. The language server starts for supported files; no Assistant command is needed for editor diagnostics, hover, or code actions.

## Manual Verification Procedure (G5)

1. Install `zed-dotenv` as a dev extension in Zed, then open a project containing a `.env` file. Confirm the `dotenv Language Server` starts in Zed's logs.
2. Put these lines in `.env`:
   - `export PORT=3000`
   - `API_TOKEN=ghp_1234567890abcdefghijklmnopqrstuv`
   - `PORT=4000`
   - `BROKEN_LINE`
   - `UNCLOSED="hello`
3. Save the file. Confirm diagnostics appear on the correct lines: `DOTENV001` for `BROKEN_LINE`, `DOTENV003` for the unclosed quote, `DOTENV004` for the second `PORT`, and `DOTENV006` for `API_TOKEN`. The diagnostic must not include the token value.
4. Place the cursor on the duplicate `PORT` line and open Code Actions with `Ctrl+.` (Windows/Linux) or `Cmd+.` (macOS). Confirm a Quick Fix is offered and applying it removes the duplicate definition.
5. Put the cursor on `BROKEN_LINE` and open Code Actions. Confirm the whitespace/fix action is available where applicable; apply it and verify the resulting edit.
6. Hover over `API_TOKEN`. Confirm the hover shows a masked value and classification without revealing `ghp_1234567890abcdefghijklmnopqrstuv`.
7. Replace the token with `AWS_SECRET=AKIAIOSFODNN7EXAMPLE` and save. Confirm a `DOTENV006` diagnostic appears and the raw value is not shown in the diagnostic or hover.
8. Change `.env` to remove the duplicate key and repair the malformed lines. Confirm the diagnostics clear after saving.

This G5 procedure validates the editor-native LSP experience only. Verify Task workflows separately with the CLI installed on PATH.

## Development

```bash
cargo fmt --all -- --check                      # Formatting check
cargo clippy --all-targets -- -D warnings       # Lint check (0 warnings)
cargo test --workspace                          # Host unit & integration tests (G2)
cargo build --target wasm32-wasip2 --release    # Extension WASM build (G3)
```

- `languages/dotenv/` — Declarative `dotenv` language (mirrors upstream `vscode-dotenv`): `config.toml` (`.env*` / `.flaskenv` file list, `#` comments, brackets) and `highlights.scm` using the pinned [`pnx/tree-sitter-dotenv`](https://github.com/pnx/tree-sitter-dotenv) grammar
- the Tier 2 modules in `src/` — Pure `std` module: `.env` lexer, `DOTENV001`–`DOTENV006` linter, matrix comparator, code reference scanner, HTTP readiness checker, schema drift comparator, secret masker, template generator, JSONC stripper, and `.vscode/settings.json` translator (30 unit tests)
- `src/lib.rs` — Zed Extension WASM boundary: `dotenv-lsp` startup and configuration hooks

Planning & Design: [PLAN.md](PLAN.md) · Requirements: [REQUIREMENTS.md](REQUIREMENTS.md) · Architecture: [ARCHITECTURE.md](ARCHITECTURE.md)

## License

MIT — This extension is an independent Rust/WASM implementation inspired by [mikestead/vscode-dotenv](https://github.com/mikestead/vscode-dotenv) (MIT), [dotenv-linter/dotenv-linter](https://github.com/dotenv-linter/dotenv-linter) (MIT), and [irongeek/vscode-env](https://github.com/irongeek/vscode-env) (MIT).
