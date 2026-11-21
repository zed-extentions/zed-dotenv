//! Host unit tests for the pure Tier 2 domain.

use super::*;

#[test]
fn lexer_parses_quotes_escapes_exports_and_comments() {
    let src = "\
# Database config
export DB_HOST=localhost # inline comment
DB_PORT='5432'
GREETING=\"Hello\\n\\\"World\\\"\\\\End\" # after double quote
EMPTY_VAL=
HASH_IN_VAL=abc#123
";
    let report = parse_dotenv(src, false);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(report.entries.len(), 5);

    assert_eq!(report.entries[0].key, "DB_HOST");
    assert_eq!(report.entries[0].value, "localhost");
    assert!(report.entries[0].has_export);
    assert_eq!(
        report.entries[0].inline_comment.as_deref(),
        Some("# inline comment")
    );
    assert_eq!(report.entries[0].quote_style, QuoteStyle::Unquoted);

    assert_eq!(report.entries[1].key, "DB_PORT");
    assert_eq!(report.entries[1].value, "5432");
    assert_eq!(report.entries[1].quote_style, QuoteStyle::Single);

    assert_eq!(report.entries[2].key, "GREETING");
    assert_eq!(report.entries[2].value, "Hello\n\"World\"\\End");
    assert_eq!(report.entries[2].quote_style, QuoteStyle::Double);
    assert_eq!(
        report.entries[2].inline_comment.as_deref(),
        Some("# after double quote")
    );

    assert_eq!(report.entries[3].key, "EMPTY_VAL");
    assert_eq!(report.entries[3].value, "");

    // `#` without preceding whitespace in unquoted value is part of the value
    assert_eq!(report.entries[4].key, "HASH_IN_VAL");
    assert_eq!(report.entries[4].value, "abc#123");
}

#[test]
fn lexer_handles_crlf_line_endings_and_escaped_dollars() {
    let src = "KEY_A=\"price is \\$100\\r\\t\"\r\nKEY_B='literal'\r\n";
    let report = parse_dotenv(src, false);
    assert!(report.diagnostics.is_empty());
    assert_eq!(report.entries.len(), 2);
    assert_eq!(report.entries[0].value, "price is $100\r\t");
    assert_eq!(report.entries[1].value, "literal");
}

#[test]
fn linter_reports_dotenv001_missing_equals() {
    let src = "VALID_KEY=1\nINVALID_LINE_WITHOUT_EQUALS\nANOTHER=2\n";
    let report = parse_dotenv(src, false);
    assert_eq!(report.entries.len(), 2);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "DOTENV001");
    assert_eq!(report.diagnostics[0].severity, Severity::Error);
    assert_eq!(report.diagnostics[0].line, 2);
}

#[test]
fn linter_reports_dotenv002_invalid_key_identifier() {
    let src = "123_INVALID=abc\nBAD-KEY-NAME=xyz\n=empty_key\nGOOD_KEY_1=ok\n";
    let report = parse_dotenv(src, false);
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].key, "GOOD_KEY_1");
    assert_eq!(report.diagnostics.len(), 3);
    assert!(report.diagnostics.iter().all(|d| d.code == "DOTENV002"));
    assert_eq!(report.diagnostics[0].line, 1);
    assert_eq!(report.diagnostics[1].line, 2);
    assert_eq!(report.diagnostics[2].line, 3);
}

#[test]
fn linter_reports_dotenv003_unclosed_quotes_and_trailing_chars() {
    let src = "\
UNCLOSED_SINGLE='hello
UNCLOSED_DOUBLE=\"world\\\"
TRAILING_SINGLE='ok' extra_tokens
TRAILING_DOUBLE=\"ok\" extra_tokens
VALID_COMMENT_AFTER=\"ok\" # allowed comment
";
    let report = parse_dotenv(src, false);
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].key, "VALID_COMMENT_AFTER");
    assert_eq!(report.diagnostics.len(), 4);
    for (i, line) in [1usize, 2, 3, 4].iter().enumerate() {
        assert_eq!(report.diagnostics[i].code, "DOTENV003");
        assert_eq!(report.diagnostics[i].severity, Severity::Error);
        assert_eq!(report.diagnostics[i].line, *line);
    }
}

