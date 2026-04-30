use crate::config::Config;
use anyhow::{Context, Result, anyhow, bail};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Options for the `install` subcommand.
pub(crate) struct InstallOptions<'a> {
    /// Path to the archive on local disk (.zip or .exe).
    pub archive: &'a Path,
    /// Target installation directory (created if missing).
    pub dest: &'a Path,
    /// Optional explicit profile name. If None, derived from the basename of `dest`.
    pub name: Option<&'a str>,
    /// Optional explicit config directory. If None, derived from the (possibly bumped) name.
    pub config_dir: Option<&'a Path>,
    /// If true, skip profile registration entirely (install is extract-only).
    pub no_register: bool,
    /// Allow destructive overrides (e.g., extracting on top of a non-empty dest).
    pub force: bool,
    /// Skip interactive [y/N] confirmation prompts.
    pub yes: bool,
    /// Print plan and exit without writing anything.
    pub dry_run: bool,
    /// Optional 7z executable path override (CLI flag).
    pub seven_zip: Option<&'a Path>,
    /// Path to the active config file (used to append a new profile entry when registering).
    pub config_path: &'a Path,
}

/// Detected archive kind, chosen by file extension.
enum ArchiveType {
    Zip,
    NsisExe,
}

fn detect_archive_type(archive: &Path) -> Result<ArchiveType> {
    let ext = archive
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);

    match ext.as_deref() {
        Some("zip") => Ok(ArchiveType::Zip),
        Some("exe") => {
            if !cfg!(windows) {
                bail!(
                    "Windows installer .exe archives can only be extracted on Windows (current target lacks 7z + NSIS interop in this tool)"
                );
            }
            Ok(ArchiveType::NsisExe)
        }
        Some(other) => bail!("Unsupported archive extension '.{other}': expected .zip or .exe"),
        None => bail!("Archive has no extension; cannot detect type"),
    }
}

fn validate_dest(
    out: &crate::output::Output,
    dest: &Path,
    force: bool,
    yes: bool,
    config: &Config,
) -> Result<()> {
    let exists = dest.exists();
    let is_empty = if exists {
        if !dest.is_dir() {
            bail!(
                "Destination exists but is not a directory: {}",
                dest.display()
            );
        }
        std::fs::read_dir(dest)
            .with_context(|| format!("Failed to read destination directory {}", dest.display()))?
            .next()
            .is_none()
    } else {
        true
    };

    let users = profiles_using_install_dir(config, dest);
    let needs_override = !is_empty || !users.is_empty();

    if !needs_override {
        return Ok(());
    }

    if !is_empty {
        out.warn(&format!(
            "Warning: destination directory is not empty: {}",
            dest.display()
        ));
        out.status(
            "Files in the archive will overlay existing entries with the same name. Non-conflicting files are preserved (no pre-clean)."
        );
    }
    if !users.is_empty() {
        out.warn(&format!(
            "Install path is currently referenced by {} profile(s): {}",
            users.len(),
            users.join(", ")
        ));
    }

    if !is_empty && !force {
        bail!(
            "Destination is not empty and --force was not given. Pass --force to overlay-extract."
        );
    }

    if force && !is_empty {
        out.status(&format!(
            "(--force) Will overlay-extract on top of non-empty {}",
            dest.display()
        ));
    }

    if yes {
        return Ok(());
    }

    if !crate::cli::confirm_prompt("Continue?", true)? {
        bail!("Aborted by user.");
    }
    Ok(())
}

/// Sorted list of profile names whose `install_dir` resolves to the same path as `dest`.
///
/// Matching strategy: prefer `fs::canonicalize` on both paths when both succeed;
/// otherwise fall back to lexical equality of path components. The fallback may miss
/// slightly-different surface forms of the same path -- but the worst case is a missed
/// warning (same as current behavior with no warning at all).
fn profiles_using_install_dir(config: &Config, dest: &Path) -> Vec<String> {
    let mut names: Vec<String> = config
        .profiles
        .iter()
        .filter(|(_, profile)| paths_refer_to_same(&profile.install_dir, dest))
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names
}

