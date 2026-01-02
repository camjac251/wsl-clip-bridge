# WSL Clip Bridge

[![CI](https://github.com/camjac251/wsl-clip-bridge/actions/workflows/ci.yml/badge.svg)](https://github.com/camjac251/wsl-clip-bridge/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A secure xclip replacement for WSL that enables clipboard sharing between Windows and Linux. Designed for pasting screenshots into terminal applications like [Claude Code](https://claude.ai/code). Works with WSLg's wl-clipboard for direct Windows clipboard access, or with file-based workflows like ShareX.

## Installation

### Option 1: mise (Recommended)

If you use [mise](https://mise.jdx.dev/) for tool management:

```bash
# Install globally
mise use -g "github:camjac251/wsl-clip-bridge@latest"

# Or add to ~/.config/mise/config.toml
```

```toml
[tools]
"github:camjac251/wsl-clip-bridge" = { version = "latest", bin = "xclip" }
```

### Option 2: Download Binary

```bash
# Detect architecture and download
ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')
curl -fsSL "https://github.com/camjac251/wsl-clip-bridge/releases/latest/download/xclip-${ARCH}" -o xclip
chmod +x xclip

# System-wide (requires sudo)
sudo mv xclip /usr/local/bin/

# Or user-local (no sudo)
mkdir -p ~/.local/bin
mv xclip ~/.local/bin/
# Ensure ~/.local/bin is in your PATH
```

### Option 3: Build from Source

Requires Rust 1.89+:

```bash
git clone https://github.com/camjac251/wsl-clip-bridge
cd wsl-clip-bridge

# Static musl build (no glibc dependency)
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl --locked

# System-wide
sudo install -m 755 target/x86_64-unknown-linux-musl/release/xclip /usr/local/bin/

# Or user-local
install -m 755 target/x86_64-unknown-linux-musl/release/xclip ~/.local/bin/
```

For ARM64, use `aarch64-unknown-linux-musl` instead.

### Option 4: PowerShell Installer (Windows)

Automated setup with optional ShareX integration:

```powershell
iwr -useb https://raw.githubusercontent.com/camjac251/wsl-clip-bridge/main/scripts/setup.ps1 | iex
```

## Usage

### Basic Commands

```bash
xclip -o                        # Output text from clipboard
xclip -t TARGETS -o             # List available clipboard formats
xclip -t image/png -o > img.png # Save clipboard image to file
echo "text" | xclip -i          # Copy text to clipboard
xclip -t image/png -i < img.png # Copy image to clipboard
xclip --help                    # Show all options
```

### With Claude Code

1. Copy an image in Windows (screenshot, browser, etc.)
2. In Claude Code terminal: press `Ctrl+V`

**Terminal Setup Required**: Your terminal must forward `Ctrl+V` to the application:

| Terminal | Configuration |
|----------|---------------|
| Windows Terminal | Settings → Actions → Remove `Ctrl+V` binding |
| Warp | Settings → Keyboard Shortcuts → Set "Paste" to `Ctrl+Shift+V` |
| Others | Unbind `Ctrl+V` from paste action |

### How It Works

1. **WSLg/wl-clipboard**: Automatically detects Windows clipboard via WSLg
2. **BMP→PNG conversion**: Windows clipboard BMPs are converted to PNG automatically
3. **File fallback**: Falls back to cached files if wl-clipboard unavailable

```
Windows Clipboard → wl-clipboard (WSLg) → xclip → Application
```

## Configuration

Config auto-creates at `~/.config/wsl-clip-bridge/config.toml` on first run:

```toml
# Clipboard data TTL in seconds (default: 300)
ttl_secs = 300

# Max image dimension for downscaling (0 = disabled)
# 1568 is optimal for Claude API
max_image_dimension = 1568

# Max file size in MB (default: 100)
max_file_size_mb = 100

# Clipboard mode: "auto" (default) or "file_only"
# auto = wl-clipboard first, file fallback
# file_only = only use file-based clipboard
clipboard_mode = "auto"

# Cache wl-clipboard images for faster access (default: true)
cache_wl_images = true

# Optional: restrict file access to specific directories
# allowed_directories = ["/mnt/c/Users/YOU/Screenshots", "/tmp"]
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `WSL_CLIP_BRIDGE_TTL_SECS` | Override TTL (takes precedence over config) |
| `WSL_CLIP_BRIDGE_CONFIG` | Custom config file path |

## ShareX Integration (Optional)

For advanced screenshot workflows with ShareX:

### 1. Create Batch File

Save as `%USERPROFILE%\Documents\ShareX\Tools\copy-to-wsl.bat`:

```batch
@echo off
setlocal enabledelayedexpansion

if "%~1"=="" exit /b 1

set "EXT=%~x1"
set "EXT=!EXT:~1!"

if /i "!EXT!"=="png" (set "MIME=image/png"
) else if /i "!EXT!"=="jpg" (set "MIME=image/jpeg"
) else if /i "!EXT!"=="jpeg" (set "MIME=image/jpeg"
) else if /i "!EXT!"=="gif" (set "MIME=image/gif"
) else if /i "!EXT!"=="webp" (set "MIME=image/webp"
) else (set "MIME=image/png")

rem Change "Ubuntu" to your WSL distro name (run 'wsl -l' to check)
for /f "usebackq tokens=*" %%i in (`wsl -d Ubuntu wslpath -u "%~1"`) do set WSLPATH=%%i
wsl -d Ubuntu xclip -selection clipboard -t !MIME! -i "!WSLPATH!"
```

**Important**: Replace `Ubuntu` with your WSL distribution name. Check with `wsl -l`.

### 2. Configure ShareX

1. **Task Settings → Actions → Add**:
   - Name: `Copy to WSL`
   - File path: `%USERPROFILE%\Documents\ShareX\Tools\copy-to-wsl.bat`
   - Arguments: `%input`
   - Hidden window: checked

2. **After capture tasks**:
   - Enable "Save image to file"
   - Enable "Perform actions" → select "Copy to WSL"

## Troubleshooting

### `xclip: command not found`

Ensure the binary is in your PATH:

```bash
which xclip
echo $PATH | tr ':' '\n' | grep -E '(local|bin)'
```

### Images not pasting

1. Check wl-clipboard works: `wl-paste --list-types`
2. Check xclip sees it: `xclip -t TARGETS -o`
3. Verify config mode: `cat ~/.config/wsl-clip-bridge/config.toml`

### Ctrl+V not working in Claude Code

Your terminal is intercepting the keystroke. See [Terminal Setup](#with-claude-code) above.

### Permission denied

If `allowed_directories` is configured in your config, files must be in those paths:

```bash
cat ~/.config/wsl-clip-bridge/config.toml | grep allowed
```

### BMP shows as available but image is empty

This is expected. Windows clipboard often contains BMP format, which is automatically converted to PNG. Use `xclip -t image/png -o` to get the converted image.

### WSLg not available (Windows 10)

Set `clipboard_mode = "file_only"` and use ShareX integration instead.

## Storage

Clipboard data is cached at:

```
~/.cache/wsl-clip-bridge/
├── text.txt      # Text clipboard
├── image.bin     # Image data
└── image.format  # MIME type
```

Files expire based on TTL and are cleaned up automatically.

## Development

```bash
cargo build                    # Dev build
cargo test                     # Run tests
cargo fmt && cargo clippy      # Format and lint
```

## License

MIT