#[test]
fn linter_reports_dotenv004_duplicate_keys() {
    let src = "API_URL=https://a.example.com\nPORT=3000\nAPI_URL=https://b.example.com\n";
    let report = parse_dotenv(src, false);
    assert_eq!(report.entries.len(), 3);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "DOTENV004");
    assert_eq!(report.diagnostics[0].severity, Severity::Warning);
    assert_eq!(report.diagnostics[0].line, 3);
    assert!(report.diagnostics[0].message.contains("line 1"));
}

#[test]
fn linter_reports_dotenv005_whitespace_around_equals_and_unquoted_spaces() {
    let src = "\
SPACED_EQ = value
UNQUOTED_WORDS=hello world
  LEADING_SPACE_KEY=1
";
    let report = parse_dotenv(src, false);
    assert_eq!(report.entries.len(), 3);
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec!["DOTENV005", "DOTENV005", "DOTENV005"]);
}

#[test]
fn linter_reports_dotenv006_secret_patterns_in_env_and_template() {
    let src = "\
AWS_KEY=AKIAIOSFODNN7EXAMPLE
GITHUB_TOKEN=ghp_1234567890abcdef1234567890abcdef1234
OPENAI_API_KEY=sk-proj-1234567890abcdef1234567890
SAFE_PLACEHOLDER=<YOUR_OPENAI_API_KEY>
";
    let env_report = parse_dotenv(src, false);
    let env_006: Vec<_> = env_report
        .diagnostics
        .iter()
        .filter(|d| d.code == "DOTENV006")
        .collect();
    assert_eq!(env_006.len(), 3);
    assert!(env_006.iter().all(|d| d.severity == Severity::Info));

    let tpl_report = parse_dotenv(src, true);
    let tpl_006: Vec<_> = tpl_report
        .diagnostics
        .iter()
        .filter(|d| d.code == "DOTENV006")
        .collect();
    assert_eq!(tpl_006.len(), 3);
    assert!(tpl_006.iter().all(|d| d.severity == Severity::Warning));
}

#[test]
fn classify_secret_pattern_recognizes_slack_google_pem_and_embedded_uri_credentials() {
    assert_eq!(
        classify_secret_pattern("SLACK_BOT_TOKEN", "xoxb-123456789012-1234567890123-abcdef"),
        Some("Slack Token (xox*)")
    );
    assert_eq!(
        classify_secret_pattern("GOOGLE_KEY", "AIzaSyA1234567890abcdef1234567890abcdef"),
        Some("Google API Key (AIza*)")
    );
    assert_eq!(
        classify_secret_pattern(
            "PRIVATE_KEY",
            "-----BEGIN RSA PRIVATE KEY-----MIIEowIBAAKCAQEA-----END RSA PRIVATE KEY-----"
        ),
        Some("PEM Private Key")
    );
    assert_eq!(
        classify_secret_pattern(
            "DATABASE_URL",
            "postgres://admin:ProdSecret99@db.example.com:5432/app"
        ),
        Some("Connection URI with embedded credentials")
    );
    // Local default password `postgres` is not flagged as a live secret pattern
    assert_eq!(
        classify_secret_pattern(
            "DATABASE_URL",
            "postgres://postgres:postgres@localhost:5432/app"
        ),
        None
    );
}

#[test]
fn drift_analyzer_detects_missing_extra_empty_and_template_leaks() {
    let env_src = "\
PORT=8080
DATABASE_URL=
UNDOCUMENTED_FLAG=true
";
    let example_src = "\
PORT=3000
DATABASE_URL=postgres://localhost:5432/app
REDIS_URL=redis://localhost:6379
STRIPE_SECRET_KEY=sk_live_FAKE0003
";
    let env_rep = parse_dotenv(env_src, false);
    let ex_rep = parse_dotenv(example_src, true);
    let drift = compare_env_files(&env_rep, &ex_rep);

    assert!(!drift.is_clean());
    assert_eq!(drift.synced_count, 1); // PORT
    assert_eq!(drift.missing_keys.len(), 2); // REDIS_URL, STRIPE_SECRET_KEY
    assert_eq!(drift.missing_keys[0].key, "REDIS_URL");
    assert_eq!(drift.missing_keys[1].key, "STRIPE_SECRET_KEY");
    assert_eq!(drift.extra_keys.len(), 1);
    assert_eq!(drift.extra_keys[0].key, "UNDOCUMENTED_FLAG");
    assert_eq!(drift.empty_keys.len(), 1);
    assert_eq!(drift.empty_keys[0].key, "DATABASE_URL");
    assert_eq!(drift.template_leaks.len(), 1);
    assert_eq!(drift.template_leaks[0].key, "STRIPE_SECRET_KEY");
}