/// True if `a` and `b` refer to the same filesystem location, to the extent we can tell
/// without requiring either path to exist.
fn paths_refer_to_same(a: &Path, b: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return ca == cb;
    }
    a.components().collect::<Vec<_>>() == b.components().collect::<Vec<_>>()
}

pub(crate) fn run_install(
    out: &crate::output::Output,
    config: &Config,
    options: &InstallOptions,
) -> Result<()> {
    let archive_type = detect_archive_type(options.archive)?;
    validate_dest(out, options.dest, options.force, options.yes, config)?;

    let registration_plan = if options.no_register {
        None
    } else {
        Some(plan_profile_registration(config, options)?)
    };

    if options.dry_run {
        out.heading("[Dry run] Install plan:");
        out.status(&format!("  Archive: {}", options.archive.display()));
        out.status(&format!("  Dest:    {}", options.dest.display()));
        if let Some(plan) = &registration_plan {
            out.status(&format!(
                "  Register profile '{}' (config_dir={})",
                plan.name,
                plan.config_dir.display()
            ));
        } else {
            out.status("  No profile registration (--no-register).");
        }
        out.status("\n[Dry run] No changes made.");
        return Ok(());
    }

    match archive_type {
        ArchiveType::Zip => extract_zip(out, options.archive, options.dest)?,
        ArchiveType::NsisExe => {
            let seven_zip = resolve_seven_zip(options.seven_zip, config)?;
            extract_nsis(out, options.archive, options.dest, &seven_zip)?;
        }
    }

    out.success(&format!("\nInstalled to {}", options.dest.display()));

    if let Some(plan) = registration_plan {
        fs::create_dir_all(&plan.config_dir).with_context(|| {
            format!(
                "Failed to create config directory {}",
                plan.config_dir.display()
            )
        })?;
        crate::config::append_profile_to_config(
            options.config_path,
            &plan.name,
            options.dest,
            &plan.config_dir,
        )?;
        if let Some(original) = &plan.original_name {
            out.warn(&format!(
                "Note: profile name '{}' was already taken; registered as '{}' instead.",
                original, plan.name
            ));
        }
        out.success(&format!(
            "Registered profile '{}' (install_dir={}, config_dir={})",
            plan.name,
            options.dest.display(),
            plan.config_dir.display()
        ));
        out.status(&format!("Launch it with: bn-loader {}", plan.name));
    }

    Ok(())
}

/// Resolve the path to a 7z executable.
///
/// Resolution order:
/// 1. Explicit `--seven-zip` CLI flag.
/// 2. `[install] seven_zip` config field.
/// 3. Looked up by name on `$PATH`.
///
/// If none of these locate an existing executable, returns an error with installation guidance.
fn resolve_seven_zip(cli_flag: Option<&Path>, config: &Config) -> Result<PathBuf> {
    if let Some(p) = cli_flag {
        if p.is_file() {
            return Ok(p.to_path_buf());
        }
        bail!("--seven-zip path does not point to a file: {}", p.display());
    }

    if let Some(p) = config.install.seven_zip.as_deref() {
        if p.is_file() {
            return Ok(p.to_path_buf());
        }
        bail!(
            "[install] seven_zip path in config does not point to a file: {}",
            p.display()
        );
    }

    let candidate = if cfg!(windows) { "7z.exe" } else { "7z" };
    if let Some(found) = which(candidate) {
        return Ok(found);
    }

    Err(anyhow!(
        "Could not find 7z executable. Install 7-Zip (https://www.7-zip.org/) and either put it on PATH, pass --seven-zip <path>, or set [install] seven_zip in your config."
    ))
}

