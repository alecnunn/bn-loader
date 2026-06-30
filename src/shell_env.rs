use crate::config::{ENV_VAR_NAME, Profile};
use anyhow::{Result, anyhow};
use std::env;

/// PATH-list separator for the host OS (not the path-component separator).
#[cfg(windows)]
const PATH_SEP: char = ';';
#[cfg(not(windows))]
const PATH_SEP: char = ':';

/// Name of the OS shared-library search-path variable on the host OS.
#[cfg(windows)]
const LIB_PATH_VAR: &str = "PATH";
#[cfg(target_os = "macos")]
const LIB_PATH_VAR: &str = "DYLD_LIBRARY_PATH";
#[cfg(all(unix, not(target_os = "macos")))]
const LIB_PATH_VAR: &str = "LD_LIBRARY_PATH";

/// Shell dialects we can emit assignments for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShellDialect {
    Posix,
    Fish,
    PowerShell,
    Cmd,
}

/// `--shell` choices. `bash`/`zsh`/`sh` all map to the POSIX emitter.
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum ShellArg {
    Bash,
    Zsh,
    Sh,
    Fish,
    Powershell,
    Cmd,
}

impl ShellArg {
    fn dialect(self) -> ShellDialect {
        match self {
            ShellArg::Bash | ShellArg::Zsh | ShellArg::Sh => ShellDialect::Posix,
            ShellArg::Fish => ShellDialect::Fish,
            ShellArg::Powershell => ShellDialect::PowerShell,
            ShellArg::Cmd => ShellDialect::Cmd,
        }
    }
}