#[test]
fn mask_value_never_exposes_plaintext_secrets() {
    assert_eq!(mask_value("", false), "(empty)");
    assert_eq!(mask_value("super_secret_password_123", false), "***");
    assert_eq!(mask_value("short", true), "***");
    assert_eq!(mask_value("abcdefghijklmnop", true), "ab***op");
    assert!(!mask_value("abcdefghijklmnop", true).contains("cdefghijklmn"));
}

#[test]
fn template_generator_preserves_safe_defaults_and_sanitizes_secrets() {
    let env_src = "\
# Server settings
PORT=3000
NODE_ENV=development
ENABLE_CACHE=true
API_BASE_URL=http://localhost:4000 # local dev

# Secrets
export STRIPE_SECRET_KEY=\"sk_live_FAKE0001\" # billing
DATABASE_URL=postgres://admin:s3cr3tP4ss@db.internal:5432/prod
EMPTY_OPTIONAL=
";
    let generated = generate_template(env_src);
    let expected = "\
# Server settings
PORT=3000
NODE_ENV=development
ENABLE_CACHE=true
API_BASE_URL=http://localhost:4000 # local dev

# Secrets
export STRIPE_SECRET_KEY=<YOUR_STRIPE_SECRET_KEY> # billing
DATABASE_URL=<YOUR_DATABASE_URL>
EMPTY_OPTIONAL=
";
    assert_eq!(generated, expected);
    assert!(!generated.contains("sk_live_"));
    assert!(!generated.contains("s3cr3tP4ss"));
}

#[test]
fn masked_preview_renders_lengths_and_classifications() {
    let env_src = "PORT=3000\nGITHUB_TOKEN=ghp_1234567890abcdef1234567890abcdef1234\n";
    let settings = DotenvSettings::default();
    let preview = render_masked_preview(".env", env_src, &settings, Some("/workspace/app"));
    assert!(preview.contains("## dotenv Masked Preview"));
    assert!(preview.contains("PORT=*** (len 4, unquoted, standard)"));
    assert!(
        preview.contains("GITHUB_TOKEN=*** (len 40, unquoted, secret-token: GitHub Access Token)")
    );
    assert!(!preview.contains("ghp_1234567890"));
}

#[test]
fn jsonc_and_vscode_settings_resolver() {
    let vscode_jsonc = r#"{
        // dotenv VSCode extension configuration
        "dotenv.enableAutocompletion": false,
        "dotenv.MaskValues": true,
        "dotenv.revealEdges": true,
        "dotenv.examplePath": "./config/.env.template", /* block comment */
    }"#;
    let resolved = parse_dotenv_settings(Some(vscode_jsonc), None);
    assert!(!resolved.enable_autocompletion);
    assert!(resolved.mask_values);
    assert!(resolved.reveal_edges);
    assert_eq!(
        resolved.example_path.as_deref(),
        Some("config/.env.template")
    );

    // Zed settings override VSCode settings
    let zed_json = r#"{
        "enableAutocompletion": true,
        "examplePath": ".env.sample"
    }"#;
    let overridden = parse_dotenv_settings(Some(vscode_jsonc), Some(zed_json));
    assert!(overridden.enable_autocompletion);
    assert_eq!(overridden.example_path.as_deref(), Some(".env.sample"));

    // Opting out of VSCode settings import ignores `.vscode/settings.json`
    let opt_out_zed = r#"{ "importVscodeSettings": false }"#;
    let opted_out = parse_dotenv_settings(Some(vscode_jsonc), Some(opt_out_zed));
    assert!(opted_out.enable_autocompletion);
    assert_eq!(opted_out.example_path, None);
}

