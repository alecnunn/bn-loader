use crate::config::{Config, default_profiles_dir};
use crate::output::Output;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// Result of one doctor check.
#[allow(dead_code)]
enum Status {
    Ok,
    Warn(String),
    Fail(String),
}

pub(crate) fn run_doctor(out: &Output, config: &Config) -> Result<i32> {
    let mut failures = 0usize;
    let mut warnings = 0usize;
    let mut checks = 0usize;

    out.heading("Doctor: validating bn-loader config...\n");

    for (name, profile) in &config.profiles {
        out.heading(&format!("Profile '{name}':"));

        checks += 1;
        match check_dir(&profile.install_dir) {
            Status::Ok => out.success(&format!(
                "  [OK]   install_dir: {}",
                profile.install_dir.display()
            )),
            Status::Warn(msg) => {
                out.warn(&format!("  [WARN] install_dir: {msg}"));
                warnings += 1;
            }
            Status::Fail(msg) => {
                out.warn(&format!("  [FAIL] install_dir: {msg}"));
                failures += 1;
            }
        }

        let exe_path = profile.install_dir.join(&profile.executable);
        checks += 1;
        if exe_path.is_file() {
            out.success(&format!("  [OK]   executable: {}", exe_path.display()));
        } else {
            out.warn(&format!(
                "  [FAIL] executable: not found at {}",
                exe_path.display()
            ));
            failures += 1;
        }

        checks += 1;
        match check_dir(&profile.config_dir) {
            Status::Ok => out.success(&format!(
                "  [OK]   config_dir: {}",
                profile.config_dir.display()
            )),
            Status::Warn(msg) => {
                out.warn(&format!("  [WARN] config_dir: {msg}"));
                warnings += 1;
            }
            Status::Fail(msg) => {
                out.warn(&format!("  [FAIL] config_dir: {msg}"));
                failures += 1;
            }
        }
    }

    out.heading("\nGlobal:");

    checks += 1;
    if let Some(seven_zip) = &config.install.seven_zip {
        if seven_zip.is_file() {
            out.success(&format!(
                "  [OK]   [install] seven_zip: {}",
                seven_zip.display()
            ));
        } else {
            out.warn(&format!(
                "  [FAIL] [install] seven_zip: not a file: {}",
                seven_zip.display()
            ));
            failures += 1;
        }
    } else {
        out.success("  [OK]   [install] seven_zip: not set (will fall back to PATH)");
    }

    checks += 1;
    if let Some(profiles_dir) = default_profiles_dir() {
        if profiles_dir.is_dir() {
            let referenced: HashSet<&Path> = config
                .profiles
                .values()
                .map(|p| p.config_dir.as_path())
                .collect();
            let mut orphans: Vec<String> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && !referenced.contains(path.as_path()) {
                        orphans.push(path.display().to_string());
                    }
                }
            }
            if orphans.is_empty() {
                out.success(&format!(
                    "  [OK]   no orphan dirs under {}",
                    profiles_dir.display()
                ));
            } else {
                out.warn(&format!(
                    "  [WARN] orphan dirs under {} (not referenced by any profile): {}",
                    profiles_dir.display(),
                    orphans.join(", ")
                ));
                warnings += 1;
            }
        } else {
            out.success(&format!(
                "  [OK]   default profiles dir does not exist yet: {}",
                profiles_dir.display()
            ));
        }
    } else {
        out.warn("  [WARN] could not determine home directory; orphan check skipped");
        warnings += 1;
    }

    out.out(&format!(
        "Doctor: {} checks, {} warning(s), {} failure(s).",
        checks, warnings, failures
    ));

    Ok(if failures > 0 { 1 } else { 0 })
}

fn check_dir(p: &Path) -> Status {
    if !p.exists() {
        return Status::Fail(format!("does not exist: {}", p.display()));
    }
    if !p.is_dir() {
        return Status::Fail(format!("exists but is not a directory: {}", p.display()));
    }
    Status::Ok
}
