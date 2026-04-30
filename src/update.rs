use crate::config::Config;
use crate::install::{InstallOptions, run_install};
use crate::output::Output;
use anyhow::Result;
use std::path::Path;

pub(crate) struct UpdateOptions<'a> {
    pub name: &'a str,
    pub archive: &'a Path,
    pub yes: bool,
    pub dry_run: bool,
    pub seven_zip: Option<&'a Path>,
}

pub(crate) fn run_update(
    out: &Output,
    config: &Config,
    config_path: &Path,
    options: &UpdateOptions,
) -> Result<()> {
    let profile = config
        .profiles
        .get(options.name)
        .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", options.name))?;

    out.heading(&format!("Updating profile '{}'", options.name));
    out.status(&format!("  Archive:     {}", options.archive.display()));
    out.status(&format!("  Install dir: {}", profile.install_dir.display()));

    let install_options = InstallOptions {
        archive: options.archive,
        dest: &profile.install_dir,
        name: None,
        config_dir: None,
        no_register: true,
        force: true,
        // In dry-run mode, suppress the interactive confirmation: the plan is shown but
        // nothing is written, so prompting would only confuse scripted / piped invocations.
        yes: options.yes || options.dry_run,
        dry_run: options.dry_run,
        seven_zip: options.seven_zip,
        config_path,
    };
    run_install(out, config, &install_options)
}
