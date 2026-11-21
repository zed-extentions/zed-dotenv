//! Release asset selection and cache path helpers for `zed-dotenv-lsp` (and legacy `dotenv-linter`) (FR-15).

#[derive(Clone, Copy)]
enum ReleaseTarget {
    LinuxX86_64,
    LinuxAarch64,
    DarwinArm64,
    DarwinX86_64,
    WindowsX86_64,
}

fn resolve_release_target(os: &str, arch: &str) -> Option<ReleaseTarget> {
    let os_l = os.to_ascii_lowercase();
    let arch_l = arch.to_ascii_lowercase();
    match (os_l.as_str(), arch_l.as_str()) {
        ("linux", "x86_64" | "amd64") => Some(ReleaseTarget::LinuxX86_64),
        ("linux", "aarch64" | "arm64") => Some(ReleaseTarget::LinuxAarch64),
        ("macos" | "darwin" | "mac", "aarch64" | "arm64") => Some(ReleaseTarget::DarwinArm64),
        ("macos" | "darwin" | "mac", "x86_64" | "amd64") => Some(ReleaseTarget::DarwinX86_64),
        ("windows" | "win", "x86_64" | "amd64") => Some(ReleaseTarget::WindowsX86_64),
        _ => None,
    }
}

fn is_windows_os(os: &str) -> bool {
    os.eq_ignore_ascii_case("windows") || os.eq_ignore_ascii_case("win")
}

fn format_cached_binary_rel_path(binary_name: &str, version: &str, os: &str) -> String {
    let exe = if is_windows_os(os) {
        format!("{binary_name}.exe")
    } else {
        binary_name.to_string()
    };
    format!("{binary_name}-{version}/{exe}")
}

/// Returns the `zed-dotenv-lsp` executable filename for the given OS.
pub fn lsp_binary_filename(os: &str) -> &'static str {
    if is_windows_os(os) {
        "zed-dotenv-lsp.exe"
    } else {
        "zed-dotenv-lsp"
    }
}

/// Selects the official `zed-dotenv-lsp` release archive asset name for the given OS and architecture (FR-15).
pub fn select_dotenv_lsp_asset(os: &str, arch: &str) -> Option<&'static str> {
    match resolve_release_target(os, arch)? {
        ReleaseTarget::LinuxX86_64 => Some("zed-dotenv-lsp-linux-x86_64.tar.gz"),
        ReleaseTarget::LinuxAarch64 => Some("zed-dotenv-lsp-linux-aarch64.tar.gz"),
        ReleaseTarget::DarwinArm64 => Some("zed-dotenv-lsp-darwin-arm64.tar.gz"),
        ReleaseTarget::DarwinX86_64 => Some("zed-dotenv-lsp-darwin-x86_64.tar.gz"),
        ReleaseTarget::WindowsX86_64 => Some("zed-dotenv-lsp-windows-x86_64.zip"),
    }
}

/// Returns the expected cached binary relative path for a downloaded `zed-dotenv-lsp` version (FR-15).
pub fn cached_lsp_binary_rel_path(version: &str, os: &str) -> String {
    format_cached_binary_rel_path("zed-dotenv-lsp", version, os)
}

/// Finds an existing cached `zed-dotenv-lsp` binary across `zed-dotenv-lsp-*` subdirectories (FR-15).
pub fn find_cached_lsp_binary<'a>(
    dir_names: impl IntoIterator<Item = &'a str>,
    os: &str,
    is_file: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let exe_name = lsp_binary_filename(os);
    let mut matches: Vec<&str> = dir_names
        .into_iter()
        .filter(|name| name.starts_with("zed-dotenv-lsp-"))
        .collect();
    matches.sort_unstable();
    matches.into_iter().rev().find_map(|dir| {
        let candidate = format!("{dir}/{exe_name}");
        is_file(&candidate).then_some(candidate)
    })
}

/// Selects the legacy `dotenv-linter` release archive asset name for the given OS and architecture.
#[allow(dead_code)]
pub fn select_dotenv_linter_asset(os: &str, arch: &str) -> Option<&'static str> {
    match resolve_release_target(os, arch)? {
        ReleaseTarget::LinuxX86_64 => Some("dotenv-linter-linux-x86_64.tar.gz"),
        ReleaseTarget::LinuxAarch64 => Some("dotenv-linter-linux-aarch64.tar.gz"),
        ReleaseTarget::DarwinArm64 => Some("dotenv-linter-darwin-arm64.tar.gz"),
        ReleaseTarget::DarwinX86_64 => Some("dotenv-linter-darwin-x86_64.tar.gz"),
        ReleaseTarget::WindowsX86_64 => Some("dotenv-linter-windows-x86_64.zip"),
    }
}

/// Returns the expected cached binary relative path for a downloaded `dotenv-linter` version.
#[allow(dead_code)]
pub fn cached_binary_rel_path(version: &str, os: &str) -> String {
    format_cached_binary_rel_path("dotenv-linter", version, os)
}
