use crate::config::{ENV_VAR_NAME, Profile};
use std::path::{Path, PathBuf};
use std::process::Command;

const DEBUG_LOG_FILENAME: &str = "bn-loader-debug.log";

#[derive(Default)]
pub(crate) struct LaunchOptions<'a> {
    pub debug: bool,
    pub log_file: Option<&'a PathBuf>,
}

pub(crate) fn launch_profile(
    name: &str,
    profile: &Profile,
    options: &LaunchOptions,
) -> Result<(), String> {
    let exe_path = profile.install_dir.join(&profile.executable);

    if !profile.install_dir.exists() {
        return Err(format!(
            "Install directory does not exist: {}",
            profile.install_dir.display()
        ));
    }

    if !exe_path.exists() {
        return Err(format!("Executable not found: {}", exe_path.display()));
    }

    if !profile.config_dir.exists() {
        return Err(format!(
            "Config directory does not exist: {}",
            profile.config_dir.display()
        ));
    }

    let use_debug = options.debug || profile.debug;

    println!("Launching profile '{name}'...");
    println!("  Install dir: {}", profile.install_dir.display());
    println!("  Config dir:  {}", profile.config_dir.display());
    println!("  Executable:  {}", profile.executable);

    if use_debug {
        launch_debug(profile, &exe_path, options)
    } else {
        launch_normal(profile, &exe_path)
    }
}

fn launch_normal(profile: &Profile, exe_path: &Path) -> Result<(), String> {
    Command::new(exe_path)
        .current_dir(&profile.install_dir)
        .env(ENV_VAR_NAME, &profile.config_dir)
        .spawn()
        .map_err(|e| format!("Failed to launch Binary Ninja: {e}"))?;
    Ok(())
}

fn launch_debug(profile: &Profile, exe_path: &Path, options: &LaunchOptions) -> Result<(), String> {
    let log_path = options
        .log_file
        .cloned()
        .unwrap_or_else(|| profile.config_dir.join(DEBUG_LOG_FILENAME));

    println!("  Debug mode: enabled");
    println!("  Log file:   {}", log_path.display());

    // Use Binary Ninja's native debug flags: -d for debug mode, -l for log file
    let child = Command::new(exe_path)
        .current_dir(&profile.install_dir)
        .env(ENV_VAR_NAME, &profile.config_dir)
        .arg("-d")
        .arg("-l")
        .arg(&log_path)
        .spawn()
        .map_err(|e| format!("Failed to launch Binary Ninja: {e}"))?;

    println!("\nBinary Ninja launched (PID: {}).", child.id());
    println!("Debug logs will be written to: {}", log_path.display());

    #[cfg(windows)]
    println!(
        "\nTo monitor: Get-Content -Path \"{}\" -Wait",
        log_path.display()
    );

    #[cfg(not(windows))]
    println!("\nTo monitor: tail -f \"{}\"", log_path.display());

    Ok(())
}
