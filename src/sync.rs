use crate::config::{Config, Profile, default_exclusions};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::io::{self, Write};
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
    pub backup_retention: usize,
}

pub(crate) fn run_sync(config: &Config, options: &SyncOptions) -> Result<(), String> {
    let source = config
        .profiles
        .get(options.from)
        .ok_or_else(|| format!("Source profile '{}' not found", options.from))?;

    let targets: Vec<(&str, &Profile)> = if let Some(to) = options.to {
        let target = config
            .profiles
            .get(to)
            .ok_or_else(|| format!("Target profile '{to}' not found"))?;
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
        return Err("No target profiles to sync to".to_string());
    }

    // Start with defaults, add config exclusions, then CLI exclusions
    let mut exclusions = default_exclusions();
    exclusions.extend(config.sync.exclusions.iter().cloned());
    for excl in &options.extra_exclusions {
        exclusions.push((*excl).to_string());
    }

    let glob_set = build_glob_set(&exclusions)?;
    let items = collect_sync_items(&source.config_dir, &glob_set)?;

    println!("Sync Plan:");
    println!(
        "  Source: {} ({})",
        options.from,
        source.config_dir.display()
    );
    println!("  Targets:");
    for (name, profile) in &targets {
        println!("    - {} ({})", name, profile.config_dir.display());
    }
    println!("  Items to sync: {}", items.len());
    println!("  Exclusions: {exclusions:?}");

    if items.is_empty() {
        println!("\nNo items to sync.");
        return Ok(());
    }

    println!("\nItems:");
    for item in &items {
        println!("    {}", item.display());
    }

    if options.dry_run {
        println!("\n[Dry run] No changes made.");
        return Ok(());
    }

    if !options.yes {
        print!("\nProceed? [y/N] ");
        io::stdout()
            .flush()
            .map_err(|e| format!("Failed to flush stdout: {e}"))?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read input: {e}"))?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    for (name, target) in &targets {
        sync_to_target(
            &source.config_dir,
            &target.config_dir,
            &items,
            name,
            options.backup_retention,
        )?;
    }

    println!("\nSync complete.");
    Ok(())
}

fn build_glob_set(patterns: &[String]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob =
            Glob::new(pattern).map_err(|e| format!("Invalid glob pattern '{pattern}': {e}"))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| format!("Failed to build glob set: {e}"))
}

fn collect_sync_items(source_dir: &Path, exclusions: &GlobSet) -> Result<Vec<PathBuf>, String> {
    let mut items = Vec::new();

    for item_name in SYNC_ITEMS {
        let item_path = source_dir.join(item_name);
        if item_path.exists() && !exclusions.is_match(item_name) {
            items.push(PathBuf::from(item_name));
        }
    }

    Ok(items)
}

fn sync_to_target(
    source_dir: &Path,
    target_dir: &Path,
    items: &[PathBuf],
    target_name: &str,
    backup_retention: usize,
) -> Result<(), String> {
    println!("\nSyncing to '{target_name}'...");

    let backup_dir = create_backup(target_dir, items)?;
    if let Some(ref backup) = backup_dir {
        println!("  Backup created: {}", backup.display());
    }

    // Clean up old backups if retention is set
    if backup_retention > 0 {
        cleanup_old_backups(target_dir, backup_retention)?;
    }

    for item in items {
        let source_path = source_dir.join(item);
        let target_path = target_dir.join(item);

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create directory: {e}"))?;
            }
            fs::copy(&source_path, &target_path)
                .map_err(|e| format!("Failed to copy {}: {}", item.display(), e))?;
        }
        println!("  Copied: {}", item.display());
    }

    Ok(())
}

fn create_backup(target_dir: &Path, items: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    let items_to_backup: Vec<&PathBuf> = items
        .iter()
        .filter(|item| target_dir.join(item).exists())
        .collect();

    if items_to_backup.is_empty() {
        return Ok(None);
    }

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| format!("System clock error: {e}"))?
        .as_secs();
    let backup_name = format!("{BACKUP_PREFIX}{timestamp}");
    let backup_dir = target_dir.join(&backup_name);

    fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {e}"))?;

    for item in items_to_backup {
        let source = target_dir.join(item);
        let dest = backup_dir.join(item);

        if source.is_dir() {
            copy_dir_recursive(&source, &dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create backup subdirectory: {e}"))?;
            }
            fs::copy(&source, &dest)
                .map_err(|e| format!("Failed to backup {}: {}", item.display(), e))?;
        }
    }

    Ok(Some(backup_dir))
}

fn cleanup_old_backups(target_dir: &Path, retention: usize) -> Result<(), String> {
    let entries = fs::read_dir(target_dir)
        .map_err(|e| format!("Failed to read directory for backup cleanup: {e}"))?;

    // Collect all backup directories with their timestamps
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
            // Extract timestamp from name
            let timestamp: u64 = name.strip_prefix(BACKUP_PREFIX)?.parse().ok()?;
            Some((path, timestamp))
        })
        .collect();

    // Sort by timestamp (newest first)
    backups.sort_by(|a, b| b.1.cmp(&a.1));

    // Remove old backups beyond retention limit
    for (path, _) in backups.into_iter().skip(retention) {
        if let Err(e) = fs::remove_dir_all(&path) {
            eprintln!(
                "  Warning: Failed to remove old backup {}: {e}",
                path.display()
            );
        } else {
            println!("  Removed old backup: {}", path.display());
        }
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|e| format!("Failed to remove existing directory: {e}"))?;
    }

    fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {}: {}", dst.display(), e))?;

    for entry in fs::read_dir(src)
        .map_err(|e| format!("Failed to read directory {}: {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| format!("Failed to copy file: {e}"))?;
        }
    }

    Ok(())
}
