# cc-usage-tracker

[![Build](https://github.com/georgii/cc-usage-tracker/actions/workflows/build.yml/badge.svg)](https://github.com/georgii/cc-usage-tracker/actions/workflows/build.yml)

A Linux system tray application that displays your Claude Code usage statistics at a glance.

## Features

- **Real-time usage display** in system tray: `XX% (Xh Xm)`
- **Session usage** (5-hour window) with reset countdown
- **Weekly usage** (7-day) across all models
- **Sonnet-specific usage** tracking (when available)
- **Dropdown menu** with detailed statistics
- Automatic polling every 60 seconds
- Lightweight and unobtrusive

## Requirements

- Linux with a system tray (GNOME, KDE, XFCE, etc.)
- Claude Code CLI installed and authenticated (`claude` command available)
- GTK3 and libappindicator3

### System Dependencies

**Debian/Ubuntu:**
```bash
sudo apt install libgtk-3-dev libappindicator3-dev
```

**Fedora:**
```bash
sudo dnf install gtk3-devel libappindicator-gtk3-devel
```

**Arch Linux:**
```bash
sudo pacman -S gtk3 libappindicator-gtk3
```

## Installation

### From Release (Recommended)

Download the latest release from the [Releases page](https://github.com/georgii/cc-usage-tracker/releases):

```bash
# Extract the tarball
tar -xzf cc-usage-tracker-linux-x86_64.tar.gz

# Move to a directory in your PATH
sudo mv cc-usage-tracker /usr/local/bin/
sudo mkdir -p /usr/local/share/cc-usage-tracker
sudo mv assets /usr/local/share/cc-usage-tracker/
```

### From Source

1. Install Rust (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Clone the repository:
   ```bash
   git clone https://github.com/georgii/cc-usage-tracker.git
   cd cc-usage-tracker
   ```

3. Build and run (assets are found automatically during development):
   ```bash
   cargo build --release
   cargo run --release
   ```

4. Optional: Install system-wide:
   ```bash
   sudo cp target/release/cc-usage-tracker /usr/local/bin/
   ```

## Usage

Simply run the application:
```bash
cc-usage-tracker
```

The indicator will appear in your system tray showing your current usage percentage and time until reset.

### Tray Display

- **Label format:** `XX% (Xh Xm)` - Current session usage and time until reset
- **Click** the indicator to see detailed statistics:
  - Session (5h): XX% used, resets in Xh Xm
  - Weekly (7d): XX% used, resets in Xd Xh
  - Sonnet (7d): XX% used (if available)

### Autostart

To start automatically on login, create a desktop entry:

```bash
mkdir -p ~/.config/autostart
cat > ~/.config/autostart/cc-usage-tracker.desktop << EOF
[Desktop Entry]
Type=Application
Name=Claude Code Usage Tracker
Exec=cc-usage-tracker
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
EOF
```

## Development

```bash
# Build debug version
cargo build

# Run with logging
cargo run

# Run tests
cargo test

# Check for issues
cargo clippy

# Generate documentation
cargo doc --open
```

## How It Works

The application fetches usage data by running the Claude CLI with the `/usage` command. Since the Claude CLI requires a terminal (PTY), it uses the `script` command for terminal emulation:

```bash
script -q -c "timeout 8 sh -c \"echo '/usage' | claude\"" /dev/null
```

It parses the terminal output to extract:
- Usage percentages (handles both `% used` and `%used` formats)
- Reset times (converted to countdown format)

The `/usage` command is a local CLI command that queries the API directly without consuming any model tokens. The app polls every 60 seconds and updates the tray label and menu with fresh data.

## Troubleshooting

### "CC: Error" in tray

- Ensure Claude Code CLI is installed: `which claude`
- Ensure you're logged in: `claude` should start without auth errors
- Check logs: run `cc-usage-tracker` from terminal to see error messages

### Tray icon not visible

- Ensure your desktop environment supports AppIndicator/system tray
- For GNOME, you may need the "AppIndicator Support" extension

### Usage not updating

- The app polls every 60 seconds; wait for the next update
- Check terminal output for timeout or parsing errors

## Contributing

Contributions are welcome! Please ensure your code passes CI checks:

```bash
cargo fmt --check    # Code formatting
cargo clippy         # Linting
cargo test           # Unit tests
cargo build --release
```

## License

MIT
