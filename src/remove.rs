use crate::config::{Config, remove_profile_from_config};
use crate::output::Output;
use anyhow::{Context, Result, bail};
use std::path::Path;

pub(crate) struct RemoveOptions<'a> {
    pub name: &'a str,
    pub purge: bool,
    pub force: bool,
    pub yes: bool,
    pub dry_run: bool,
}

pub(crate) fn run_remove(
    out: &Output,
    config: &Config,
    config_path: &Path,
    options: &RemoveOptions,
) -> Result<()> {
    let profile = config
        .profiles
        .get(options.name)
        .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", options.name))?;

    out.heading(&format!("Removing profile '{}'", options.name));
    out.status(&format!("  install_dir: {}", profile.install_dir.display()));
    out.status(&format!("  config_dir:  {}", profile.config_dir.display()));

    let install_users: Vec<&String> = config
        .profiles
        .iter()
        .filter(|(n, p)| *n != options.name && p.install_dir == profile.install_dir)
        .map(|(n, _)| n)
        .collect();
    if !install_users.is_empty() {
        out.warn(&format!(
            "install_dir is shared with {} other profile(s): {}",
            install_users.len(),
            install_users
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let cfg_users: Vec<&String> = config
        .profiles
        .iter()
        .filter(|(n, p)| *n != options.name && p.config_dir == profile.config_dir)
        .map(|(n, _)| n)
        .collect();
    if !cfg_users.is_empty() {
        out.warn(&format!(
            "config_dir is shared with {} other profile(s): {}",
            cfg_users.len(),
            cfg_users
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if options.purge {
        out.warn(&format!(
            "--purge: will delete config_dir {}",
            profile.config_dir.display()
        ));
        if options.force {
            out.warn(&format!(
                "--purge --force: will also delete install_dir {}",
                profile.install_dir.display()
            ));
        }
        if !cfg_users.is_empty() {
            bail!(
                "Cannot --purge config_dir: it's shared with other profiles ({}). Remove those first or skip --purge.",
                cfg_users
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if options.force && !install_users.is_empty() {
            bail!(
                "Cannot --purge --force install_dir: it's shared with other profiles ({}). Remove those first or skip --force.",
                install_users
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    if options.dry_run {
        out.status("\n[Dry run] No changes made.");
        return Ok(());
    }

    if !options.yes && !crate::cli::confirm_prompt("\nProceed with removal?", true)? {
        out.status("Aborted.");
        return Ok(());
    }

    if options.purge {
        if profile.config_dir.exists() {
            std::fs::remove_dir_all(&profile.config_dir).with_context(|| {
                format!(
                    "Failed to delete config_dir {}",
                    profile.config_dir.display()
                )
            })?;
            out.status(&format!("  Deleted: {}", profile.config_dir.display()));
        }
        if options.force && profile.install_dir.exists() {
            std::fs::remove_dir_all(&profile.install_dir).with_context(|| {
                format!(
                    "Failed to delete install_dir {}",
                    profile.install_dir.display()
                )
            })?;
            out.status(&format!("  Deleted: {}", profile.install_dir.display()));
        }
    }

    remove_profile_from_config(config_path, options.name)?;
    out.success(&format!("\nRemoved profile '{}'.", options.name));
    if !options.purge {
        out.status("Disk artifacts left in place. Use --purge to also delete config_dir, --purge --force to also delete install_dir.");
    }

    Ok(())
}
