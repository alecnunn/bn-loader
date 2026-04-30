use crate::config::{Config, append_profile_to_config};
use crate::output::Output;
use anyhow::{Context, Result, anyhow, bail};
use std::fs;
use std::path::Path;

const LICENSE_FILES: &[&str] = &["license.dat", "license.txt"];

pub(crate) struct InitOptions<'a> {
    pub name: &'a str,
    pub template: &'a str,
    pub config_dir: &'a Path,
    pub dry_run: bool,
    /// Reserved for future interactive prompts. Currently no-op.
    pub yes: bool,
}

pub(crate) fn run_init(
    out: &Output,
    config: &Config,
    config_path: &Path,
    options: &InitOptions,
) -> Result<()> {
    let _ = options.yes; // Reserved for future prompts.

    let template_profile = config
        .profiles
        .get(options.template)
        .ok_or_else(|| anyhow!("Template profile '{}' not found", options.template))?;

    if config.profiles.contains_key(options.name) {
        bail!("Profile '{}' already exists", options.name);
    }

    if options.config_dir.exists() {
        bail!(
            "Config directory already exists: {}",
            options.config_dir.display()
        );
    }

    out.heading(&format!("Initializing profile '{}'...", options.name));
    out.status(&format!("  Template:    {}", options.template));
    out.status(&format!(
        "  Install dir: {}",
        template_profile.install_dir.display()
    ));
    out.status(&format!("  Config dir:  {}", options.config_dir.display()));

    if options.dry_run {
        out.status("\n[Dry run] No changes made.");
        return Ok(());
    }

    fs::create_dir_all(options.config_dir).context("Failed to create config directory")?;

    let mut copied_files = Vec::new();
    for license_file in LICENSE_FILES {
        let src = template_profile.config_dir.join(license_file);
        if src.exists() {
            let dst = options.config_dir.join(license_file);
            fs::copy(&src, &dst).with_context(|| format!("Failed to copy {license_file}"))?;
            copied_files.push(*license_file);
        }
    }

    if copied_files.is_empty() {
        out.warn(&format!(
            "Warning: No license files found in template profile at {}",
            template_profile.config_dir.display()
        ));
    } else {
        out.status(&format!("  Copied:      {}", copied_files.join(", ")));
    }

    append_profile_to_config(
        config_path,
        options.name,
        &template_profile.install_dir,
        options.config_dir,
    )?;

    out.success(&format!(
        "\nProfile '{}' initialized successfully.",
        options.name
    ));
    out.status(&format!(
        "You can now launch it with: bn-loader {}",
        options.name
    ));

    Ok(())
}
