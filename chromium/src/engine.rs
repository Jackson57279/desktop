// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The browser engine is Chromium, the open-source build.
//!
//! Google Chrome is a different product even though it is built from the same
//! tree. Launch refuses it. When a Chromium executable is not already on
//! `PATH` or in `ZEN_CHROMIUM_BIN`, Linux x86-64 downloads the pinned official
//! snapshot.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Official Chromium continuous-build revision for Linux x86-64.
pub const CHROMIUM_SNAPSHOT_REVISION: &str = "1725037";

#[derive(Debug)]
pub struct ChromiumEngine {
    pub binary: PathBuf,
    pub version: String,
}

#[derive(Debug)]
pub enum EngineError {
    Message(String),
    Io(io::Error),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Message(_) => None,
        }
    }
}

impl From<io::Error> for EngineError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn is_chromium_product(version: &str) -> bool {
    let version = version.trim();
    version.starts_with("Chromium ") && !version.contains("Google Chrome")
}

pub fn ensure_chromium_engine(explicit: Option<&Path>) -> Result<ChromiumEngine, EngineError> {
    if let Some(path) = explicit {
        return verified_binary(path);
    }
    if let Some(path) = std::env::var_os("ZEN_CHROMIUM_BIN") {
        return verified_binary(Path::new(&path));
    }
    for name in ["chromium", "chromium-browser"] {
        if let Some(path) = search_path(name) {
            if let Ok(engine) = verified_binary(&path) {
                return Ok(engine);
            }
        }
    }
    install_snapshot()
}

pub(crate) fn configure_process(command: &mut Command, binary: &Path) {
    let Some(directory) = binary.parent() else {
        return;
    };
    let mut paths = vec![directory.to_path_buf()];
    if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        command.env("LD_LIBRARY_PATH", joined);
    }
}

fn verified_binary(path: &Path) -> Result<ChromiumEngine, EngineError> {
    let binary = resolve_binary(path)?;
    let version = product_version(&binary)?;
    if !is_chromium_product(&version) {
        return Err(EngineError::Message(format!(
            "the browser engine must be Chromium, but {binary} reports `{version}`",
            binary = binary.display()
        )));
    }
    Ok(ChromiumEngine { binary, version })
}

fn product_version(binary: &Path) -> Result<String, EngineError> {
    let mut command = Command::new(binary);
    configure_process(&mut command, binary);
    let output = command.arg("--version").output().map_err(|error| {
        EngineError::Message(format!("failed to run {}: {error}", binary.display()))
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let version = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string();
    if version.is_empty() {
        return Err(EngineError::Message(format!(
            "failed to read the product version from {}",
            binary.display()
        )));
    }
    Ok(version)
}

fn install_snapshot() -> Result<ChromiumEngine, EngineError> {
    if cfg!(not(all(target_os = "linux", target_arch = "x86_64"))) {
        return Err(EngineError::Message(
            "Chromium was not found. Set ZEN_CHROMIUM_BIN to a Chromium executable.".to_string(),
        ));
    }
    let binary = snapshot_dir().join("chrome-linux").join("chrome");
    if binary.is_file() {
        if let Ok(engine) = verified_binary(&binary) {
            return Ok(engine);
        }
    }
    let cache = cache_dir();
    fs::create_dir_all(&cache)?;
    let staging = cache.join(format!("{CHROMIUM_SNAPSHOT_REVISION}.partial"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let archive = staging.join("chrome-linux.zip");
    let url = format!(
        "https://commondatastorage.googleapis.com/chromium-browser-snapshots/Linux_x64/{CHROMIUM_SNAPSHOT_REVISION}/chrome-linux.zip"
    );
    eprintln!("zen-chromium: downloading Chromium {CHROMIUM_SNAPSHOT_REVISION}");
    download(&url, &archive)?;
    extract_zip(&archive, &staging)?;
    let _ = fs::remove_file(&archive);
    let destination = snapshot_dir();
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(&staging, &destination)?;
    verified_binary(&destination.join("chrome-linux").join("chrome"))
}

fn download(url: &str, destination: &Path) -> Result<(), EngineError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(600))
        .build();
    let response = agent.get(url).call().map_err(|error| {
        EngineError::Message(format!("failed to download Chromium from {url}: {error}"))
    })?;
    if response.status() != 200 {
        return Err(EngineError::Message(format!(
            "failed to download Chromium from {url}: HTTP {}",
            response.status()
        )));
    }
    let mut file = File::create(destination)?;
    let mut reader = response.into_reader();
    io::copy(&mut reader, &mut file)?;
    file.flush()?;
    Ok(())
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), EngineError> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| EngineError::Message(format!("Chromium archive is invalid: {error}")))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            EngineError::Message(format!("Chromium archive entry is unreadable: {error}"))
        })?;
        let Some(name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let out = destination.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mode = entry.unix_mode();
        let mut outfile = File::create(&out)?;
        io::copy(&mut entry, &mut outfile)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = mode.unwrap_or_else(|| {
                if out.file_name().and_then(|name| name.to_str()) == Some("chrome") {
                    0o755
                } else {
                    0o644
                }
            });
            fs::set_permissions(&out, fs::Permissions::from_mode(mode))?;
        }
        #[cfg(not(unix))]
        {
            let _ = mode;
        }
    }
    Ok(())
}

fn snapshot_dir() -> PathBuf {
    cache_dir().join(CHROMIUM_SNAPSHOT_REVISION)
}

fn cache_dir() -> PathBuf {
    if let Some(directory) = std::env::var_os("ZEN_CHROMIUM_CACHE") {
        return PathBuf::from(directory);
    }
    if let Some(directory) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(directory).join("zen-chromium");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".cache").join("zen-chromium");
    }
    std::env::temp_dir().join("zen-chromium-cache")
}

fn resolve_binary(path: &Path) -> Result<PathBuf, EngineError> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(search_path)
        .ok_or_else(|| {
            EngineError::Message(format!(
                "Chromium executable {} was not found",
                path.display()
            ))
        })
}

fn search_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ensure_chromium_engine, is_chromium_product};

    #[test]
    fn product_check_accepts_only_chromium() {
        assert!(is_chromium_product("Chromium 142.0.7444.0"));
        assert!(!is_chromium_product("Google Chrome 142.0.7444.0"));
        assert!(!is_chromium_product("Chrome 142.0.7444.0"));
        assert!(!is_chromium_product(""));
    }

    #[test]
    fn installed_google_chrome_is_rejected() {
        let chrome = Path::new("/usr/local/bin/google-chrome");
        if !chrome.is_file() {
            return;
        }
        let error = ensure_chromium_engine(Some(chrome)).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("Google Chrome"),
            "expected Google Chrome to be refused, got {message}"
        );
    }
}