/// Map a `$SHELL` value to a dialect (Unix only).
#[cfg(not(windows))]
fn dialect_from_shell_path(shell: &str) -> ShellDialect {
    let base = std::path::Path::new(shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if base.contains("fish") {
        ShellDialect::Fish
    } else {
        ShellDialect::Posix
    }
}

/// Auto-detect the output dialect: PowerShell on Windows; otherwise from `$SHELL`.
fn detect_dialect() -> ShellDialect {
    #[cfg(windows)]
    {
        ShellDialect::PowerShell
    }
    #[cfg(not(windows))]
    {
        env::var("SHELL")
            .map(|s| dialect_from_shell_path(&s))
            .unwrap_or(ShellDialect::Posix)
    }
}

/// Prepend `dir` to `current` using the host PATH separator. Empty/None current
/// yields `dir` alone (no trailing separator).
fn prepend_value(dir: &str, current: Option<String>) -> String {
    match current {
        Some(c) if !c.is_empty() => format!("{dir}{PATH_SEP}{c}"),
        _ => dir.to_string(),
    }
}

/// Escape for a POSIX double-quoted string.
fn escape_double(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '"' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Escape for a fish double-quoted string (backtick is not special in fish).
fn escape_fish(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '"' | '$') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Escape for a PowerShell single-quoted literal.
fn escape_single_ps(value: &str) -> String {
    value.replace('\'', "''")
}

fn format_assignment(dialect: ShellDialect, name: &str, value: &str) -> String {
    match dialect {
        ShellDialect::Posix => format!("export {name}=\"{}\";", escape_double(value)),
        ShellDialect::Fish => format!("set -gx {name} \"{}\";", escape_fish(value)),
        ShellDialect::PowerShell => format!("$env:{name} = '{}'", escape_single_ps(value)),
        ShellDialect::Cmd => format!("set \"{name}={value}\""),
    }
}

/// The (name, value) pairs to emit, in display order.
fn build_assignments(profile: &Profile) -> Vec<(String, String)> {
    let python_dir = profile
        .install_dir
        .join("python")
        .to_string_lossy()
        .into_owned();
    let install = profile.install_dir.to_string_lossy().into_owned();
    let config = profile.config_dir.to_string_lossy().into_owned();

    vec![
        (
            "PYTHONPATH".to_string(),
            prepend_value(&python_dir, env::var("PYTHONPATH").ok()),
        ),
        (ENV_VAR_NAME.to_string(), config),
        (
            LIB_PATH_VAR.to_string(),
            prepend_value(&install, env::var(LIB_PATH_VAR).ok()),
        ),
    ]
}

/// Validate the profile dirs. Hard error if the install/python dir is missing;
/// a missing config dir is only a stderr warning (BN creates it).
fn validate(profile: &Profile) -> Result<()> {
    if !profile.install_dir.exists() {
        return Err(anyhow!(
            "Install directory does not exist: {}",
            profile.install_dir.display()
        ));
    }
    let python_dir = profile.install_dir.join("python");
    if !python_dir.exists() {
        return Err(anyhow!(
            "Binary Ninja Python API not found at: {}\n\
             The profile's install_dir may be incomplete.",
            python_dir.display()
        ));
    }
    if !profile.config_dir.exists() {
        eprintln!(
            "Warning: config directory does not exist: {}",
            profile.config_dir.display()
        );
    }
    Ok(())
}

/// Build the full eval-able output (one assignment per line). Pure except for
/// the stderr warning in `validate`.
fn render(profile: &Profile, dialect: ShellDialect) -> Result<String> {
    validate(profile)?;
    let lines: Vec<String> = build_assignments(profile)
        .iter()
        .map(|(name, value)| format_assignment(dialect, name, value))
        .collect();
    Ok(lines.join("\n"))
}

/// Print shell commands that set up the environment for headless BN Python use.
pub(crate) fn run_env(profile: &Profile, shell: Option<ShellArg>) -> Result<()> {
    let dialect = shell.map(ShellArg::dialect).unwrap_or_else(detect_dialect);
    println!("{}", render(profile, dialect)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_profile() -> (tempfile::TempDir, Profile) {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        std::fs::create_dir_all(install.join("python")).unwrap();
        let config = dir.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        let profile = Profile {
            install_dir: install,
            config_dir: config,
            executable: "binaryninja".to_string(),
            debug: false,
        };
        (dir, profile)
    }

    #[test]
    fn prepend_to_empty_returns_dir_only() {
        assert_eq!(prepend_value("/opt/bn", None), "/opt/bn");
        assert_eq!(prepend_value("/opt/bn", Some(String::new())), "/opt/bn");
    }

    #[test]
    fn prepend_to_nonempty_uses_separator() {
        assert_eq!(
            prepend_value("/opt/bn", Some("/usr/lib".to_string())),
            format!("/opt/bn{PATH_SEP}/usr/lib")
        );
    }

    #[test]
    fn posix_escapes_special_chars() {
        assert_eq!(
            escape_double(r#"a "b" $c \d `e`"#),
            r#"a \"b\" \$c \\d \`e\`"#
        );
    }

    #[test]
    fn fish_escapes_without_backtick() {
        assert_eq!(escape_fish(r#"a "b" $c \d `e`"#), r#"a \"b\" \$c \\d `e`"#);
    }

    #[test]
    fn powershell_doubles_single_quotes() {
        assert_eq!(escape_single_ps("it's a 'test'"), "it''s a ''test''");
    }

    #[test]
    fn format_per_dialect() {
        assert_eq!(
            format_assignment(ShellDialect::Posix, "FOO", "/a b"),
            r#"export FOO="/a b";"#
        );
        assert_eq!(
            format_assignment(ShellDialect::Fish, "FOO", "/a b"),
            r#"set -gx FOO "/a b";"#
        );
        assert_eq!(
            format_assignment(ShellDialect::PowerShell, "FOO", "/a b"),
            "$env:FOO = '/a b'"
        );
        assert_eq!(
            format_assignment(ShellDialect::Cmd, "FOO", "C:\\a b"),
            "set \"FOO=C:\\a b\""
        );
    }

    #[test]
    fn render_emits_three_known_vars() {
        let (_dir, profile) = temp_profile();
        let out = render(&profile, ShellDialect::Posix).unwrap();
        assert_eq!(out.lines().count(), 3);
        assert!(out.contains("PYTHONPATH"));
        assert!(out.contains(ENV_VAR_NAME));
        assert!(out.contains(LIB_PATH_VAR));
    }

    #[test]
    fn render_errors_when_python_dir_missing() {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("install");
        std::fs::create_dir_all(&install).unwrap(); // no python/ subdir
        let profile = Profile {
            install_dir: install,
            config_dir: dir.path().join("config"),
            executable: "binaryninja".to_string(),
            debug: false,
        };
        assert!(render(&profile, ShellDialect::Posix).is_err());
    }

    #[test]
    fn run_env_smoke_posix() {
        let (_dir, profile) = temp_profile();
        assert!(run_env(&profile, Some(ShellArg::Bash)).is_ok());
    }

    #[test]
    fn run_env_smoke_autodetect() {
        let (_dir, profile) = temp_profile();
        assert!(run_env(&profile, None).is_ok());
    }

    #[cfg(not(windows))]
    #[test]
    fn detect_maps_fish_and_posix() {
        assert_eq!(dialect_from_shell_path("/usr/bin/fish"), ShellDialect::Fish);
        assert_eq!(dialect_from_shell_path("/bin/bash"), ShellDialect::Posix);
        assert_eq!(dialect_from_shell_path("/bin/zsh"), ShellDialect::Posix);
    }

    #[cfg(windows)]
    #[test]
    fn detect_is_powershell_on_windows() {
        assert_eq!(detect_dialect(), ShellDialect::PowerShell);
    }
}