/// Minimal $PATH lookup for an executable name. Avoids pulling in a `which` crate dep.
fn which(name: &str) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Extract a ZIP archive into `dest`.
///
/// If the archive has exactly one top-level entry and that entry is a directory,
/// it is stripped — the directory's children land directly under `dest`. This matches
/// the Linux Binary Ninja bundle layout (`binaryninja/...`) and the user's intuition
/// that `--dest /opt/binaryninja-personal` should produce `/opt/binaryninja-personal/binaryninja`,
/// not `/opt/binaryninja-personal/binaryninja/binaryninja`.
fn extract_zip(out: &crate::output::Output, archive: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive)
        .with_context(|| format!("Failed to open archive {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("Failed to read ZIP archive")?;

    fs::create_dir_all(dest)
        .with_context(|| format!("Failed to create destination directory {}", dest.display()))?;

    let strip_prefix = detect_zip_strip_prefix(&mut zip)?;

    out.status(&format!(
        "Extracting {} entries from {}{}",
        zip.len(),
        archive.display(),
        if let Some(p) = &strip_prefix {
            format!(" (stripping leading '{}/')", p.display())
        } else {
            String::new()
        }
    ));

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .with_context(|| format!("Failed to read entry {i} from archive"))?;

        // Use enclosed_name to defeat directory-traversal attempts.
        let raw_name = match entry.enclosed_name() {
            Some(n) => n,
            None => continue, // unsafe path (e.g., absolute or contains ..) — skip silently
        };

        let relative = match &strip_prefix {
            Some(prefix) => match raw_name.strip_prefix(prefix) {
                Ok(p) if p.as_os_str().is_empty() => continue, // the stripped dir entry itself
                Ok(p) => p.to_path_buf(),
                Err(_) => raw_name.clone(), // shouldn't happen if detect_zip_strip_prefix is correct
            },
            None => raw_name.clone(),
        };

        let out_path = dest.join(&relative);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .with_context(|| format!("Failed to create directory {}", out_path.display()))?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create parent directory {}", parent.display())
            })?;
        }

        let mut out_file = fs::File::create(&out_path)
            .with_context(|| format!("Failed to create output file {}", out_path.display()))?;
        io::copy(&mut entry, &mut out_file)
            .with_context(|| format!("Failed to write {}", out_path.display()))?;

        // Preserve unix executable bit when present (binaryninja, crashpad_handler, scc, etc.).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let perms = std::fs::Permissions::from_mode(mode);
                let _ = std::fs::set_permissions(&out_path, perms);
            }
        }
    }

    Ok(())
}

/// If the archive has exactly one unique top-level path component AND no entry uses
/// that component as its full path (i.e., it's only ever the parent of nested entries),
/// return that component. Otherwise return None.
///
/// Many ZIPs omit explicit directory entries — only the leaf files are listed, with
/// directory paths inferred from the slashes. So we cannot rely on finding an explicit
/// `binaryninja/` directory entry; we have to detect the wrapper by structure.
fn detect_zip_strip_prefix(zip: &mut zip::ZipArchive<fs::File>) -> Result<Option<PathBuf>> {
    use std::collections::HashSet;

    let mut top_level: HashSet<PathBuf> = HashSet::new();
    let mut top_level_appears_as_file = false;

    for i in 0..zip.len() {
        let entry = zip.by_index(i).with_context(|| {
            format!("Failed to read entry {i} while detecting top-level prefix")
        })?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let mut components = name.components();
        let Some(first) = components.next() else {
            continue;
        };
        let first_path = PathBuf::from(first.as_os_str());

        // If this entry IS the top-level (no further components) AND it's a file
        // (not a directory entry), then the top-level isn't a directory wrapper —
        // it's a real file at the archive root. Don't strip.
        let has_more = components.next().is_some();
        if !has_more && !entry.is_dir() {
            top_level_appears_as_file = true;
        }

        top_level.insert(first_path);

        if top_level.len() > 1 {
            return Ok(None);
        }
    }

    if top_level.len() == 1 && !top_level_appears_as_file {
        let only = top_level.into_iter().next().unwrap();
        return Ok(Some(only));
    }

    Ok(None)
}