#[test]
fn example_candidates_prioritizes_configured_then_same_dir_then_root() {
    let candidates = example_candidates("packages/api/.env.local", Some("custom/.env.example"));
    assert_eq!(
        candidates,
        vec![
            "custom/.env.example",
            "packages/api/.env.example",
            "packages/api/.env.sample",
            "packages/api/.env.template",
            ".env.example",
            ".env.sample",
            ".env.template",
        ]
    );
}

#[test]
fn resolves_custom_and_fallback_lsp_commands() {
    // 1. Custom binary path wins
    let custom = resolve_server_command(
        Some("/opt/bin/custom-dotenv-lsp".to_string()),
        Some(vec!["--stdio".to_string()]),
        Some(vec![("RUST_LOG".to_string(), "debug".to_string())]),
        &|_| None,
    )
    .unwrap();
    assert_eq!(custom.command, "/opt/bin/custom-dotenv-lsp");
    assert_eq!(custom.args, vec!["--stdio"]);
    assert_eq!(custom.env.len(), 1);

    // 2. Prefers `zed-dotenv-lsp`, falls back to `dotenv-lsp` then `dotenv-linter`
    let sidecar = resolve_server_command(None, None, None, &|bin| {
        (bin == "zed-dotenv-lsp").then(|| "/usr/local/bin/zed-dotenv-lsp".to_string())
    })
    .unwrap();
    assert_eq!(sidecar.command, "/usr/local/bin/zed-dotenv-lsp");

    let fallback_lsp = resolve_server_command(None, None, None, &|bin| {
        (bin == "dotenv-lsp").then(|| "/usr/local/bin/dotenv-lsp".to_string())
    })
    .unwrap();
    assert_eq!(fallback_lsp.command, "/usr/local/bin/dotenv-lsp");

    let fallback = resolve_server_command(None, None, None, &|bin| {
        (bin == "dotenv-linter").then(|| "/usr/local/bin/dotenv-linter".to_string())
    })
    .unwrap();
    assert_eq!(fallback.command, "/usr/local/bin/dotenv-linter");

    // 3. Returns actionable error when neither binary is found
    let err = resolve_server_command(None, None, None, &|_| None).unwrap_err();
    assert!(err.contains("zed-dotenv-lsp"));
    assert!(err.contains("dotenv-lsp"));
    assert!(err.contains("dotenv-linter"));
    assert!(err.contains("lsp.dotenv-lsp.binary.path"));
}

#[test]
fn audit_report_marks_fail_when_syntax_errors_present() {
    let bad_env = "BROKEN_LINE_WITHOUT_EQ\nPORT=3000\n";
    let settings = DotenvSettings::default();
    let out = render_audit_report(".env", bad_env, None, &settings, Some("/workspace"));
    assert!(out.contains("status: **FAIL (syntax errors detected)**"));
    assert!(out.contains("DOTENV001"));
}

