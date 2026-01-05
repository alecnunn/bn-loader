mod colors;
mod completions;
mod config;
mod diff;
mod init;
mod launch;
mod plugins;
mod sync;
mod update;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::CompleteEnv;
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use config::{CONFIG_FILE_NAME, Config, find_config_file, load_config};
use diff::diff_profiles;
use init::{InitOptions, run_init};
use launch::{LaunchOptions, launch_profile};
use plugins::{list_plugins, print_plugins};
use std::env;
use std::path::{Path, PathBuf};
use std::process;
use sync::{SyncOptions, run_sync};

/// Get profile names from config for shell completion
fn profile_completer() -> Vec<CompletionCandidate> {
    find_config_file(None)
        .and_then(|p| load_config(&p).ok())
        .map(|c| c.profiles.keys().map(CompletionCandidate::new).collect())
        .unwrap_or_default()
}

#[derive(Parser)]
#[command(name = "bn-loader", version, about = "Binary Ninja profile launcher")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Use a specific config file
    #[arg(long, short = 'c', global = true)]
    config: Option<PathBuf>,

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

    /// Check for updates and exit
    #[arg(long)]
    check_update: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new profile from a template
    Init {
        /// Name for the new profile
        name: String,

        /// Source profile for license and `install_dir`
        #[arg(long, add = ArgValueCandidates::new(profile_completer))]
        template: String,

        /// Directory for new profile's config
        #[arg(long)]
        config_dir: PathBuf,
    },

    /// Sync config between profiles
    Sync {
        /// Source profile to sync from
        #[arg(long, add = ArgValueCandidates::new(profile_completer))]
        from: String,

        /// Target profile (default: all other profiles)
        #[arg(long, add = ArgValueCandidates::new(profile_completer))]
        to: Option<String>,

        /// Additional exclusion pattern (can be repeated)
        #[arg(long, action = clap::ArgAction::Append)]
        exclude: Vec<String>,

        /// Show what would be synced without changes
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// List plugins for a profile
    Plugins {
        /// Profile name
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        profile: String,
    },

    /// Compare two profiles
    Diff {
        /// First profile
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        profile1: String,

        /// Second profile
        #[arg(add = ArgValueCandidates::new(profile_completer))]
        profile2: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell type
        #[arg(value_enum)]
        shell: ShellType,
    },
}

#[derive(Clone, ValueEnum)]
pub enum ShellType {
    Bash,
    Powershell,
    Zsh,
    Fish,
}

fn list_profiles_cmd(config: &Config) {
    println!("Available profiles:");
    for (name, profile) in &config.profiles {
        println!("  {} -> {}", name, profile.install_dir.display());
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

    // Manual update check (doesn't require config)
    if cli.check_update {
        println!("Checking for updates...");
        println!("Current version: {}", env!("CARGO_PKG_VERSION"));
        match update::check_for_updates_forced() {
            Some(info) => {
                println!("Update available: v{} -> v{}", info.current, info.latest);
                println!("Download: {}", info.url);
            }
            None => {
                println!("You're on the latest version.");
            }
        }
        return;
    }

    // All other commands need config
    let (config_path, config) = load_config_or_exit(cli.config.as_deref());

    // Check for updates (non-blocking, silent on error)
    if config.global.check_updates
        && let Some(update_info) = update::check_for_updates()
    {
        update::print_update_notice(&update_info);
    }

    if cli.list {
        list_profiles_cmd(&config);
        return;
    }

    match cli.command {
        Some(Commands::Init {
            name,
            template,
            config_dir,
        }) => {
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
            };
            if let Err(e) = run_init(&config, &config_path, &options) {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }

        Some(Commands::Sync {
            from,
            to,
            exclude,
            dry_run,
            yes,
        }) => {
            let extra_exclusions: Vec<&str> =
                exclude.iter().map(std::string::String::as_str).collect();
            let options = SyncOptions {
                from: &from,
                to: to.as_deref(),
                extra_exclusions,
                dry_run,
                yes,
                backup_retention: config.global.backup_retention,
            };
            if let Err(e) = run_sync(&config, &options) {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }

        Some(Commands::Plugins { profile }) => {
            let prof = if let Some(p) = config.profiles.get(&profile) {
                p
            } else {
                eprintln!("Error: Profile '{profile}' not found.");
                process::exit(1);
            };
            match list_plugins(prof) {
                Ok(plugins) => print_plugins(&profile, &plugins),
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        Some(Commands::Diff { profile1, profile2 }) => {
            let prof1 = if let Some(p) = config.profiles.get(&profile1) {
                p
            } else {
                eprintln!("Error: Profile '{profile1}' not found.");
                process::exit(1);
            };
            let prof2 = if let Some(p) = config.profiles.get(&profile2) {
                p
            } else {
                eprintln!("Error: Profile '{profile2}' not found.");
                process::exit(1);
            };
            if let Err(e) = diff_profiles(&profile1, prof1, &profile2, prof2) {
                eprintln!("Error: {e}");
                process::exit(1);
            }
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
                    // Try default profile from global config
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

            let profile = if let Some(p) = config.profiles.get(&name) {
                p
            } else {
                eprintln!("Error: Profile '{name}' not found.");
                eprintln!("Use --list to see available profiles.");
                process::exit(1);
            };

            // Combine CLI debug flag with global debug setting
            let use_debug = cli.debug || config.global.debug;

            let options = LaunchOptions {
                debug: use_debug,
                log_file: cli.log_file.as_ref(),
            };
            if let Err(e) = launch_profile(&name, profile, &options) {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        }
    }
}
