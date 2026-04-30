use crate::config::{Config, Profile};
use crate::output::Output;
use crate::plugins::{PluginInfo, list_plugins};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use termcolor::Color;

const SETTINGS_FILE: &str = "settings.json";
const MAX_DIFF_DISPLAY: usize = 20;
const MAX_VALUE_DISPLAY_LEN: usize = 30;
const VALUE_PREVIEW_LEN: usize = 27;

pub(crate) fn diff_profiles(
    config: &Config,
    name1: &str,
    profile1: &Profile,
    name2: &str,
    profile2: &Profile,
) -> Result<()> {
    let out = Output::new(config.global.color);

    out.out_bold_inline("Comparing profiles: ");
    out.out(&format!("'{name1}' vs '{name2}'\n"));

    diff_plugins(&out, name1, profile1, name2, profile2)?;
    out.out("");
    diff_settings(&out, name1, profile1, name2, profile2)?;

    Ok(())
}

fn diff_plugins(
    out: &Output,
    name1: &str,
    profile1: &Profile,
    name2: &str,
    profile2: &Profile,
) -> Result<()> {
    let plugins1 = list_plugins(profile1)?;
    let plugins2 = list_plugins(profile2)?;

    let set1: HashSet<&str> = plugins1.iter().map(|p| p.dir_name.as_str()).collect();
    let set2: HashSet<&str> = plugins2.iter().map(|p| p.dir_name.as_str()).collect();

    let only_in_1: Vec<&PluginInfo> = plugins1
        .iter()
        .filter(|p| !set2.contains(p.dir_name.as_str()))
        .collect();
    let only_in_2: Vec<&PluginInfo> = plugins2
        .iter()
        .filter(|p| !set1.contains(p.dir_name.as_str()))
        .collect();
    let in_both: Vec<(&PluginInfo, &PluginInfo)> = plugins1
        .iter()
        .filter_map(|p1| {
            plugins2
                .iter()
                .find(|p2| p2.dir_name == p1.dir_name)
                .map(|p2| (p1, p2))
        })
        .collect();

    out.heading("=== Plugins ===");
    out.out(&format!(
        "  {} has {} plugins, {} has {} plugins",
        name1,
        plugins1.len(),
        name2,
        plugins2.len()
    ));

    if !only_in_1.is_empty() {
        out.out(&format!("\n  Only in '{name1}':"));
        for p in &only_in_1 {
            let name = p.name.as_deref().unwrap_or(&p.dir_name);
            out.out_colored(&format!("    + {name}"), Color::Green);
        }
    }

    if !only_in_2.is_empty() {
        out.out(&format!("\n  Only in '{name2}':"));
        for p in &only_in_2 {
            let name = p.name.as_deref().unwrap_or(&p.dir_name);
            out.out_colored(&format!("    - {name}"), Color::Red);
        }
    }

    let version_diffs: Vec<_> = in_both
        .iter()
        .filter(|(p1, p2)| p1.version != p2.version)
        .collect();

    if !version_diffs.is_empty() {
        out.out("\n  Version differences:");
        for (p1, p2) in &version_diffs {
            let name = p1.name.as_deref().unwrap_or(&p1.dir_name);
            let v1 = p1.version.as_deref().unwrap_or("?");
            let v2 = p2.version.as_deref().unwrap_or("?");
            out.out_colored(&format!("    ~ {name} : {v1} -> {v2}"), Color::Yellow);
        }
    }

    if only_in_1.is_empty() && only_in_2.is_empty() && version_diffs.is_empty() {
        out.out("  (no differences)");
    }

    Ok(())
}

enum DiffKind {
    Added,
    Removed,
    Changed,
}

struct DiffEntry {
    kind: DiffKind,
    text: String,
}

fn diff_settings(
    out: &Output,
    name1: &str,
    profile1: &Profile,
    name2: &str,
    profile2: &Profile,
) -> Result<()> {
    let settings1_path = profile1.config_dir.join(SETTINGS_FILE);
    let settings2_path = profile2.config_dir.join(SETTINGS_FILE);

    let settings1: Option<Value> = if settings1_path.exists() {
        fs::read_to_string(&settings1_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    };

    let settings2: Option<Value> = if settings2_path.exists() {
        fs::read_to_string(&settings2_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    };

    out.heading("=== Settings ===");

    match (&settings1, &settings2) {
        (None, None) => {
            out.out(&format!("  Neither profile has {SETTINGS_FILE}"));
        }
        (Some(_), None) => {
            out.out(&format!("  Only '{name1}' has {SETTINGS_FILE}"));
        }
        (None, Some(_)) => {
            out.out(&format!("  Only '{name2}' has {SETTINGS_FILE}"));
        }
        (Some(v1), Some(v2)) => {
            let diffs = diff_json_objects(v1, v2, "");
            if diffs.is_empty() {
                out.out("  (no differences)");
            } else {
                out.out(&format!("  {} differences found:\n", diffs.len()));
                for diff in diffs.iter().take(MAX_DIFF_DISPLAY) {
                    let color = match diff.kind {
                        DiffKind::Added => Color::Green,
                        DiffKind::Removed => Color::Red,
                        DiffKind::Changed => Color::Yellow,
                    };
                    out.out_colored(&format!("  {}", diff.text), color);
                }
                if diffs.len() > MAX_DIFF_DISPLAY {
                    out.out(&format!("  ... and {} more", diffs.len() - MAX_DIFF_DISPLAY));
                }
            }
        }
    }

    Ok(())
}

fn diff_json_objects(v1: &Value, v2: &Value, prefix: &str) -> Vec<DiffEntry> {
    let mut diffs = Vec::new();

    match (v1, v2) {
        (Value::Object(o1), Value::Object(o2)) => {
            let keys1: HashSet<_> = o1.keys().collect();
            let keys2: HashSet<_> = o2.keys().collect();

            for key in keys1.difference(&keys2) {
                let path = if prefix.is_empty() { (*key).clone() } else { format!("{prefix}.{key}") };
                diffs.push(DiffEntry {
                    kind: DiffKind::Removed,
                    text: format!("- {path} (only in first)"),
                });
            }

            for key in keys2.difference(&keys1) {
                let path = if prefix.is_empty() { (*key).clone() } else { format!("{prefix}.{key}") };
                diffs.push(DiffEntry {
                    kind: DiffKind::Added,
                    text: format!("+ {path} (only in second)"),
                });
            }

            for key in keys1.intersection(&keys2) {
                let path = if prefix.is_empty() { (*key).clone() } else { format!("{prefix}.{key}") };
                diffs.extend(diff_json_objects(&o1[*key], &o2[*key], &path));
            }
        }
        _ if v1 != v2 => {
            let s1 = format_value(v1);
            let s2 = format_value(v2);
            diffs.push(DiffEntry {
                kind: DiffKind::Changed,
                text: format!("~ {prefix} : {s1} -> {s2}"),
            });
        }
        _ => {}
    }

    diffs
}

fn format_value(v: &Value) -> String {
    match v {
        Value::String(s) => {
            if s.len() > MAX_VALUE_DISPLAY_LEN {
                format!("\"{}...\"", &s[..VALUE_PREVIEW_LEN])
            } else {
                format!("{s:?}")
            }
        }
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::Object(o) => format!("{{{} keys}}", o.len()),
        _ => v.to_string(),
    }
}