#[test]
fn select_dotenv_lsp_asset_and_cached_lsp_binary_rel_path() {
    // FR-15: Mapping table matches official zed-dotenv-lsp release archives
    assert_eq!(
        select_dotenv_lsp_asset("linux", "x86_64"),
        Some("zed-dotenv-lsp-linux-x86_64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("linux", "amd64"),
        Some("zed-dotenv-lsp-linux-x86_64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("linux", "aarch64"),
        Some("zed-dotenv-lsp-linux-aarch64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("linux", "arm64"),
        Some("zed-dotenv-lsp-linux-aarch64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("macos", "aarch64"),
        Some("zed-dotenv-lsp-darwin-arm64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("darwin", "arm64"),
        Some("zed-dotenv-lsp-darwin-arm64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("mac", "arm64"),
        Some("zed-dotenv-lsp-darwin-arm64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("macos", "x86_64"),
        Some("zed-dotenv-lsp-darwin-x86_64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("darwin", "amd64"),
        Some("zed-dotenv-lsp-darwin-x86_64.tar.gz")
    );
    assert_eq!(
        select_dotenv_lsp_asset("windows", "x86_64"),
        Some("zed-dotenv-lsp-windows-x86_64.zip")
    );
    assert_eq!(
        select_dotenv_lsp_asset("win", "amd64"),
        Some("zed-dotenv-lsp-windows-x86_64.zip")
    );
    assert_eq!(select_dotenv_lsp_asset("freebsd", "x86_64"), None);
    assert_eq!(select_dotenv_lsp_asset("windows", "arm64"), None);
    assert_eq!(select_dotenv_lsp_asset("linux", "x86"), None);

    // Versioned path helper
    assert_eq!(
        cached_lsp_binary_rel_path("0.1.0", "windows"),
        "zed-dotenv-lsp-0.1.0/zed-dotenv-lsp.exe"
    );
    assert_eq!(
        cached_lsp_binary_rel_path("v0.1.0", "win"),
        "zed-dotenv-lsp-v0.1.0/zed-dotenv-lsp.exe"
    );
    assert_eq!(
        cached_lsp_binary_rel_path("0.1.0", "linux"),
        "zed-dotenv-lsp-0.1.0/zed-dotenv-lsp"
    );
    assert_eq!(
        cached_lsp_binary_rel_path("v0.1.0", "macos"),
        "zed-dotenv-lsp-v0.1.0/zed-dotenv-lsp"
    );
}

#[test]
fn find_cached_lsp_binary_resolves_offline_directory() {
    let dirs = [
        "dotenv-linter-3.3.0",
        "zed-dotenv-lsp-v0.1.0",
        "zed-dotenv-lsp-v0.2.0",
        "other-dir",
    ];
    // Ignores legacy dotenv-linter-* and picks existing zed-dotenv-lsp-* executable
    let found_linux = find_cached_lsp_binary(dirs, "linux", &|p| {
        p == "zed-dotenv-lsp-v0.1.0/zed-dotenv-lsp"
    });
    assert_eq!(
        found_linux.as_deref(),
        Some("zed-dotenv-lsp-v0.1.0/zed-dotenv-lsp")
    );

    let found_win = find_cached_lsp_binary(dirs, "windows", &|p| {
        p == "zed-dotenv-lsp-v0.2.0/zed-dotenv-lsp.exe"
    });
    assert_eq!(
        found_win.as_deref(),
        Some("zed-dotenv-lsp-v0.2.0/zed-dotenv-lsp.exe")
    );

    let none_found = find_cached_lsp_binary(dirs, "macos", &|_| false);
    assert_eq!(none_found, None);
}

#[test]
fn select_dotenv_linter_asset_and_cached_binary_rel_path() {
    // FR-15: Mapping table matches official release archives
    assert_eq!(
        select_dotenv_linter_asset("linux", "x86_64"),
        Some("dotenv-linter-linux-x86_64.tar.gz")
    );
    assert_eq!(
        select_dotenv_linter_asset("linux", "amd64"),
        Some("dotenv-linter-linux-x86_64.tar.gz")
    );
    assert_eq!(
        select_dotenv_linter_asset("linux", "aarch64"),
        Some("dotenv-linter-linux-aarch64.tar.gz")
    );
    assert_eq!(
        select_dotenv_linter_asset("macos", "aarch64"),
        Some("dotenv-linter-darwin-arm64.tar.gz")
    );
    assert_eq!(
        select_dotenv_linter_asset("darwin", "arm64"),
        Some("dotenv-linter-darwin-arm64.tar.gz")
    );
    assert_eq!(
        select_dotenv_linter_asset("macos", "x86_64"),
        Some("dotenv-linter-darwin-x86_64.tar.gz")
    );
    assert_eq!(
        select_dotenv_linter_asset("windows", "x86_64"),
        Some("dotenv-linter-windows-x86_64.zip")
    );
    assert_eq!(select_dotenv_linter_asset("freebsd", "x86_64"), None);
    assert_eq!(select_dotenv_linter_asset("windows", "arm64"), None);

    // Versioned path helper
    assert_eq!(
        cached_binary_rel_path("3.3.0", "windows"),
        "dotenv-linter-3.3.0/dotenv-linter.exe"
    );
    assert_eq!(
        cached_binary_rel_path("3.3.0", "linux"),
        "dotenv-linter-3.3.0/dotenv-linter"
    );
    assert_eq!(
        cached_binary_rel_path("v3.2.0", "macos"),
        "dotenv-linter-v3.2.0/dotenv-linter"
    );
}

#[test]
fn matrix_report_builds_markdown_table_and_reports_inconsistencies() {
    // FR-16: Multi-tier comparison across .env.example, .env, and .env.production
    let example = "PORT=3000\nDATABASE_URL=\nAPI_KEY=<KEY>\n";
    let env = "PORT=3000\nDATABASE_URL=postgres://localhost:5432/db\nAPI_KEY=my_secret_token_123456\nLOCAL_ONLY=true\n";
    let prod = "PORT=4000\nDATABASE_URL=postgres://admin:s3cr3tPassword@db.internal:5432/prod\nAPI_KEY=sk_live_FAKE0002\nPROD_ONLY=true\n";

    let files = &[
        (".env.example", example),
        (".env", env),
        (".env.production", prod),
    ];

    let report = render_matrix_report(files, true);

    assert!(report.contains("## dotenv Multi-Environment Matrix"));
    assert!(report.contains("### Summary"));
    assert!(report.contains("- environments: `3` file(s)"));
    assert!(report.contains("### Environment Matrix"));
    assert!(report.contains("| Key | .env.example | .env | .env.production |"));

    // PORT is set across all 3
    assert!(report.contains("| PORT | ✅ Set (***) | ✅ Set (***) | ✅ Set (***) |"));

    // DATABASE_URL is Empty in example, Set in env, Secret Pattern in prod
    assert!(report.contains("| DATABASE_URL | ⚪ Empty | ✅ Set (***) | ⚠️ Secret Pattern |"));

    // LOCAL_ONLY is missing in example and prod
    assert!(report.contains("| LOCAL_ONLY | ❌ Missing | ✅ Set (***) | ❌ Missing |"));

    // Inconsistencies section
    assert!(report.contains("### Inconsistent Keys"));
    assert!(report.contains("[DRIFT] DATABASE_URL:"));
    assert!(report.contains("[DRIFT] LOCAL_ONLY:"));
    assert!(report.contains("[DRIFT] PROD_ONLY:"));

    // Raw secrets must never be exposed
    assert!(!report.contains("my_secret_token_123456"));
    assert!(!report.contains("s3cr3tPassword"));
    assert!(!report.contains("sk_live_FAKE0002"));
}

#[test]
fn extract_source_env_refs_across_languages_and_ignores_comments() {
    // FR-17: Scan source code in JS/TS, Rust, Python, Go with comment exclusion
    let src = r#"
// Single line JS comment: process.env.IGNORED_JS = 1
# Single line Python comment: os.environ["IGNORED_PY"]
/* Multi-line comment: std::env::var("IGNORED_RUST") */
* Inner comment: os.Getenv("IGNORED_GO")

const port = process.env.PORT || 3000;
const dbUrl = process.env["DATABASE_URL"];
const viteTitle = import.meta.env.VITE_APP_TITLE;
const viteApi = import.meta.env['VITE_API_ENDPOINT'];

fn init() {
let host = std::env::var("HOST").unwrap();
let env_name = env::var("RUST_LOG").unwrap_or_default();
let version = env!("CARGO_PKG_VERSION");
let opt_debug = option_env!("DEBUG_MODE");
let (multi_a, multi_b) = (std::env::var("VAR_A"), std::env::var("VAR_B"));
}

def load_py():
secret = os.environ["SECRET_KEY"]
timeout = os.environ.get("TIMEOUT_MS", 5000)
cache = os.getenv("REDIS_HOST")

func loadGo() {
logLvl := os.Getenv("LOG_LEVEL")
svc, ok := os.LookupEnv("SERVICE_NAME")
}
"#;

    let refs = extract_source_env_refs(src);

    // Ensure commented variables were NOT extracted
    assert!(!refs.iter().any(|r| r.key.starts_with("IGNORED")));

    // Verify JS/TS extractions
    assert!(refs
        .iter()
        .any(|r| r.key == "PORT" && r.pattern == "process.env"));
    assert!(refs
        .iter()
        .any(|r| r.key == "DATABASE_URL" && r.pattern == "process.env[]"));
    assert!(refs
        .iter()
        .any(|r| r.key == "VITE_APP_TITLE" && r.pattern == "import.meta.env"));
    assert!(refs
        .iter()
        .any(|r| r.key == "VITE_API_ENDPOINT" && r.pattern == "import.meta.env[]"));

    // Verify Rust extractions
    assert!(refs
        .iter()
        .any(|r| r.key == "HOST" && r.pattern == "std::env::var"));
    assert!(refs
        .iter()
        .any(|r| r.key == "RUST_LOG" && r.pattern == "env::var"));
    assert!(refs
        .iter()
        .any(|r| r.key == "CARGO_PKG_VERSION" && r.pattern == "env!"));
    assert!(refs
        .iter()
        .any(|r| r.key == "DEBUG_MODE" && r.pattern == "option_env!"));
    assert!(refs
        .iter()
        .any(|r| r.key == "VAR_A" && r.pattern == "std::env::var"));
    assert!(refs
        .iter()
        .any(|r| r.key == "VAR_B" && r.pattern == "std::env::var"));

    // Verify Python extractions
    assert!(refs
        .iter()
        .any(|r| r.key == "SECRET_KEY" && r.pattern == "os.environ[]"));
    assert!(refs
        .iter()
        .any(|r| r.key == "TIMEOUT_MS" && r.pattern == "os.environ.get"));
    assert!(refs
        .iter()
        .any(|r| r.key == "REDIS_HOST" && r.pattern == "os.getenv"));

    // Verify Go extractions
    assert!(refs
        .iter()
        .any(|r| r.key == "LOG_LEVEL" && r.pattern == "os.Getenv"));
    assert!(refs
        .iter()
        .any(|r| r.key == "SERVICE_NAME" && r.pattern == "os.LookupEnv"));
}

#[test]
fn render_scan_report_detects_declared_and_undeclared_keys() {
    // FR-17: Cross-checking source references against .env and .env.example
    let src = r#"
const port = process.env.PORT;
const db = process.env.DATABASE_URL;
const stripe = process.env.STRIPE_KEY;
"#;
    let env = "PORT=3000\nDATABASE_URL=postgres://localhost:5432/app\n";
    let example = "PORT=3000\nDATABASE_URL=\nSTRIPE_KEY=<KEY>\n";

    let report = render_scan_report(
        "src/app.ts",
        src,
        ".env",
        env,
        Some((".env.example", example)),
        Some("/workspace"),
    );

    assert!(report.contains("## dotenv Source Reference Scanner"));
    assert!(
        report.contains("status: **FAIL (undeclared environment variable references detected)**")
    );
    assert!(report.contains("STRIPE_KEY"));
    assert!(report.contains("missing in target env, defined in template"));
    assert!(report.contains("PORT"));
    assert!(report.contains("✅ Declared in `.env`"));
}

#[test]
fn extract_http_dotenv_refs_and_readiness_report() {
    // FR-18: Extract {{$dotenv KEY}} and {{$dotenv %KEY}} tokens from .http files
    let http_content = r#"
# Commented request:
# GET https://example.com/api?key={{$dotenv IGNORED_KEY}}
// Another comment: Authorization: Bearer {{$dotenv IGNORED_TOO}}

GET https://api.service.com/v1/data
Authorization: Bearer {{$dotenv API_TOKEN}}
X-Client: {{$dotenv %ENCODED_CLIENT}}
X-Empty: {{$dotenv EMPTY_CONFIG}}
X-Missing: {{$dotenv MISSING_SETTING}}
"#;

    let refs = extract_http_dotenv_refs(http_content);
    assert_eq!(refs.len(), 4);
    assert!(!refs.iter().any(|r| r.key.starts_with("IGNORED")));
    assert!(refs.iter().any(|r| r.key == "API_TOKEN" && r.line == 7));
    assert!(refs
        .iter()
        .any(|r| r.key == "ENCODED_CLIENT" && r.line == 8));
    assert!(refs.iter().any(|r| r.key == "EMPTY_CONFIG" && r.line == 9));
    assert!(refs
        .iter()
        .any(|r| r.key == "MISSING_SETTING" && r.line == 10));

    let env_content =
        "API_TOKEN=super_secret_token_12345\nENCODED_CLIENT=client_abc\nEMPTY_CONFIG=\n";
    let example_content = "API_TOKEN=<YOUR_TOKEN>\nENCODED_CLIENT=<CLIENT>\nEMPTY_CONFIG=\nMISSING_SETTING=<SETTING>\n";

    let report = render_http_readiness_report(
        "requests/api.http",
        http_content,
        ".env",
        env_content,
        Some((".env.example", example_content)),
        Some("/workspace"),
    );

    assert!(report.contains("## dotenv HTTP Readiness Check"));
    assert!(report.contains("status: **FAIL (missing variables required by .http file)**"));
    assert!(report.contains("| L07 | API_TOKEN | ✅ Ready (masked ***) |"));
    assert!(report.contains("| L08 | ENCODED_CLIENT | ✅ Ready (masked ***) |"));
    assert!(report.contains("| L09 | EMPTY_CONFIG | ⚪ Empty in .env |"));
    assert!(report.contains("| L10 | MISSING_SETTING | ❌ Missing in .env (present in template) |"));
    assert!(!report.contains("super_secret_token_12345"));
}

#[test]
fn encrypted_value_classification_and_dotenv_vault_support() {
    // FR-19: .env.vault and dotenvx encrypted values
    assert!(is_encrypted_value("ANY_KEY", "encrypted:BQ7Z...XYZ"));
    assert!(is_encrypted_value("ANY_KEY", "vlt_1234567890abcdef"));
    assert!(is_encrypted_value(
        "DOTENV_VAULT_DEVELOPMENT",
        "vlt_production_key"
    ));
    assert!(is_encrypted_value("DOTENV_VAULT", "vault_identifier"));
    assert!(!is_encrypted_value("PORT", "3000"));
    assert!(!is_encrypted_value("AWS_KEY", "AKIAIOSFODNN7EXAMPLE"));

    // Encrypted values must not be flagged as live secrets (DOTENV006)
    assert_eq!(
        classify_secret_pattern("SECRET", "encrypted:ghp_1234567890abcdef"),
        None
    );
    assert_eq!(
        classify_secret_pattern("DOTENV_VAULT_DEV", "vlt_sk-1234567890123456"),
        None
    );

    let vault_content = "DOTENV_VAULT_DEVELOPMENT=\"vlt_0123456789abcdef\"\nCIPHERTEXT=\"encrypted:ABCDEFGHIJKLMNOPQRSTUVWXYZ\"\n";
    let parse_rep = parse_dotenv(vault_content, true);
    assert_eq!(parse_rep.error_count(), 0);
    assert_eq!(parse_rep.warning_count(), 0);
    assert!(!parse_rep.diagnostics.iter().any(|d| d.code == "DOTENV006"));

    let settings = DotenvSettings::default();
    let masked = render_masked_preview(".env.vault", vault_content, &settings, None);
    assert!(masked.contains("[ENCRYPTED: dotenvx/vault]"));

    // Template generator preserves encrypted values
    let tpl = generate_template(vault_content);
    assert!(tpl.contains("DOTENV_VAULT_DEVELOPMENT=\"vlt_0123456789abcdef\""));
    assert!(tpl.contains("CIPHERTEXT=\"encrypted:ABCDEFGHIJKLMNOPQRSTUVWXYZ\""));
}

#[test]
fn completion_candidates_filter_references_without_values() {
    let text = "API_KEY=secret\nAPI_URL=https://example.com\nOTHER=ok\nREF=??${API";
    assert_eq!(
        completion_candidates(text, 3, 11),
        vec!["API_KEY", "API_URL"]
    );
    assert!(completion_candidates(text, 3, 5).is_empty()); // inside UTF-16 surrogate pair
    assert!(completion_candidates("KEY=one\nREF='$KE", 1, 8).is_empty());
    assert!(completion_candidates("KEY=one\n# $KE", 1, 5).is_empty());
}

#[test]
fn hover_settings_keep_secrets_masked() {
    let mut settings = DotenvSettings::default();
    assert_eq!(hover_value("NODE_ENV", "production", &settings), "***");
    settings.reveal_edges = true;
    assert_eq!(hover_value("NODE_ENV", "production", &settings), "pr***on");
    assert_eq!(
        hover_value("API_TOKEN", "ghp_1234567890abcdefghijkl", &settings),
        "***"
    );
    assert_eq!(hover_value("PASSWORD", "sensitivevalue", &settings), "***");
    settings.mask_values = false;
    assert_eq!(
        hover_value("NODE_ENV", "production", &settings),
        "production"
    );
    assert_eq!(
        hover_value("API_TOKEN", "ghp_1234567890abcdefghijkl", &settings),
        "***"
    );
    assert_eq!(
        hover_value("CUSTOM", "unknown-secret-string", &settings),
        "***"
    );
}