/// Names (or directory roots) extracted from the NSIS installer that are NOT part of
/// the actual Binary Ninja install. Filtered out before moving to dest.
///
/// Verified against `binaryninja_win64_5.3.9434_personal.exe` via `7z l`. Includes:
/// - `$PLUGINSDIR/`: NSIS internal staging directory.
/// - `*.bmp`, `icon.ico`: installer chrome/branding.
/// - `vc_redist*.exe`, `vcredist*.exe`: VC++ redistributables run by the installer.
fn is_nsis_artifact(top_name: &str) -> bool {
    if top_name == "$PLUGINSDIR" {
        return true;
    }
    if top_name.eq_ignore_ascii_case("icon.ico") {
        return true;
    }
    let lower = top_name.to_ascii_lowercase();
    if lower.ends_with(".bmp") {
        return true;
    }
    if lower.starts_with("vc_redist") && lower.ends_with(".exe") {
        return true;
    }
    if lower.starts_with("vcredist") && lower.ends_with(".exe") {
        return true;
    }
    false
}

/// Extract a Windows NSIS installer EXE into `dest`.
///
/// Strategy:
/// 1. Extract the EXE into a fresh temp directory using 7z (`7z x -o<tmp> -y <archive>`).
/// 2. Walk the temp dir's top-level entries, filter out NSIS-only artifacts
///    (see `is_nsis_artifact`), and move the survivors into `dest`.
/// 3. Clean up the temp directory (handled by `tempfile::TempDir`'s Drop impl).
fn extract_nsis(
    out: &crate::output::Output,
    archive: &Path,
    dest: &Path,
    seven_zip: &Path,
) -> Result<()> {
    let temp = tempfile::Builder::new()
        .prefix("bn-loader-install-")
        .tempdir()
        .context("Failed to create temporary directory for NSIS extraction")?;
    let temp_path = temp.path();

    out.status(&format!(
        "Extracting {} via 7z into {} (will filter NSIS artifacts before moving to {})",
        archive.display(),
        temp_path.display(),
        dest.display()
    ));

    let status = Command::new(seven_zip)
        .arg("x")
        .arg(archive)
        .arg(format!("-o{}", temp_path.display()))
        .arg("-y")
        .status()
        .with_context(|| format!("Failed to invoke 7z at {}", seven_zip.display()))?;

    if !status.success() {
        bail!(
            "7z exited with status {}: {} extraction failed",
            status,
            archive.display()
        );
    }

    fs::create_dir_all(dest)
        .with_context(|| format!("Failed to create destination directory {}", dest.display()))?;

    let mut moved = 0usize;
    let mut skipped: Vec<String> = Vec::new();

    for entry in fs::read_dir(temp_path)
        .with_context(|| format!("Failed to read temp dir {}", temp_path.display()))?
    {
        let entry = entry.context("Failed to read temp dir entry")?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if is_nsis_artifact(&name_str) {
            skipped.push(name_str.into_owned());
            continue;
        }

        let src_path = entry.path();
        let dst_path = dest.join(&name);

        // If a same-named entry exists at dest (e.g., --force overlay), remove it first
        // so rename-or-copy semantics are predictable.
        if dst_path.exists() {
            if dst_path.is_dir() {
                fs::remove_dir_all(&dst_path)
                    .with_context(|| format!("Failed to remove existing {}", dst_path.display()))?;
            } else {
                fs::remove_file(&dst_path)
                    .with_context(|| format!("Failed to remove existing {}", dst_path.display()))?;
            }
        }

        // Try a same-volume rename first (fast). Fall back to recursive copy if it
        // crosses filesystems (typical when temp is on a different volume than dest).
        match fs::rename(&src_path, &dst_path) {
            Ok(()) => {}
            Err(_) => {
                if src_path.is_dir() {
                    crate::fs_util::copy_dir_recursive(&src_path, &dst_path)?;
                } else {
                    if let Some(parent) = dst_path.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("Failed to create parent directory {}", parent.display())
                        })?;
                    }
                    fs::copy(&src_path, &dst_path).with_context(|| {
                        format!(
                            "Failed to copy {} to {}",
                            src_path.display(),
                            dst_path.display()
                        )
                    })?;
                }
            }
        }
        moved += 1;
    }

    out.status(&format!(
        "Moved {} top-level entries; skipped {} installer artifact(s){}",
        moved,
        skipped.len(),
        if skipped.is_empty() {
            String::new()
        } else {
            format!(": {}", skipped.join(", "))
        }
    ));

    // `temp` goes out of scope here and TempDir cleans up the directory.
    drop(temp);
    Ok(())
}

