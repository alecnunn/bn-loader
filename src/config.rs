use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub(crate) const DEFAULT_EXECUTABLE: &str = "binaryninja.exe";

#[cfg(not(windows))]
pub(crate) const DEFAULT_EXECUTABLE: &str = "binaryninja";

pub(crate) const CONFIG_FILE_NAME: &str = "bn-loader.toml";
pub(crate) const ENV_VAR_NAME: &str = "BN_USER_DIRECTORY";

fn default_executable() -> String {
    DEFAULT_EXECUTABLE.to_string()
}

/// Get the user's home directory (cross-platform)
fn home_dir() -> Option<PathBuf> {
    // Try HOME first (works on all platforms, required for WSL/Cygwin)
    if let Ok(home) = env::var("HOME") {
        return Some(PathBuf::from(home));
    }

    // Windows fallback: USERPROFILE
    if let Ok(userprofile) = env::var("USERPROFILE") {
        return Some(PathBuf::from(userprofile));
    }

    None
}

/// Get the configuration path
fn user_config_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".config").join(CONFIG_FILE_NAME))
}

/// Get the default base directory for auto-generated profile config dirs.
///
/// Returns `<home>/.config/bn-loader/profiles`. Used by `bn-loader install`
/// to derive a default `--config-dir` when one isn't supplied.
pub(crate) fn default_profiles_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".config").join("bn-loader").join("profiles"))
}

pub(crate) fn default_exclusions() -> Vec<String> {
    vec![
        "license.dat".to_string(),
        "license.txt".to_string(),
        "user.id".to_string(),
        "keychain/".to_string(),
        "__pycache__/".to_string(),
        "*.pyc".to_string(),
    ]
}

fn default_backup_retention() -> usize {
    5
}

#[derive(Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub(crate) struct GlobalConfig {
    /// Default profile to launch when no argument given
    #[serde(default)]
    pub default_profile: Option<String>,

    /// Color output mode: auto, always, never
    #[serde(default)]
    pub color: ColorMode,

    /// How many sync backups to retain (0 = unlimited)
    #[serde(default = "default_backup_retention")]
    pub backup_retention: usize,

    /// Default debug mode for all profiles
    #[serde(default)]
    pub debug: bool,
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub(crate) struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub install: InstallConfig,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub(crate) struct SyncConfig {
    /// Additional exclusion patterns (merged with defaults)
    #[serde(default)]
    pub exclusions: Vec<String>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub(crate) struct InstallConfig {
    /// Optional path to the 7-Zip executable (used for NSIS installer extraction).
    /// Resolution order: --seven-zip CLI flag > this config field > $PATH lookup.
    #[serde(default)]
    pub seven_zip: Option<PathBuf>,
}

#[derive(Deserialize, Serialize, Clone)]
pub(crate) struct Profile {
    pub install_dir: PathBuf,
    pub config_dir: PathBuf,
    #[serde(default = "default_executable")]
    pub executable: String,
    #[serde(default)]
    pub debug: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            install_dir: PathBuf::new(),
            config_dir: PathBuf::new(),
            executable: default_executable(),
            debug: false,
        }
    }
}

/// Find config file in order of precidence
pub(crate) fn find_config_file(custom_path: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = custom_path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
        eprintln!("Error: Config file not found: {path}");
        return None;
    }

    if let Some(config_path) = user_config_path()
        && config_path.exists()
    {
        return Some(config_path);
    }

    None
}

pub(crate) fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path).context("Failed to read config file")?;
    toml::from_str(&content).context("Failed to parse config file")
}

/// Append a new `[profiles.<name>]` block to the given config file.
///
/// Validates the profile name to prevent TOML injection and uses the `toml` crate
/// to escape path values.
pub(crate) fn append_profile_to_config(
    config_path: &Path,
    name: &str,
    install_dir: &Path,
    config_dir: &Path,
) -> Result<()> {
    if !is_valid_profile_name(name) {
        bail!(
            "Invalid profile name '{name}': must contain only alphanumeric characters, hyphens, and underscores"
        );
    }

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(config_path)
        .context("Failed to open config file")?;

    let install_str = install_dir.to_string_lossy();
    let config_str = config_dir.to_string_lossy();
    let install_escaped = toml::Value::String(install_str.into_owned());
    let config_escaped = toml::Value::String(config_str.into_owned());

    let profile_toml = format!(
        "\n[profiles.{name}]\ninstall_dir = {install_escaped}\nconfig_dir = {config_escaped}\n"
    );

    use std::io::Write;
    file.write_all(profile_toml.as_bytes())
        .context("Failed to write to config file")?;

    println!("  Added profile to: {}", config_path.display());

    Ok(())
}

/// Remove the `[profiles.<name>]` block from a config file. Returns Ok(()) even if
/// the profile wasn't present (idempotent).
///
/// Implementation: re-read the file, parse with toml, mutate the in-memory Table,
/// serialize back. This loses comments and exact formatting (TOML round-trip is lossy
/// for whitespace/comments), but it's safe and predictable.
pub(crate) fn remove_profile_from_config(config_path: &Path, name: &str) -> Result<()> {
    if !is_valid_profile_name(name) {
        bail!(
            "Invalid profile name '{name}': must contain only alphanumeric characters, hyphens, and underscores"
        );
    }

    let content = fs::read_to_string(config_path).context("Failed to read config file")?;
    let mut doc: toml::Table = content
        .parse()
        .context("Failed to parse config file as TOML")?;

    if let Some(profiles_value) = doc.get_mut("profiles")
        && let Some(profiles_table) = profiles_value.as_table_mut()
    {
        profiles_table.remove(name);
    }

    let serialized = toml::to_string_pretty(&doc).context("Failed to re-serialize config")?;
    fs::write(config_path, serialized).context("Failed to write updated config file")?;
    Ok(())
}

/// Profile name must be non-empty and contain only alphanumerics, hyphens, and underscores.
pub(crate) fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}
