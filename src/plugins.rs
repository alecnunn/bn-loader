use crate::config::Profile;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

// Bit 1 (value 2) indicates "installed" in pluginStatus
const INSTALLED_BIT: u32 = 2;

// Directory and file names
const PLUGINS_DIR: &str = "plugins";
const REPOSITORIES_DIR: &str = "repositories";
const PLUGIN_STATUS_FILE: &str = "plugin_status.json";
const PLUGIN_METADATA_FILE: &str = "plugin.json";

#[derive(Deserialize, Default)]
struct PluginJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
}

#[derive(Deserialize)]
struct PluginStatusFile(Vec<Repository>);

#[derive(Deserialize)]
struct Repository {
    #[serde(default)]
    plugins: Vec<RepoPlugin>,
}

#[derive(Deserialize)]
struct RepoPlugin {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default, rename = "pluginStatus")]
    plugin_status: u32,
}

#[derive(Clone)]
pub(crate) enum PluginSource {
    Manual,
    Official,
    Community,
}

pub(crate) struct PluginInfo {
    pub dir_name: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub source: PluginSource,
}

pub(crate) fn list_plugins(profile: &Profile) -> Result<Vec<PluginInfo>> {
    let mut plugins = Vec::new();

    // 1. Manual plugins from plugins/ directory
    let plugins_dir = profile.config_dir.join(PLUGINS_DIR);
    if plugins_dir.exists() {
        plugins.extend(read_manual_plugins(&plugins_dir)?);
    }

    // 2. Repository plugins from plugin_status.json
    let status_file = profile
        .config_dir
        .join(REPOSITORIES_DIR)
        .join(PLUGIN_STATUS_FILE);
    if status_file.exists() {
        plugins.extend(read_repo_plugins(&status_file)?);
    }

    plugins.sort_by(|a, b| {
        let name_a = a.name.as_deref().unwrap_or(&a.dir_name).to_lowercase();
        let name_b = b.name.as_deref().unwrap_or(&b.dir_name).to_lowercase();
        name_a.cmp(&name_b)
    });

    Ok(plugins)
}

fn read_manual_plugins(plugins_dir: &Path) -> Result<Vec<PluginInfo>> {
    let mut plugins = Vec::new();

    let entries = fs::read_dir(plugins_dir).context("Failed to read plugins directory")?;

    for entry in entries {
        let entry = entry.context("Failed to read entry")?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();
        let plugin_info = read_plugin_metadata(&path, &dir_name);
        plugins.push(plugin_info);
    }

    Ok(plugins)
}

fn read_plugin_metadata(plugin_dir: &Path, dir_name: &str) -> PluginInfo {
    let plugin_json_path = plugin_dir.join(PLUGIN_METADATA_FILE);

    if plugin_json_path.exists()
        && let Ok(content) = fs::read_to_string(&plugin_json_path)
        && let Ok(meta) = serde_json::from_str::<PluginJson>(&content)
    {
        return PluginInfo {
            dir_name: dir_name.to_string(),
            name: meta.name,
            version: meta.version,
            author: meta.author,
            source: PluginSource::Manual,
        };
    }

    PluginInfo {
        dir_name: dir_name.to_string(),
        name: None,
        version: None,
        author: None,
        source: PluginSource::Manual,
    }
}

fn read_repo_plugins(status_file: &Path) -> Result<Vec<PluginInfo>> {
    let content = fs::read_to_string(status_file).context("Failed to read plugin_status.json")?;

    let repos: PluginStatusFile =
        serde_json::from_str(&content).context("Failed to parse plugin_status.json")?;

    let mut plugins = Vec::new();

    for (idx, repo) in repos.0.iter().enumerate() {
        // First repo is typically "official", second is "community"
        let source = if idx == 0 {
            PluginSource::Official
        } else {
            PluginSource::Community
        };

        for plugin in &repo.plugins {
            // Check if installed (bit 1 set)
            if plugin.plugin_status & INSTALLED_BIT != 0 {
                plugins.push(PluginInfo {
                    dir_name: plugin.path.clone().unwrap_or_default(),
                    name: plugin.name.clone(),
                    version: plugin.version.clone(),
                    author: plugin.author.clone(),
                    source: source.clone(),
                });
            }
        }
    }

    Ok(plugins)
}

pub(crate) fn print_plugins(
    out: &crate::output::Output,
    profile_name: &str,
    plugins: &[PluginInfo],
) {
    if plugins.is_empty() {
        out.status(&format!(
            "No plugins installed for profile '{profile_name}'"
        ));
        return;
    }

    let manual: Vec<_> = plugins
        .iter()
        .filter(|p| matches!(p.source, PluginSource::Manual))
        .collect();
    let official: Vec<_> = plugins
        .iter()
        .filter(|p| matches!(p.source, PluginSource::Official))
        .collect();
    let community: Vec<_> = plugins
        .iter()
        .filter(|p| matches!(p.source, PluginSource::Community))
        .collect();

    out.heading(&format!(
        "Plugins for profile '{}' ({} total):",
        profile_name,
        plugins.len()
    ));

    if !official.is_empty() {
        out.heading(&format!("\n  [Official Repository] ({}):", official.len()));
        for plugin in &official {
            print_plugin_line(out, plugin);
        }
    }

    if !community.is_empty() {
        out.heading(&format!(
            "\n  [Community Repository] ({}):",
            community.len()
        ));
        for plugin in &community {
            print_plugin_line(out, plugin);
        }
    }

    if !manual.is_empty() {
        out.heading(&format!("\n  [Manual] ({}):", manual.len()));
        for plugin in &manual {
            print_plugin_line(out, plugin);
        }
    }
}

fn print_plugin_line(out: &crate::output::Output, plugin: &PluginInfo) {
    let display_name = plugin.name.as_deref().unwrap_or(&plugin.dir_name);
    let version = plugin.version.as_deref().unwrap_or("?");
    let author = plugin
        .author
        .as_deref()
        .map(|a| format!(" by {a}"))
        .unwrap_or_default();

    out.out(&format!("    {display_name} v{version}{author}"));
}
