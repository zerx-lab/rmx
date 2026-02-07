use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;

const GITHUB_API_URL: &str = "https://api.github.com/repos/zerx-lab/rmx/releases/latest";
const ASSET_SUFFIX: &str = "x86_64-pc-windows-msvc.zip";

// ── GitHub API types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

// ── Installation method detection ────────────────────────────────────────

#[derive(Debug)]
enum InstallMethod {
    Scoop,
    Cargo,
    Npm,
    Manual,
}

impl InstallMethod {
    fn detect() -> Self {
        let path_str = env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        if path_str.contains("scoop") && path_str.contains("apps") {
            InstallMethod::Scoop
        } else if path_str.contains(".cargo") && path_str.contains("bin") {
            InstallMethod::Cargo
        } else if path_str.contains("node_modules")
            || path_str.contains("\\npm\\")
            || path_str.contains("\\npx\\")
        {
            InstallMethod::Npm
        } else {
            InstallMethod::Manual
        }
    }

    /// 返回包管理器升级提示；Manual 返回 None
    fn upgrade_hint(&self) -> Option<&'static str> {
        match self {
            InstallMethod::Scoop => Some("scoop update rmx"),
            InstallMethod::Cargo => {
                Some("cargo install --git https://github.com/zerx-lab/rmx --force")
            }
            InstallMethod::Npm => Some("npm update -g rmx"),
            InstallMethod::Manual => None,
        }
    }
}

// ── Public API ───────────────────────────────────────────────────────────

/// 清理上次升级残留的 .old 文件（在 main 启动时调用）
pub fn cleanup_old_binary() {
    if let Ok(exe) = env::current_exe() {
        let old = old_path(&exe);
        if old.exists() {
            let _ = fs::remove_file(&old);
        }
    }
}

/// 执行升级流程
pub fn run_upgrade(check_only: bool, force: bool) -> anyhow::Result<()> {
    cleanup_old_binary();

    if !force {
        let method = InstallMethod::detect();
        if let Some(hint) = method.upgrade_hint() {
            println!("rmx: detected {:?} installation", method);
            println!("  recommended: {}", hint);
            println!("  or use: rmx upgrade --force");
            return Ok(());
        }
    }

    let current_version = env!("APP_VERSION");
    print!(
        "rmx: checking for updates (current: v{})... ",
        current_version
    );
    io::stdout().flush().ok();

    let release = fetch_latest_release()?;
    let latest_version = release.tag_name.trim_start_matches('v');
    println!("v{}", latest_version);

    if !force {
        let current = semver::Version::parse(current_version).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse current version '{}': {}",
                current_version,
                e
            )
        })?;
        let latest = semver::Version::parse(latest_version).map_err(|e| {
            anyhow::anyhow!("failed to parse latest version '{}': {}", latest_version, e)
        })?;

        if current >= latest {
            println!("rmx: already up to date");
            return Ok(());
        }
    }

    if check_only {
        println!(
            "rmx: update available: v{} -> v{}",
            current_version, latest_version
        );
        return Ok(());
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(ASSET_SUFFIX))
        .ok_or_else(|| anyhow::anyhow!("no matching release asset for this platform"))?;

    println!(
        "rmx: downloading {} ({})...",
        asset.name,
        format_size(asset.size)
    );

    let temp_dir = env::temp_dir().join("rmx-upgrade");
    fs::create_dir_all(&temp_dir)?;
    let zip_path = temp_dir.join(&asset.name);
    download_file(&asset.browser_download_url, &zip_path)?;

    println!("rmx: extracting...");
    let new_exe = temp_dir.join("rmx.exe");
    extract_exe_from_zip(&zip_path, &new_exe)?;

    println!("rmx: installing...");
    let installed_path = replace_self(&new_exe)?;

    let _ = fs::remove_dir_all(&temp_dir);

    println!(
        "rmx: upgraded v{} -> v{}\n  -> {}",
        current_version,
        latest_version,
        installed_path.display()
    );
    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────

fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

fn fetch_latest_release() -> anyhow::Result<GitHubRelease> {
    let body: String = ureq::get(GITHUB_API_URL)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "rmx-self-updater")
        .call()
        .map_err(|e| anyhow::anyhow!("failed to query GitHub API: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("failed to read response body: {}", e))?;

    let release: GitHubRelease = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("failed to parse GitHub response: {}", e))?;

    Ok(release)
}

fn download_file(url: &str, dest: &Path) -> anyhow::Result<()> {
    let mut reader = ureq::get(url)
        .header("User-Agent", "rmx-self-updater")
        .call()
        .map_err(|e| anyhow::anyhow!("download failed: {}", e))?
        .into_body()
        .into_reader();

    let mut file = fs::File::create(dest)?;
    io::copy(&mut reader, &mut file)?;
    file.flush()?;
    Ok(())
}

fn extract_exe_from_zip(zip_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        // zip 内可能是 rmx.exe 或 release/rmx.exe
        if name == "rmx.exe" || name.ends_with("/rmx.exe") {
            let mut out = fs::File::create(dest)?;
            io::copy(&mut entry, &mut out)?;
            out.flush()?;
            return Ok(());
        }
    }

    Err(anyhow::anyhow!("rmx.exe not found in archive"))
}

/// Rename-and-Replace: Windows 允许重命名正在运行的 exe
fn replace_self(new_exe: &Path) -> anyhow::Result<PathBuf> {
    let current_exe = env::current_exe()?;
    let old_exe = old_path(&current_exe);

    if old_exe.exists() {
        fs::remove_file(&old_exe).map_err(|e| {
            anyhow::anyhow!("failed to remove old binary '{}': {}", old_exe.display(), e)
        })?;
    }

    fs::rename(&current_exe, &old_exe)
        .map_err(|e| anyhow::anyhow!("failed to rename current binary: {}", e))?;

    if let Err(e) = fs::copy(new_exe, &current_exe) {
        // rollback
        eprintln!("rmx: install failed, rolling back...");
        let _ = fs::rename(&old_exe, &current_exe);
        return Err(anyhow::anyhow!("failed to install new binary: {}", e));
    }

    Ok(current_exe)
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
