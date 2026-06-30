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

# Force color output (also: "always", "never", or default "auto")
bn-loader --color always profile diff personal commercial
```

### Commands

Profile management commands are grouped under `bn-loader profile`. The flat launch shortcut (`bn-loader <name>`) and `--list` remain at the top level.

**profile init** - Create a new profile from an existing one:
```bash
bn-loader profile init dev --template personal --config-dir ~/bn-dev-config
```
This copies the license and install directory from the template but gives the new profile its own config directory.

**profile install** - Install Binary Ninja from an archive and register a profile:

```bash
# Linux: extract a .zip bundle (registers profile 'stable' at ~/.config/bn-loader/profiles/stable)
bn-loader profile install ~/Downloads/binaryninja_linux_X.Y.Z_personal.zip --dest /opt/binaryninja/stable

# Windows: extract an NSIS installer .exe (requires 7-Zip)
bn-loader profile install C:\Downloads\binaryninja_win64_X.Y.Z_personal.exe --dest "C:\Program Files\Binary Ninja Personal"

# Override the auto-derived profile name and/or config dir
bn-loader profile install ~/Downloads/binaryninja_linux_X.Y.Z_personal.zip \
    --dest /opt/binaryninja/stable \
    --name personal-stable \
    --config-dir ~/.binaryninja-personal-stable

# Extract only -- don't touch the config file
bn-loader profile install ~/Downloads/binaryninja_linux_X.Y.Z_personal.zip --dest /opt/binaryninja/stable --no-register
```

By default, `bn-loader profile install` registers a new profile in your config:
- `--name` defaults to a sanitized basename of `--dest` (`/opt/binaryninja/stable` -> `stable`). Invalid characters become underscores.
- `--config-dir` defaults to `~/.config/bn-loader/profiles/<name>`.
- The profile name is the only uniqueness constraint. If you omit `--name` and the derived name is already taken, bn-loader auto-bumps to `<name>-2`, `<name>-3`, etc.
- If you explicitly pass `--name` and that name already exists, bn-loader errors out -- an explicit name is never silently rewritten.
- `--config-dir` can be shared across multiple profiles (e.g., two installations that share plugins/settings). bn-loader does not check or warn if the config dir already contains files.

Pass `--no-register` to skip profile registration entirely (install becomes a pure extract).

For the Windows NSIS path, `bn-loader` shells out to 7z. Resolution order is `--seven-zip` flag, then `[install] seven_zip` in your config, then `$PATH`. NSIS-only artifacts (`$PLUGINSDIR/`, installer images, VC++ redistributables) are filtered out automatically. ZIP archives have their single top-level directory stripped (so `--dest /opt/binaryninja/stable` produces `binaryninja` directly under it, not nested).

If `--dest` is non-empty, `bn-loader profile install` requires `--force` to overlay-extract on top (existing files are preserved unless they share a name with an entry in the archive). The blast-radius warning lists which profiles will be affected by the install (relevant for updates and shared installations). Pass `--yes` to skip the interactive `[y/N]` confirmation; pass `--force --yes` for a fully unattended overlay install.

**profile update** - Re-extract Binary Ninja into an existing profile's install_dir:
```bash
bn-loader profile update stable ~/Downloads/binaryninja_linux_NEW.zip --yes
```
A thin wrapper over `profile install`. Looks up the profile by name, uses its `install_dir` as the dest, and runs the install logic with `--force` (overlay) and `--no-register` (already registered) implicit. Honors `--yes`, `--dry-run`, `--seven-zip`. Useful for unattended updates.

**profile remove** - Deregister a profile (and optionally delete its on-disk dirs):
```bash
# Just deregister (default): leaves install_dir and config_dir on disk
bn-loader profile remove dev

# Also delete the profile's config_dir
bn-loader profile remove dev --purge

# Also delete the install_dir (only with --purge --force)
bn-loader profile remove dev --purge --force
```
Always shows blast radius before acting (warns if `install_dir` or `config_dir` is shared with other profiles). Refuses to `--purge` a `config_dir` shared with other profiles, and refuses to `--purge --force` an `install_dir` shared with other profiles — remove those profiles first or skip `--purge`/`--force`. Note: editing the config file is a TOML round-trip, so comments and exact formatting are not preserved across removes.

**profile sync** - Copy settings between profiles:
```bash
# Sync from personal to all other profiles
bn-loader profile sync --from personal

