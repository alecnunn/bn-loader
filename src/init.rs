use crate::config::Config;
use std::fs;
use std::io::Write;
use std::path::Path;

const LICENSE_FILES: &[&str] = &["license.dat", "license.txt"];

pub(crate) struct InitOptions<'a> {
    pub name: &'a str,
    pub template: &'a str,
    pub config_dir: &'a Path,
}

pub(crate) fn run_init(
    config: &Config,
    config_path: &Path,
    options: &InitOptions,
) -> Result<(), String> {
    // Validate template exists
    let template_profile = config
        .profiles
        .get(options.template)
        .ok_or_else(|| format!("Template profile '{}' not found", options.template))?;

    // Check if profile name already exists
    if config.profiles.contains_key(options.name) {
        return Err(format!("Profile '{}' already exists", options.name));
    }

    // Check if config_dir already exists
    if options.config_dir.exists() {
        return Err(format!(
            "Config directory already exists: {}",
            options.config_dir.display()
        ));
    }

    println!("Initializing profile '{}'...", options.name);
    println!("  Template:    {}", options.template);
    println!("  Install dir: {}", template_profile.install_dir.display());
    println!("  Config dir:  {}", options.config_dir.display());

    // Create the config directory
    fs::create_dir_all(options.config_dir)
        .map_err(|e| format!("Failed to create config directory: {e}"))?;

    // Copy license files from template
    let mut copied_files = Vec::new();
    for license_file in LICENSE_FILES {
        let src = template_profile.config_dir.join(license_file);
        if src.exists() {
            let dst = options.config_dir.join(license_file);
            fs::copy(&src, &dst).map_err(|e| format!("Failed to copy {license_file}: {e}"))?;
            copied_files.push(*license_file);
        }
    }

    if copied_files.is_empty() {
        eprintln!(
            "Warning: No license files found in template profile at {}",
            template_profile.config_dir.display()
        );
    } else {
        println!("  Copied:      {}", copied_files.join(", "));
    }

    // Append new profile to config file
    append_profile_to_config(
        config_path,
        options.name,
        &template_profile.install_dir,
        options.config_dir,
    )?;

    println!("\nProfile '{}' initialized successfully.", options.name);
    println!("You can now launch it with: bn-loader {}", options.name);

    Ok(())
}

fn append_profile_to_config(
    config_path: &Path,
    name: &str,
    install_dir: &Path,
    config_dir: &Path,
) -> Result<(), String> {
    // Validate profile name to prevent TOML injection
    if !is_valid_profile_name(name) {
        return Err(format!(
            "Invalid profile name '{name}': must contain only alphanumeric characters, hyphens, and underscores"
        ));
    }

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(config_path)
        .map_err(|e| format!("Failed to open config file: {e}"))?;

    // Use toml crate to properly escape path values
    let install_str = install_dir.to_string_lossy();
    let config_str = config_dir.to_string_lossy();
    let install_escaped = toml::Value::String(install_str.into_owned());
    let config_escaped = toml::Value::String(config_str.into_owned());

    let profile_toml = format!(
        "\n[profiles.{name}]\ninstall_dir = {install_escaped}\nconfig_dir = {config_escaped}\n"
    );

    file.write_all(profile_toml.as_bytes())
        .map_err(|e| format!("Failed to write to config file: {e}"))?;

    println!("  Added profile to: {}", config_path.display());

    Ok(())
}

fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}
