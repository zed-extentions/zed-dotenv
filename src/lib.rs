//! zed-dotenv — Environment variable (`.env`) autocompletion, hover masking,
//! strict syntax linting, `.env.example` drift analysis, and template generation
//! for Zed via `dotenv-lsp`.
//!
//! Tier 1 WASM boundary (`src/lib.rs`). All pure domain parsing, linting, drift
//! comparison, secret masking, and JSONC configuration resolution live in
//! the Tier 2 modules below (`audit`, `completion`, `http`, `language_server`, `reports`,
//! `security`, `source_scan`) so unit tests run natively on the host target.

// Tier 2: pure `std` modules (never import `zed_extension_api` from them).
#[allow(dead_code, unused_imports)]
#[path = "language_server/assets.rs"]
mod assets;
#[allow(dead_code, unused_imports)]
#[path = "language_server/completion.rs"]
mod completion;
#[allow(dead_code, unused_imports)]
#[path = "audit/drift.rs"]
mod drift;
#[allow(dead_code, unused_imports)]
#[path = "http/mod.rs"]
mod http;
#[allow(dead_code, unused_imports)]
#[path = "audit/parser.rs"]
mod parser;
#[allow(dead_code, unused_imports)]
#[path = "audit/paths.rs"]
mod paths;
#[allow(dead_code, unused_imports)]
#[path = "reports/mod.rs"]
mod report;
#[allow(dead_code, unused_imports)]
#[path = "source_scan/mod.rs"]
mod scan;
#[allow(dead_code, unused_imports)]
#[path = "security/mod.rs"]
mod secrets;
#[allow(dead_code, unused_imports)]
#[path = "language_server/settings.rs"]
mod settings;
#[allow(dead_code, unused_imports)]
#[path = "audit/types.rs"]
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
#[path = "tests/mod.rs"]
mod audit_tests;
use zed_extension_api::{serde_json, settings::LspSettings, Command, LanguageServerId, Worktree};

struct DotenvExtension;

impl zed_extension_api::Extension for DotenvExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command, String> {
        validate_language_server_id(language_server_id.as_ref())?;

        let lsp_settings = LspSettings::for_worktree(LANGUAGE_SERVER_ID, worktree).ok();
        let binary = lsp_settings.as_ref().and_then(|s| s.binary.as_ref());

