mod cli;
mod completions;
mod config;
mod diff;
mod doctor;
mod fs_util;
mod init;
mod install;
mod launch;
mod output;
mod plugins;
mod remove;
mod shell_env;
mod sync;
mod update;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::CompleteEnv;
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use cli::{report_and_exit, resolve_profile};
use config::{CONFIG_FILE_NAME, Config, find_config_file, load_config};
use diff::diff_profiles;
use doctor::run_doctor;
use init::{InitOptions, run_init};
use install::{InstallOptions, run_install};
use launch::{LaunchOptions, launch_profile};
use plugins::{list_plugins, print_plugins};
use remove::{RemoveOptions, run_remove};
use shell_env::{ShellArg, run_env};
use std::env;
use std::path::{Path, PathBuf};
use std::process;
use sync::{SyncOptions, run_sync};
use update::{UpdateOptions, run_update};

/// Get profile names from config for shell completion
fn profile_completer() -> Vec<CompletionCandidate> {
    find_config_file(None)
        .and_then(|p| load_config(&p).ok())
        .map(|c| c.profiles.keys().map(CompletionCandidate::new).collect())
        .unwrap_or_default()
}

/// CLI flag wins over config; config wins over the default (Auto).
fn effective_color(
    cli_color: Option<config::ColorMode>,
    config_color: config::ColorMode,
) -> config::ColorMode {
    cli_color.unwrap_or(config_color)
}

#[derive(Parser)]
#[command(name = "bn-loader", version, about = "Binary Ninja profile launcher")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Use a specific config file
    #[arg(long, short = 'c', global = true)]
    config: Option<PathBuf>,

    /// Color output mode (overrides [global] color in config)
    #[arg(long, value_enum, global = true)]
    color: Option<config::ColorMode>,

    /// List available profiles
    #[arg(long, short = 'l')]
    list: bool,

    /// Profile name to launch
    #[arg(conflicts_with = "list", add = ArgValueCandidates::new(profile_completer))]
    profile: Option<String>,

    /// Enable debug logging (redirects output to log file)
    #[arg(long)]
    debug: bool,

    /// Write debug output to file
    #[arg(long)]
    log_file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Profile management commands (init, install, sync, diff, plugins, list)
    Profile {
        #[command(subcommand)]
        subcommand: ProfileCommand,
    },

    /// Validate the whole config (read-only)
    Doctor,

    /// Print shell commands to set up env vars for headless Binary Ninja Python use
    Env {
        /// Profile name (defaults to global.default_profile)
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        profile: Option<String>,

        /// Output shell dialect (auto-detected from $SHELL / OS if omitted)
        #[arg(long, value_enum)]
        shell: Option<ShellArg>,
    },

    /// Generate shell completions
    Completions {
        /// Shell type
        #[arg(value_enum)]
        shell: ShellType,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// List available profiles
    List,

    /// Create a new profile from a template
    Init {
        name: String,
        #[arg(long, add = ArgValueCandidates::new(profile_completer))]
        template: String,
        #[arg(long)]
        config_dir: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Sync config between profiles
    Sync {
        #[arg(long, add = ArgValueCandidates::new(profile_completer))]
        from: String,
        #[arg(long, add = ArgValueCandidates::new(profile_completer))]
        to: Option<String>,
        #[arg(long, action = clap::ArgAction::Append)]
        exclude: Vec<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, short = 'y')]
        yes: bool,
        #[arg(long)]
        force: bool,
    },

    /// List plugins for a profile
    Plugins {
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        profile: String,
    },

    /// Compare two profiles
    Diff {
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        profile1: String,
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        profile2: String,
    },

    /// Install Binary Ninja from an archive and register a profile
    Install {
        archive: PathBuf,
        #[arg(long)]
        dest: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        config_dir: Option<PathBuf>,
        #[arg(long, conflicts_with_all = ["name", "config_dir"])]
        no_register: bool,
        #[arg(long, short = 'f')]
        force: bool,
        #[arg(long, short = 'y')]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        seven_zip: Option<PathBuf>,
    },

    /// Remove a profile (deregister from config; --purge to also delete on-disk dirs)
    Remove {
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        name: String,
        /// Also delete the profile's config_dir on disk
        #[arg(long)]
        purge: bool,
        /// With --purge, also delete the profile's install_dir
        #[arg(long, short = 'f')]
        force: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
        /// Print what would be removed and exit
        #[arg(long)]
        dry_run: bool,
    },

    /// Re-extract Binary Ninja into an existing profile's install_dir
    Update {
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        name: String,
        archive: PathBuf,
        /// Skip interactive confirmation prompts
        #[arg(long, short = 'y')]
        yes: bool,
        /// Print plan and exit without making changes
        #[arg(long)]
        dry_run: bool,
        /// Override 7z executable path (for NSIS .exe extraction)
        #[arg(long)]
        seven_zip: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
pub enum ShellType {
    Bash,
    Powershell,
    Zsh,
    Fish,
}

fn list_profiles_cmd(out: &output::Output, config: &Config) {
    out.heading("Available profiles:");
    for (name, profile) in &config.profiles {
        out.out(&format!("  {} -> {}", name, profile.install_dir.display()));
    }
}

fn load_config_or_exit(custom_config: Option<&Path>) -> (PathBuf, Config) {
    let config_path = if let Some(p) = find_config_file(custom_config.and_then(|p| p.to_str())) {
        p
    } else {
        eprintln!("Error: No config file found.");
        eprintln!("Searched locations:");
        // Show preferred location first
        if let Some(home) = env::var("HOME")
            .ok()
            .or_else(|| env::var("USERPROFILE").ok())
        {
            eprintln!(
                "  - {}",
                PathBuf::from(home)
                    .join(".config")
                    .join(CONFIG_FILE_NAME)
                    .display()
            );
        }
        if let Ok(exe_path) = env::current_exe()
            && let Some(exe_dir) = exe_path.parent()
        {
            eprintln!("  - {}", exe_dir.join(CONFIG_FILE_NAME).display());
        }
        process::exit(1);
    };

    let config = match load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    };

    (config_path, config)
}

