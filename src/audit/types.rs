//! Constants and shared data models (settings, diagnostics, parse and drift reports).

/// Language server identifier matching `extension.toml`.
pub const LANGUAGE_SERVER_ID: &str = "dotenv-lsp";

/// Workspace-relative path to VSCode settings for `dotenv.*` import.
pub const VSCODE_SETTINGS_PATH: &str = ".vscode/settings.json";

/// Default `.env` path when none is specified.
pub const DEFAULT_ENV_PATH: &str = ".env";

/// Candidate binary names searched on `PATH` when no custom binary is configured.
pub const FALLBACK_SERVER_BINARIES: &[&str] = &["zed-dotenv-lsp", "dotenv-lsp", "dotenv-linter"];

/// Default template filenames searched alongside the target `.env` file and at worktree root.
pub const EXAMPLE_CANDIDATE_NAMES: &[&str] = &[".env.example", ".env.sample", ".env.template"];

/// Default environment file candidate paths for matrix comparisons when no files are specified (FR-16).
pub const DEFAULT_MATRIX_CANDIDATES: &[&str] = &[
    ".env.example",
    ".env",
    ".env.local",
    ".env.development",
    ".env.staging",
    ".env.production",
    ".env.test",
];

/// Actions supported by the standalone task CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subcommand {
    /// Audit `[env_path]` for syntax diagnostics and compare keys against `[example_path]`.
    Check {
        env_path: String,
        example_path: Option<String>,
    },
    /// Generate a sanitized `.env.example` skeleton from `[env_path]`.
    Template { env_path: String },
    /// Render masked `KEY=*** (len N)` preview for `[env_path]`.
    Mask { env_path: String },
    /// Compare keys across multiple `.env*` environment files simultaneously (FR-16).
    Matrix { files: Vec<String> },
    /// Scan a source file for environment variable references and cross-check against `.env` (FR-17).
    Scan { src_path: String, env_path: String },
    /// Audit `{{$dotenv KEY}}` tokens in a `.http` file against `.env` (FR-18).
    Http { http_path: String, env_path: String },
}

/// Alias for [`Subcommand`] matching the prompt domain naming convention.
#[allow(dead_code)]
pub type DotenvSubcommand = Subcommand;

/// A reference to an environment variable discovered in source code or a `.http` file (FR-17, FR-18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEnvRef {
    /// 1-based line number where the reference appeared.
    pub line: usize,
    /// Environment variable identifier (`[A-Za-z_][A-Za-z0-9_]*`).
    pub key: String,
    /// Pattern or syntax kind used to reference the variable.
    pub pattern: &'static str,
}

/// Quote style of a parsed `.env` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    Unquoted,
    Single,
    Double,
}

impl QuoteStyle {
    /// Human-readable label for masked previews.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unquoted => "unquoted",
            Self::Single => "single-quoted",
            Self::Double => "double-quoted",
        }
    }
}

/// Diagnostic severity level for `.env` syntax and security rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// Uppercase badge used in Markdown diagnostic reports.
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warning => "WARN",
            Self::Info => "INFO",
        }
    }
}

/// A single structured diagnostic emitted during `.env` lexing and validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticIssue {
    /// 1-based line number in the `.env` file.
    pub line: usize,
    /// Diagnostic severity (`Error`, `Warning`, `Info`).
    pub severity: Severity,
    /// Stable rule code (`DOTENV001`..`DOTENV006`).
    pub code: &'static str,
    /// Actionable human-readable diagnostic message (never contains raw secret values).
    pub message: String,
    /// Associated variable key if known.
    pub key: Option<String>,
}

/// A parsed `KEY=VALUE` assignment from a `.env` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvEntry {
    /// 1-based line number where the assignment appeared.
    pub line: usize,
    /// Environment variable identifier (`[A-Za-z_][A-Za-z0-9_]*`).
    pub key: String,
    /// Unescaped effective value.
    pub value: String,
    /// Quote style used on the right-hand side.
    pub quote_style: QuoteStyle,
    /// Whether the line used an `export ` prefix.
    pub has_export: bool,
    /// Optional trailing `# ...` comment preserved for template generation.
    pub inline_comment: Option<String>,
}

/// Representation of each line in a `.env` file so `template` generation can
/// reconstruct comments and blank lines faithfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawLine {
    Blank,
    Comment(String),
    Entry(EnvEntry),
    Malformed { line: usize, raw: String },
}

/// Full parse & lint output for a single `.env` or `.env.example` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseReport {
    /// Validated `KEY=VALUE` assignments in source order.
    pub entries: Vec<EnvEntry>,
    /// Line-by-line structural AST used by template generation.
    pub lines: Vec<RawLine>,
    /// Syntax, duplicate-key, whitespace, and secret-pattern diagnostics.
    pub diagnostics: Vec<DiagnosticIssue>,
}

impl ParseReport {
    /// Count of `Severity::Error` diagnostics.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Count of `Severity::Warning` diagnostics.
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// Count of `Severity::Info` diagnostics.
    pub fn info_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Info)
            .count()
    }
}

/// A single key drift item between `.env` and `.env.example`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftItem {
    pub key: String,
    pub env_line: Option<usize>,
    pub example_line: Option<usize>,
}

/// A potential secret leak found inside `.env.example` / template file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateLeak {
    pub key: String,
    pub example_line: usize,
    pub reason: String,
}

/// Schema drift report comparing a target `.env` against a template (`.env.example`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftReport {
    /// Keys defined in `.env.example` but missing from `.env`.
    pub missing_keys: Vec<DriftItem>,
    /// Keys defined in `.env` but absent from `.env.example`.
    pub extra_keys: Vec<DriftItem>,
    /// Keys present in both files whose effective value in `.env` is empty while `.env.example` has a non-empty value.
    pub empty_keys: Vec<DriftItem>,
    /// Keys in `.env.example` that appear to contain real secret values rather than placeholders.
    pub template_leaks: Vec<TemplateLeak>,
    /// Count of keys present in both files without empty-value drift.
    pub synced_count: usize,
}

impl DriftReport {
    /// Returns `true` when there are no missing, extra, empty, or leaked template keys.
    pub fn is_clean(&self) -> bool {
        self.missing_keys.is_empty()
            && self.extra_keys.is_empty()
            && self.empty_keys.is_empty()
            && self.template_leaks.is_empty()
    }
}

/// Resolved configuration for the `dotenv-lsp` language server and task reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotenvSettings {
    /// Whether environment variable autocompletion is enabled (`dotenv.enableAutocompletion`).
    pub enable_autocompletion: bool,
    /// Whether values are masked in reports and hover previews (`dotenv.maskValues`).
    pub mask_values: bool,
    /// Whether masked previews reveal first/last 2 characters on long values (`dotenv.revealEdges`).
    pub reveal_edges: bool,
    /// Custom `.env.example` path relative to worktree root (`dotenv.examplePath`).
    pub example_path: Option<String>,
    /// Whether `.vscode/settings.json` keys should be imported (`importVscodeSettings`).
    pub import_vscode_settings: bool,
}

impl Default for DotenvSettings {
    fn default() -> Self {
        Self {
            enable_autocompletion: true,
            mask_values: true,
            reveal_edges: false,
            example_path: None,
            import_vscode_settings: true,
        }
    }
}

/// Resolved command specification for launching `dotenv-lsp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCommandSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}