        let custom_path = binary.and_then(|b| b.path.clone());
        let custom_args = binary.and_then(|b| b.arguments.clone());
        let custom_env = binary.and_then(|b| {
            b.env.as_ref().map(|env_map| {
                env_map
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
        });

        let spec = match resolve_server_command(
            custom_path,
            custom_args.clone(),
            custom_env.clone(),
            &|candidate| worktree.which(candidate),
        ) {
            Ok(spec) => spec,
            Err(err) => match auto_download_dotenv_lsp(language_server_id) {
                Ok(bin_path) => ServerCommandSpec {
                    command: bin_path,
                    args: custom_args.unwrap_or_default(),
                    env: custom_env.unwrap_or_default(),
                },
                Err(dl_err) => return Err(format!("{err} (auto-download failed: {dl_err})")),
            },
        };

        Ok(Command {
            command: spec.command,
            args: spec.args,
            env: spec.env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>, String> {
        if language_server_id.as_ref() != LANGUAGE_SERVER_ID {
            return Ok(None);
        }
        let settings = resolve_worktree_settings(worktree);
        Ok(Some(build_lsp_settings_json(&settings)))
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>, String> {
        if language_server_id.as_ref() != LANGUAGE_SERVER_ID {
            return Ok(None);
        }
        let settings = resolve_worktree_settings(worktree);
        Ok(Some(build_lsp_settings_json(&settings)))
    }
}

zed_extension_api::register_extension!(DotenvExtension);

/// Validates that `language_server_id` matches [`LANGUAGE_SERVER_ID`].
pub(crate) fn validate_language_server_id(id: &str) -> Result<(), String> {
    if id == LANGUAGE_SERVER_ID {
        Ok(())
    } else {
        Err(format!(
            "unknown language server `{id}`; this extension provides `{}`",
            LANGUAGE_SERVER_ID
        ))
    }
}

/// Resolves [`DotenvSettings`] from the worktree's `.vscode/settings.json` and Zed `LspSettings`.
fn resolve_worktree_settings(worktree: &Worktree) -> DotenvSettings {
    let vscode_jsonc = worktree.read_text_file(VSCODE_SETTINGS_PATH).ok();
    let zed_settings_json = LspSettings::for_worktree(LANGUAGE_SERVER_ID, worktree)
        .ok()
        .and_then(|s| s.settings)
        .and_then(|val| serde_json::to_string(&val).ok());
    parse_dotenv_settings(vscode_jsonc.as_deref(), zed_settings_json.as_deref())
}

/// Serializes [`DotenvSettings`] into the JSON payload expected by `dotenv-lsp`.
pub(crate) fn build_lsp_settings_json(settings: &DotenvSettings) -> serde_json::Value {
    serde_json::json!({
        "dotenv": {
            "enableAutocompletion": settings.enable_autocompletion,
            "maskValues": settings.mask_values,
            "revealEdges": settings.reveal_edges,
            "examplePath": settings.example_path.as_deref().unwrap_or(EXAMPLE_CANDIDATE_NAMES[0]),
        }
    })
}

/// Scans the extension working directory for a cached `zed-dotenv-lsp-*` binary for offline fallback (FR-15).
fn find_cached_lsp_binary_in_cwd(os_str: &str) -> Option<String> {
    let entries = std::fs::read_dir(".").ok()?;
    let dir_names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(ToString::to_string)
            } else {
                None
            }
        })
        .collect();
    find_cached_lsp_binary(dir_names.iter().map(String::as_str), os_str, &|p| {
        std::path::Path::new(p).is_file()
    })
}

fn mark_lsp_status(
    language_server_id: &LanguageServerId,
    status: zed_extension_api::LanguageServerInstallationStatus,
) {
    zed_extension_api::set_language_server_installation_status(language_server_id, &status);
}

/// Attempts to auto-download and cache a platform-appropriate standalone `zed-dotenv-lsp` binary (FR-15).
fn auto_download_dotenv_lsp(language_server_id: &LanguageServerId) -> Result<String, String> {
    use zed_extension_api::LanguageServerInstallationStatus as Status;

    let (os, arch) = zed_extension_api::current_platform();
    let os_str = match os {
        zed_extension_api::Os::Linux => "linux",
        zed_extension_api::Os::Mac => "macos",
        zed_extension_api::Os::Windows => "windows",
    };
    let arch_str = match arch {
        zed_extension_api::Architecture::Aarch64 => "aarch64",
        zed_extension_api::Architecture::X8664 => "x86_64",
        zed_extension_api::Architecture::X86 => "x86",
    };

    let asset_name = select_dotenv_lsp_asset(os_str, arch_str).ok_or_else(|| {
        format!("no pre-built zed-dotenv-lsp asset available for platform {os_str}/{arch_str}")
    })?;

    mark_lsp_status(language_server_id, Status::CheckingForUpdate);

    let release_res = zed_extension_api::latest_github_release(
        "zed-extentions/zed-dotenv",
        zed_extension_api::GithubReleaseOptions {
            require_assets: true,
            pre_release: false,
        },
    );

    match release_res {
        Ok(release) => {
            let bin_rel = cached_lsp_binary_rel_path(&release.version, os_str);
            if std::path::Path::new(&bin_rel).is_file() {
                mark_lsp_status(language_server_id, Status::None);
                return Ok(bin_rel);
            }

            let Some(asset) = release.assets.iter().find(|a| a.name == asset_name) else {
                if let Some(cached) = find_cached_lsp_binary_in_cwd(os_str) {
                    mark_lsp_status(language_server_id, Status::None);
                    return Ok(cached);
                }
                let msg = format!(
                    "asset `{asset_name}` not found in release {}",
                    release.version
                );
                mark_lsp_status(language_server_id, Status::Failed(msg.clone()));
                return Err(msg);
            };

            let dest_dir = format!("zed-dotenv-lsp-{}", release.version);
            let file_type = if asset_name.ends_with(".zip") {
                zed_extension_api::DownloadedFileType::Zip
            } else {
                zed_extension_api::DownloadedFileType::GzipTar
            };

            mark_lsp_status(language_server_id, Status::Downloading);

            if let Err(dl_err) =
                zed_extension_api::download_file(&asset.download_url, &dest_dir, file_type)
            {
                if let Some(cached) = find_cached_lsp_binary_in_cwd(os_str) {
                    mark_lsp_status(language_server_id, Status::None);
                    return Ok(cached);
                }
                mark_lsp_status(language_server_id, Status::Failed(dl_err.clone()));
                return Err(dl_err);
            }

            let _ = zed_extension_api::make_file_executable(&bin_rel);

            if std::path::Path::new(&bin_rel).is_file() {
                mark_lsp_status(language_server_id, Status::None);
                Ok(bin_rel)
            } else {
                let msg = format!("extracted binary not found at `{bin_rel}`");
                mark_lsp_status(language_server_id, Status::Failed(msg.clone()));
                Err(msg)
            }
        }
        Err(release_err) => {
            // Check for any existing cached binary in the extension working directory
            if let Some(cached) = find_cached_lsp_binary_in_cwd(os_str) {
                mark_lsp_status(language_server_id, Status::None);
                return Ok(cached);
            }
            mark_lsp_status(language_server_id, Status::Failed(release_err.clone()));
            Err(release_err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_command_and_id_validation() {
        assert!(validate_language_server_id("dotenv-lsp").is_ok());
        let err = validate_language_server_id("other-lsp").unwrap_err();
        assert!(err.contains("unknown language server `other-lsp`"));
    }

    #[test]
    fn lsp_settings_json_serialization_from_vscode_and_zed() {
        let settings = parse_dotenv_settings(
            Some(r#"{"dotenv.enableAutocompletion": false, "dotenv.examplePath": ".env.sample"}"#),
            None,
        );
        let json = build_lsp_settings_json(&settings);
        assert_eq!(
            json["dotenv"]["enableAutocompletion"],
            serde_json::json!(false)
        );
        assert_eq!(json["dotenv"]["maskValues"], serde_json::json!(true));
        assert_eq!(
            json["dotenv"]["examplePath"],
            serde_json::json!(".env.sample")
        );
    }

    /// FR-20: the sidecar parses every opened document as `.env`, so the language
    /// server must only be attached to dotenv languages. Attaching it to
    /// source languages makes every line of a `.rs`/`.py`/... file a DOTENV001 error.
    ///
    /// `env` (lowercase) covers the language name registered by the registry's
    /// `env` extension, so co-installation keeps the server attached even when
    /// Zed resolves `.env` files to that language; `Env` is kept for
    /// case-insensitive name matching.
    #[test]
    fn language_server_is_attached_only_to_dotenv_languages() {
        let manifest = include_str!("../extension.toml");
        let languages = manifest
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("languages ="))
            .expect("extension.toml must declare language_servers.dotenv-lsp.languages");
        assert_eq!(languages, r#"languages = ["Env", "dotenv", "env"]"#);
    }
}
