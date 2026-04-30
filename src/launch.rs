use crate::config::{ENV_VAR_NAME, Profile};
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

const DEBUG_LOG_FILENAME: &str = "bn-loader-debug.log";

#[derive(Default)]
pub(crate) struct LaunchOptions<'a> {
    pub debug: bool,
    pub log_file: Option<&'a PathBuf>,
}

pub(crate) fn launch_profile(
    out: &crate::output::Output,
    name: &str,
    profile: &Profile,
    options: &LaunchOptions,
) -> Result<()> {
    let exe_path = profile.install_dir.join(&profile.executable);

    if !profile.install_dir.exists() {
        return Err(anyhow!(
            "Install directory does not exist: {}",
            profile.install_dir.display()
        ));
    }

    if !exe_path.exists() {
        return Err(anyhow!("Executable not found: {}", exe_path.display()));
    }

    if !profile.config_dir.exists() {
        return Err(anyhow!(
            "Config directory does not exist: {}",
            profile.config_dir.display()
        ));
    }

    let use_debug = options.debug || profile.debug;

    out.heading(&format!("Launching profile '{name}'..."));
    out.status(&format!("  Install dir: {}", profile.install_dir.display()));
    out.status(&format!("  Config dir:  {}", profile.config_dir.display()));
    out.status(&format!("  Executable:  {}", profile.executable));

    if use_debug {
        launch_debug(out, profile, &exe_path, options)
    } else {
        launch_normal(profile, &exe_path)
    }
}

fn launch_normal(profile: &Profile, exe_path: &Path) -> Result<()> {
    Command::new(exe_path)
        .current_dir(&profile.install_dir)
        .env(ENV_VAR_NAME, &profile.config_dir)
        .spawn()
        .context("Failed to launch Binary Ninja")?;
    Ok(())
}

fn launch_debug(
    out: &crate::output::Output,
    profile: &Profile,
    exe_path: &Path,
    options: &LaunchOptions,
) -> Result<()> {
    let log_path = options
        .log_file
        .cloned()
        .unwrap_or_else(|| profile.config_dir.join(DEBUG_LOG_FILENAME));

    out.status("  Debug mode: enabled");
    out.status(&format!("  Log file:   {}", log_path.display()));

    // Use Binary Ninja's native debug flags: -d for debug mode, -l for log file
    let child = Command::new(exe_path)
        .current_dir(&profile.install_dir)
        .env(ENV_VAR_NAME, &profile.config_dir)
        .arg("-d")
        .arg("-l")
        .arg(&log_path)
        .spawn()
        .context("Failed to launch Binary Ninja")?;

    out.success(&format!("\nBinary Ninja launched (PID: {}).", child.id()));
    out.status(&format!("Debug logs will be written to: {}", log_path.display()));

    #[cfg(windows)]
    out.status(&format!(
        "\nTo monitor: Get-Content -Path \"{}\" -Wait",
        log_path.display()
    ));

    #[cfg(not(windows))]
    out.status(&format!("\nTo monitor: tail -f \"{}\"", log_path.display()));

    Ok(())
}
