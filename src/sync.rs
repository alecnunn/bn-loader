use crate::config::{Config, Profile, default_exclusions};
use crate::fs_util::copy_dir_recursive;
use crate::output::Output;
use anyhow::{Context, Result, anyhow};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const SYNC_ITEMS: &[&str] = &[
    "plugins",
    "repositories",
    "signatures",
    "themes",
    "snippets",
    "types",
    "settings.json",
    "startup.py",
    "keybindings.json",
];

const BACKUP_PREFIX: &str = ".bn-loader-backup-";

pub(crate) struct SyncOptions<'a> {
    pub from: &'a str,
    pub to: Option<&'a str>,
    pub extra_exclusions: Vec<&'a str>,
    pub dry_run: bool,
    pub yes: bool,
    /// If true, skip creating backups in target profile directories before overwriting.
    pub force: bool,
    pub backup_retention: usize,
}

pub(crate) fn run_sync(out: &Output, config: &Config, options: &SyncOptions) -> Result<()> {
    let source = config
        .profiles
        .get(options.from)
        .ok_or_else(|| anyhow!("Source profile '{}' not found", options.from))?;

    let targets: Vec<(&str, &Profile)> = if let Some(to) = options.to {
        let target = config
            .profiles
            .get(to)
            .ok_or_else(|| anyhow!("Target profile '{to}' not found"))?;
        vec![(to, target)]
    } else {
        config
            .profiles
            .iter()
            .filter(|(name, _)| *name != options.from)
            .map(|(name, profile)| (name.as_str(), profile))
            .collect()
    };

    if targets.is_empty() {
        return Err(anyhow!("No target profiles to sync to"));
    }

    let mut exclusions = default_exclusions();
    exclusions.extend(config.sync.exclusions.iter().cloned());
    for excl in &options.extra_exclusions {
        exclusions.push((*excl).to_string());
    }

    let glob_set = build_glob_set(&exclusions)?;
    let items = collect_sync_items(&source.config_dir, &glob_set);

    out.heading("Sync Plan:");
    out.status(&format!(
        "  Source: {} ({})",
        options.from,
        source.config_dir.display()
    ));
    out.status("  Targets:");
    for (name, profile) in &targets {
        out.status(&format!("    - {} ({})", name, profile.config_dir.display()));
    }
    out.status(&format!("  Items to sync: {}", items.len()));
    out.status(&format!("  Exclusions: {exclusions:?}"));

    if items.is_empty() {
        out.status("\nNo items to sync.");
        return Ok(());
    }

    out.status("\nItems:");
    for item in &items {
        out.status(&format!("    {}", item.display()));
    }

    if options.dry_run {
        out.status("\n[Dry run] No changes made.");
        return Ok(());
    }

    if !options.yes && !crate::cli::confirm_prompt("\nProceed?", true)? {
        out.status("Aborted.");
        return Ok(());
    }

    if options.force {
        out.warn("--force: skipping per-target backup creation.");
    }

    for (name, target) in &targets {
        sync_to_target(
            out,
            &source.config_dir,
            &target.config_dir,
            &items,
            name,
            options.backup_retention,
            options.force,
        )?;
    }

    out.success("\nSync complete.");
    Ok(())
}

fn build_glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .with_context(|| format!("Invalid glob pattern '{pattern}'"))?;
        builder.add(glob);
    }
    builder.build().context("Failed to build glob set")
}

fn collect_sync_items(source_dir: &Path, exclusions: &GlobSet) -> Vec<PathBuf> {
    let mut items = Vec::new();
    for item_name in SYNC_ITEMS {
        let item_path = source_dir.join(item_name);
        if item_path.exists() && !exclusions.is_match(item_name) {
            items.push(PathBuf::from(item_name));
        }
    }
    items
}

fn sync_to_target(
    out: &Output,
    source_dir: &Path,
    target_dir: &Path,
    items: &[PathBuf],
    target_name: &str,
    backup_retention: usize,
    force: bool,
) -> Result<()> {
    out.status(&format!("\nSyncing to '{target_name}'..."));

    if !force {
        let backup_dir = create_backup(target_dir, items)?;
        if let Some(ref backup) = backup_dir {
            out.status(&format!("  Backup created: {}", backup.display()));
        }
        if backup_retention > 0 {
            cleanup_old_backups(out, target_dir, backup_retention)?;
        }
    }

    for item in items {
        let source_path = source_dir.join(item);
        let target_path = target_dir.join(item);

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).context("Failed to create directory")?;
            }
            fs::copy(&source_path, &target_path)
                .with_context(|| format!("Failed to copy {}", item.display()))?;
        }
        out.status(&format!("  Copied: {}", item.display()));
    }

    Ok(())
}

fn create_backup(target_dir: &Path, items: &[PathBuf]) -> Result<Option<PathBuf>> {
    let items_to_backup: Vec<&PathBuf> = items
        .iter()
        .filter(|item| target_dir.join(item).exists())
        .collect();

    if items_to_backup.is_empty() {
        return Ok(None);
    }

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("System clock error")?
        .as_secs();
    let backup_name = format!("{BACKUP_PREFIX}{timestamp}");
    let backup_dir = target_dir.join(&backup_name);

    fs::create_dir_all(&backup_dir).context("Failed to create backup directory")?;

    for item in items_to_backup {
        let source = target_dir.join(item);
        let dest = backup_dir.join(item);
        if source.is_dir() {
            copy_dir_recursive(&source, &dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).context("Failed to create backup subdirectory")?;
            }
            fs::copy(&source, &dest)
                .with_context(|| format!("Failed to backup {}", item.display()))?;
        }
    }

    Ok(Some(backup_dir))
}

fn cleanup_old_backups(out: &Output, target_dir: &Path, retention: usize) -> Result<()> {
    let entries = fs::read_dir(target_dir)
        .context("Failed to read directory for backup cleanup")?;

    let mut backups: Vec<(PathBuf, u64)> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(BACKUP_PREFIX) {
                return None;
            }
            let timestamp: u64 = name.strip_prefix(BACKUP_PREFIX)?.parse().ok()?;
            Some((path, timestamp))
        })
        .collect();

    backups.sort_by(|a, b| b.1.cmp(&a.1));

    for (path, _) in backups.into_iter().skip(retention) {
        if let Err(e) = fs::remove_dir_all(&path) {
            out.warn(&format!(
                "  Warning: Failed to remove old backup {}: {e}",
                path.display()
            ));
        } else {
            out.status(&format!("  Removed old backup: {}", path.display()));
        }
    }

    Ok(())
}
