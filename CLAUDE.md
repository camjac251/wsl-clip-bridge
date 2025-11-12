# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

WSL Clip Bridge is a secure xclip replacement for WSL that enables clipboard sharing between Windows and Linux without X11/Wayland dependencies. It's designed specifically for integrating Windows screenshot tools like ShareX with terminal applications like Claude Code.

## Project Configuration

### Cargo.toml
- Package: `wsl-clip-bridge` (binary output: `xclip`)
- Edition: 2024, requires Rust 1.89+
- Dependencies: `serde`, `toml`, `image` (with PNG/JPEG/GIF/WebP support only)
- Release profile: Optimized for size (`opt-level = "s"`), LTO enabled

### Formatting (rustfmt.toml)
- Edition: 2024
- Max width: 100 characters
- Tab spaces: 4
- Hard tabs: disabled

## Build Commands

```bash
# Development build
cargo build

# Release build (statically linked musl binary, no GLIBC dependency)
rustup target add x86_64-unknown-linux-musl  # or aarch64-unknown-linux-musl for ARM64
cargo build --release --target x86_64-unknown-linux-musl --locked

# Format code
cargo fmt

# Check formatting (CI requirement)
cargo fmt -- --check

# Run clippy linter
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --verbose

# Clean build artifacts
cargo clean
```

## Architecture

### Core Implementation
- **Single binary**: `src/main.rs` compiles to `xclip` binary
- **No unsafe code**: Enforced via `#![forbid(unsafe_code)]`
- **xclip compatibility**: Maintains full CLI compatibility with real xclip (`-selection`, `-t`, `-o`, `-i` flags)

### Clipboard Storage Strategy
The tool uses file-based clipboard emulation with these paths:
- `~/.cache/wsl-clip-bridge/text.txt` - Text clipboard data
- `~/.cache/wsl-clip-bridge/image.bin` - Image binary data
- `~/.cache/wsl-clip-bridge/image.format` - Image MIME type

Files automatically expire based on TTL and are cleaned up on next access.

### Security Model
1. **Path access**: If `allowed_directories` is not configured, all paths are allowed
2. **Directory restrictions**: When configured, only specified directories (and subdirectories) are accessible
3. **Size limits**: Configurable max file size to prevent memory exhaustion
4. **Permission hardening**: Restrictive permissions for data files and directories

### Image Processing Pipeline
1. Accepts PNG/JPEG/GIF/WebP formats
2. Optionally downscales to `max_image_dimension` for API optimization
3. Uses Lanczos3 filter for high-quality resampling
4. Preserves aspect ratio during resize

### Configuration Hierarchy
1. Environment variable: `WSL_CLIP_BRIDGE_TTL_SECS` (highest priority)
2. Config file: `~/.config/wsl-clip-bridge/config.toml`
3. Defaults in code (lowest priority)

### Configuration File Structure (config.toml)
```toml
ttl_secs = <seconds>              # Clipboard data TTL in seconds
max_image_dimension = <pixels>   # Max dimension for image downscaling (0 = disabled)
max_file_size_mb = <megabytes>   # Maximum file size limit
# allowed_directories = ["<path1>", "<path2>"]  # Optional: restrict to specific paths only
```

## Installation Scripts

### PowerShell Installer (`scripts/setup.ps1`)
- Interactive Windows installer with WSL distribution detection
- Downloads pre-built binaries based on architecture (x64/ARM64)
- Configures ShareX integration automatically
- Sets up PATH for user installations
- Parameters:
  - `-SkipShareX`: Skip ShareX configuration
  - `-AutoConfirm`: Use defaults without prompts
  - `-WSLDistribution <name>`: Specify WSL distribution

### WSL Build Script (`scripts/install-wsl.sh`)
- Builds from source using cargo
- Supports `--system` (requires sudo) or `--user` installation
- Handles PATH configuration for user installations

## Release Process

GitHub Actions workflow (`release.yml`) handles multi-architecture builds:
- Targets: `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
- Statically linked musl binaries (no GLIBC dependency)
- Works on any Linux system regardless of GLIBC version
- Binary stripping for size optimization
- Automatic checksum generation

## Development Notes

### Clippy Configuration
Strict linting is enforced via `Cargo.toml`:
```toml
[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
```

### Binary Naming
The output binary is named `xclip` (not `wsl-clip-bridge`) to act as a drop-in replacement.

### Testing ShareX Integration
1. Windows side: Take screenshot with ShareX (triggers custom action)
2. WSL side: Run `xclip -t TARGETS -o` to verify clipboard has image
3. Paste in Claude Code with Ctrl+V

### Environment Variables
- `WSL_CLIP_BRIDGE_TTL_SECS`: Override TTL
- `WSL_CLIP_BRIDGE_CONFIG`: Custom config file path
- `XDG_CONFIG_HOME`: Standard config directory override
- `XDG_CACHE_HOME`: Standard cache directory override