fn main() {
    // Handle dynamic shell completions (intercepts COMPLETE=<shell> env var)
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    // Handle completions subcommand (prints registration instructions)
    if let Some(Commands::Completions { shell }) = &cli.command {
        completions::print_instructions(shell);
        return;
    }

    // All other commands need config
    let (config_path, mut config) = load_config_or_exit(cli.config.as_deref());
    config.global.color = effective_color(cli.color, config.global.color);

    if cli.list {
        let out = output::Output::new(config.global.color);
        list_profiles_cmd(&out, &config);
        return;
    }

    match cli.command {
        Some(Commands::Profile { subcommand }) => {
            dispatch_profile(&config, &config_path, subcommand);
        }
        Some(Commands::Doctor) => {
            let out = output::Output::new(config.global.color);
            match run_doctor(&out, &config) {
                Ok(code) => process::exit(code),
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
        Some(Commands::Env { profile, shell }) => {
            let name = match profile.or_else(|| config.global.default_profile.clone()) {
                Some(n) => n,
                None => {
                    eprintln!("Error: No profile specified.");
                    eprintln!("Usage: bn-loader env <profile>");
                    process::exit(1);
                }
            };
            let result =
                resolve_profile(&config, &name).and_then(|profile| run_env(profile, shell));
            report_and_exit(result, config.global.debug);
        }
        Some(Commands::Completions { .. }) => {
            // Already handled above
            unreachable!()
        }
        None => {
            // Launch profile mode
            let name = match cli.profile {
                Some(n) => n,
                None => {
                    if let Some(default) = &config.global.default_profile {
                        default.clone()
                    } else {
                        eprintln!("Error: No profile specified.");
                        eprintln!("Use --list to see available profiles, or --help for usage.");
                        eprintln!(
                            "Tip: Set global.default_profile in config to launch without arguments."
                        );
                        process::exit(1);
                    }
                }
            };

            let use_debug = cli.debug || config.global.debug;
            let log_file = cli.log_file.clone();
            let out = output::Output::new(config.global.color);
            let result = resolve_profile(&config, &name).and_then(|profile| {
                let options = LaunchOptions {
                    debug: use_debug,
                    log_file: log_file.as_ref(),
                };
                launch_profile(&out, &name, profile, &options)
            });
            report_and_exit(result, use_debug);
        }
    }
}

fn dispatch_profile(config: &Config, config_path: &Path, subcommand: ProfileCommand) -> ! {
    let out = output::Output::new(config.global.color);
    match subcommand {
        ProfileCommand::List => {
            list_profiles_cmd(&out, config);
            process::exit(0);
        }
        ProfileCommand::Init {
            name,
            template,
            config_dir,
            dry_run,
            yes,
        } => {
            let expanded_config_dir = if config_dir.is_relative() {
                env::current_dir()
                    .map(|cwd| cwd.join(&config_dir))
                    .unwrap_or(config_dir)
            } else {
                config_dir
            };
            let options = InitOptions {
                name: &name,
                template: &template,
                config_dir: &expanded_config_dir,
                dry_run,
                yes,
            };
            report_and_exit(
                run_init(&out, config, config_path, &options),
                config.global.debug,
            );
        }
        ProfileCommand::Sync {
            from,
            to,
            exclude,
            dry_run,
            yes,
            force,
        } => {
            let extra_exclusions: Vec<&str> =
                exclude.iter().map(std::string::String::as_str).collect();
            let options = SyncOptions {
                from: &from,
                to: to.as_deref(),
                extra_exclusions,
                dry_run,
                yes,
                force,
                backup_retention: config.global.backup_retention,
            };
            report_and_exit(run_sync(&out, config, &options), config.global.debug);
        }
        ProfileCommand::Plugins { profile } => {
            let result = resolve_profile(config, &profile).and_then(|prof| {
                let plugins = list_plugins(prof)?;
                print_plugins(&out, &profile, &plugins);
                Ok(())
            });
            report_and_exit(result, config.global.debug);
        }
        ProfileCommand::Diff { profile1, profile2 } => {
            let result = resolve_profile(config, &profile1).and_then(|prof1| {
                let prof2 = resolve_profile(config, &profile2)?;
                diff_profiles(config, &profile1, prof1, &profile2, prof2)
            });
            report_and_exit(result, config.global.debug);
        }
        ProfileCommand::Install {
            archive,
            dest,
            name,
            config_dir,
            no_register,
            force,
            yes,
            dry_run,
            seven_zip,
        } => {
            let options = InstallOptions {
                archive: &archive,
                dest: &dest,
                name: name.as_deref(),
                config_dir: config_dir.as_deref(),
                no_register,
                force,
                yes,
                dry_run,
                seven_zip: seven_zip.as_deref(),
                config_path,
            };
            report_and_exit(run_install(&out, config, &options), config.global.debug);
        }
        ProfileCommand::Remove {
            name,
            purge,
            force,
            yes,
            dry_run,
        } => {
            let options = RemoveOptions {
                name: &name,
                purge,
                force,
                yes,
                dry_run,
            };
            report_and_exit(
                run_remove(&out, config, config_path, &options),
                config.global.debug,
            );
        }
        ProfileCommand::Update {
            name,
            archive,
            yes,
            dry_run,
            seven_zip,
        } => {
            let options = UpdateOptions {
                name: &name,
                archive: &archive,
                yes,
                dry_run,
                seven_zip: seven_zip.as_deref(),
            };
            report_and_exit(
                run_update(&out, config, config_path, &options),
                config.global.debug,
            );
        }
    }
}