/// Plan for registering a new profile after extraction.
struct RegistrationPlan {
    name: String,
    config_dir: PathBuf,
    /// If the auto-bump path was taken (both name and config_dir derived) and the
    /// originally-derived name was already in use, this is the original derived name —
    /// used only for the "registered as X instead" message.
    original_name: Option<String>,
}

/// Compute what name and config dir should be used for the new profile entry,
/// honoring the user's explicit overrides and applying defaults / collision avoidance
/// where appropriate. Does NOT write anything — purely a plan.
///
/// The only uniqueness constraint is the profile name. install_dir and config_dir
/// can be shared across multiple profiles (intentional use cases include two BN
/// installations sharing a config_dir for plugins/settings parity).
fn plan_profile_registration(
    config: &Config,
    options: &InstallOptions,
) -> Result<RegistrationPlan> {
    let derived_name = derive_default_name(options.dest)?;
    let name_was_explicit = options.name.is_some();
    let cfg_was_explicit = options.config_dir.is_some();

    let initial_name = options.name.map(str::to_string).unwrap_or(derived_name);
    let initial_cfg = match options.config_dir {
        Some(p) => p.to_path_buf(),
        None => default_config_dir_for(&initial_name)?,
    };

    if name_was_explicit || cfg_was_explicit {
        // Strict mode: never auto-modify what the user explicitly typed.
        // Profile name is the only uniqueness constraint — config_dir reuse across
        // profiles is intentionally allowed.
        if config.profiles.contains_key(&initial_name) {
            bail!(
                "Profile '{}' already exists in the config. Pick a different --name or remove the existing entry.",
                initial_name
            );
        }
        return Ok(RegistrationPlan {
            name: initial_name,
            config_dir: initial_cfg,
            original_name: None,
        });
    }

    // Both derived: auto-bump only on profile-name collision.
    let original = initial_name.clone();
    for i in 1..=999 {
        let candidate_name = if i == 1 {
            initial_name.clone()
        } else {
            format!("{original}-{i}")
        };
        if !config.profiles.contains_key(&candidate_name) {
            let candidate_cfg = default_config_dir_for(&candidate_name)?;
            return Ok(RegistrationPlan {
                name: candidate_name.clone(),
                config_dir: candidate_cfg,
                original_name: if candidate_name == original {
                    None
                } else {
                    Some(original)
                },
            });
        }
    }
    bail!(
        "Could not find an unused profile name starting with '{}' after 999 attempts. Pass --name explicitly.",
        original
    )
}

/// Derive a default profile name from the basename of `dest`.
///
/// Sanitizes by replacing any character that isn't alphanumeric, hyphen, or underscore
/// with `_`. Errors if the sanitized result is empty or otherwise invalid per
/// `crate::config::is_valid_profile_name`.
fn derive_default_name(dest: &Path) -> Result<String> {
    let basename = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            anyhow!(
                "Cannot derive a default profile name from --dest path '{}': no usable basename. Pass --name explicitly.",
                dest.display()
            )
        })?;

    let sanitized: String = basename
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if !crate::config::is_valid_profile_name(&sanitized) {
        bail!(
            "Cannot derive a valid profile name from --dest basename '{}'. Pass --name explicitly.",
            basename
        );
    }

    Ok(sanitized)
}

/// Compute the default config dir for a given profile name.
fn default_config_dir_for(name: &str) -> Result<PathBuf> {
    crate::config::default_profiles_dir()
        .ok_or_else(|| {
            anyhow!(
                "Cannot determine home directory for default --config-dir. Pass --config-dir explicitly."
            )
        })
        .map(|d| d.join(name))
}
