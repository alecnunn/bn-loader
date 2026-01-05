# bn-loader

A profile launcher for [Binary Ninja](https://binary.ninja) that lets you manage multiple configurations. If you work with different Binary Ninja setups (personal vs commercial licenses, different plugin sets, separate configs for development), this tool makes switching between them painless.

## Installation

### From crates.io

```bash
cargo install bn-loader
```

### From source

```bash
git clone https://github.com/alecnunn/bn-loader
cd bn-loader
cargo install --path .
```

### Pre-built binaries

Download from the [releases page](https://github.com/alecnunn/bn-loader/releases) for Windows, Linux, and macOS.

## Quick Start

1. Copy `example.config.toml` to `~/.config/bn-loader.toml`
2. Edit the file to define your profiles
3. Run `bn-loader <profile-name>` to launch

## Configuration

bn-loader looks for its config file in these locations (in order):
1. `~/.config/bn-loader.toml` (recommended)
2. Next to the executable

A basic config looks like this:

```toml
[global]
default_profile = "personal"
check_updates = true

[profiles.personal]
install_dir = "C:\\Program Files\\Binary Ninja Personal"
config_dir = "C:\\Users\\You\\AppData\\Roaming\\Binary Ninja Personal"

[profiles.commercial]
install_dir = "C:\\Program Files\\Binary Ninja"
config_dir = "C:\\Users\\You\\AppData\\Roaming\\Binary Ninja"
```

See `example.config.toml` for a full example with Linux and macOS paths.

## Usage

```bash
# Launch a profile
bn-loader personal

# Launch default profile (if configured)
bn-loader

# List available profiles
bn-loader --list

# Launch with debug output
bn-loader personal --debug

# Check for updates
bn-loader --check-update
```

### Commands

**init** - Create a new profile from an existing one:
```bash
bn-loader init dev --template personal --config-dir ~/bn-dev-config
```
This copies the license and install directory from the template but gives the new profile its own config directory.

**sync** - Copy settings between profiles:
```bash
# Sync from personal to all other profiles
bn-loader sync --from personal

# Sync to a specific profile
bn-loader sync --from personal --to commercial

# Preview changes without applying
bn-loader sync --from personal --dry-run
```
License files and other sensitive data are excluded by default. You can add more exclusions in the `[sync]` section of your config.

**plugins** - List installed plugins for a profile:
```bash
bn-loader plugins personal
```

**diff** - Compare two profiles:
```bash
bn-loader diff personal commercial
```

**completions** - Set up shell completions:
```bash
bn-loader completions bash
bn-loader completions zsh
bn-loader completions fish
bn-loader completions powershell
```

## Shell Completions

bn-loader supports tab completion for profile names and commands. Run `bn-loader completions <shell>` for setup instructions specific to your shell.

## Global Options

These go in the `[global]` section:

| Option | Default | Description |
|--------|---------|-------------|
| `default_profile` | none | Profile to launch when no argument given |
| `color` | `"auto"` | Color output: `"auto"`, `"always"`, `"never"` |
| `check_updates` | `true` | Check GitHub for new releases on launch |
| `backup_retention` | `5` | Number of sync backups to keep (0 = unlimited) |
| `debug` | `false` | Enable debug logging globally |

## Profile Options

| Option | Required | Description |
|--------|----------|-------------|
| `install_dir` | yes | Path to Binary Ninja installation |
| `config_dir` | yes | Path to user data directory |
| `executable` | no | Binary name (defaults to `binaryninja.exe` on Windows, `binaryninja` elsewhere) |
| `debug` | no | Enable debug logging for this profile |

## Sync Configuration

The `sync` command copies settings, plugins, and other configuration between profiles.

### What Gets Synced

These items are synced from the source profile (if they exist):

- `plugins/` - Manual plugin installations
- `repositories/` - Plugin manager data
- `signatures/` - Custom signatures
- `themes/` - UI themes
- `snippets/` - Code snippets
- `types/` - Type libraries
- `settings.json` - Binary Ninja settings
- `startup.py` - Startup script
- `keybindings.json` - Key bindings

### Exclusions

These patterns are always excluded to protect license files:
`license.dat`, `license.txt`, `user.id`, `keychain/`, `__pycache__/`, `*.pyc`

Add your own exclusions in your config (these merge with the defaults):

```toml
[sync]
exclusions = ["my-custom-dir/", "*.tmp"]
```

Or use the `--exclude` flag for one-off exclusions:

```bash
bn-loader sync --from personal --exclude "temp/"
```

## License

BSD-3-Clause. See [LICENSE](LICENSE) for details.