# Sync to a specific profile
bn-loader profile sync --from personal --to commercial

# Preview changes without applying
bn-loader profile sync --from personal --dry-run
```
License files and other sensitive data are excluded by default. You can add more exclusions in the `[sync]` section of your config.

**profile plugins** - List installed plugins for a profile:
```bash
bn-loader profile plugins personal
```

**profile diff** - Compare two profiles:
```bash
bn-loader profile diff personal commercial
```

**profile list** - List available profiles (canonical form: `bn-loader profile list`):
```bash
bn-loader profile list
```
This produces the same output as the `--list` shortcut flag (see Usage above).

**env** - Print shell commands to set up Binary Ninja's Python API for a profile (ssh-agent style):

```bash
# bash / zsh
eval "$(bn-loader env personal)"

# fish
bn-loader env personal --shell fish | source

# PowerShell
bn-loader env personal --shell powershell | Invoke-Expression
```

This lets you `import binaryninja` from a regular Python interpreter outside Binary Ninja, using the selected profile's install and license. It emits three variables to stdout (diagnostics go to stderr, so `eval` stays safe):

- `PYTHONPATH` - prepends `<install_dir>/python` so the `binaryninja` package is importable.
- `BN_USER_DIRECTORY` - points at the profile's `config_dir` (license and settings).
- The OS library search path (`PATH` on Windows, `LD_LIBRARY_PATH` on Linux, `DYLD_LIBRARY_PATH` on macOS) - prepends `<install_dir>` so the Binary Ninja core library resolves.

The output shell dialect is auto-detected (PowerShell on Windows; otherwise from `$SHELL`). Override it with `--shell bash|zsh|sh|fish|powershell|cmd`. Note: `cmd` output uses `set` statements that you run directly rather than via `eval`/`Invoke-Expression`.

**doctor** - Validate the whole config (read-only):
```bash
bn-loader doctor
```
Checks each profile's `install_dir` / `executable` / `config_dir`, plus global checks (`[install] seven_zip` if set, orphan dirs under `~/.config/bn-loader/profiles/`). Prints `[OK]` / `[WARN]` / `[FAIL]` per check (to stderr) and a summary line on stdout. Exits 0 if no failures, 1 otherwise — useful for `bn-loader doctor && bn-loader <profile>` patterns and CI.

**completions** - Set up shell completions:
```bash
bn-loader completions bash
bn-loader completions zsh
bn-loader completions fish
bn-loader completions powershell
```

### Flag conventions

Standardized across every mutating command:

- `--yes` (`-y`) — skip all interactive `[y/N]` confirmation prompts (just say yes).
- `--force` (`-f`) — override safety checks. Allow destructive defaults that are blocked otherwise (e.g., overlay-extract on a non-empty `--dest`, remove a profile whose `install_dir` is shared, sync without backups).
- `--dry-run` — print what would happen without doing it.

Fully-unattended scripts typically want `--yes --force`. The two are independent — `--yes` answers prompts, `--force` overrides safety blocks.

### Output streams

`bn-loader` follows standard stdout/stderr discipline so subcommand output composes cleanly with shell pipelines:

- **stdout:** subcommand result data (profile lists, diff entries, plugin tables, doctor summary).
- **stderr:** status updates, warnings, errors, prompts.

So `bn-loader profile plugins commercial | grep ...` filters only the actual plugin lines, and `bn-loader profile list 2>/dev/null` outputs only the profile list with no header noise.

## Shell Completions

bn-loader supports tab completion for profile names and commands. Run `bn-loader completions <shell>` for setup instructions specific to your shell.

## Global Options

These go in the `[global]` section:

| Option | Default | Description |
|--------|---------|-------------|
| `default_profile` | none | Profile to launch when no argument given |
| `color` | `"auto"` | Color output: `"auto"`, `"always"`, `"never"` |
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
bn-loader profile sync --from personal --exclude "temp/"
```

## License

BSD-3-Clause. See [LICENSE](LICENSE) for details.
