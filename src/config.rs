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

/// Get the cache directory for bn-loader
pub(crate) fn cache_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".cache").join("bn-loader"))
}

fn default_exclusions() -> Vec<String> {
    vec![
        "license.dat".to_string(),
        "license.txt".to_string(),
        "user.id".to_string(),
        "keychain/".to_string(),
        "__pycache__/".to_string(),
        "*.pyc".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

fn default_backup_retention() -> usize {
    5
}

#[derive(Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq)]
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

    /// Check for updates on launch
    #[serde(default = "default_true")]
    pub check_updates: bool,

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
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub(crate) struct SyncConfig {
    #[serde(default = "default_exclusions")]
    pub exclusions: Vec<String>,
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

pub(crate) fn load_config(path: &Path) -> Result<Config, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read config file: {e}"))?;
    toml::from_str(&content).map_err(|e| format!("Failed to parse config file: {e}"))
}
