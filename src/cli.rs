use crate::config::{Config, Profile};
use anyhow::{Context, Result};
use std::io::{self, Write};
use std::process;

/// Print error to stderr and exit non-zero on Err; exit 0 on Ok. Never returns.
///
/// When `debug` is true, prints the full anyhow cause chain on indented `caused by:`
/// lines after the top message; otherwise prints just the top message.
pub(crate) fn report_and_exit(result: Result<()>, debug: bool) -> ! {
    match result {
        Ok(()) => process::exit(0),
        Err(e) => {
            eprintln!("Error: {e}");
            if debug {
                for cause in e.chain().skip(1) {
                    eprintln!("  caused by: {cause}");
                }
            }
            process::exit(1);
        }
    }
}

/// Look up a profile by name. Returns Err with a helpful message including a hint
/// to use `--list` if the profile is missing.
pub(crate) fn resolve_profile<'a>(config: &'a Config, name: &str) -> Result<&'a Profile> {
    config.profiles.get(name).ok_or_else(|| {
        anyhow::anyhow!("Profile '{name}' not found.\nUse --list to see available profiles.")
    })
}

/// Prompt the user with a yes/no question. Default answer is "no" when `default_no`
/// is true (shown as `[y/N]`), otherwise "yes" (shown as `[Y/n]`). Returns Ok(true)
/// when the user answers yes.
pub(crate) fn confirm_prompt(message: &str, default_no: bool) -> Result<bool> {
    let suffix = if default_no { "[y/N]" } else { "[Y/n]" };
    print!("{message} {suffix} ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;

    let trimmed = input.trim();
    Ok(if default_no {
        trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes")
    } else {
        !(trimmed.eq_ignore_ascii_case("n") || trimmed.eq_ignore_ascii_case("no"))
    })
}
