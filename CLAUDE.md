# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this tool is

A stateless `xclip` shim for Claude Code on WSL. Its only reason to exist: decode the `BI_BITFIELDS` 32-bpp BMPs that WSLg emits from the Windows clipboard into PNGs. sharp/libvips refuses those BMPs, so Claude Code's paste pipeline otherwise silently fails on any Windows-sourced screenshot.

Everything else (per-model image caps, Lanczos3 resize, compression ladder, API size limits) is Claude Code's job. This tool is intentionally dumb and small.

## Cargo.toml

- Package: `wsl-clip-bridge` (binary output: `xclip`)
- Edition: 2024, MSRV 1.92
- Single dependency: `image` crate with `default-features = false` and only `png` + `bmp` features
- Release profile: size-optimized (`opt-level = "s"`), LTO, `codegen-units = 1`

## Build

```bash
cargo build
cargo build --release --target x86_64-unknown-linux-musl --locked

cargo fmt                              # format
cargo fmt -- --check                   # CI format check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Architecture

Single binary, `src/main.rs`.

```
Claude Code -> xclip shim -> wl-paste -> Windows clipboard
                   |
                   BMP (BI_BITFIELDS) -> PNG
                   via Rust `image` crate
```

### What Claude Code invokes

| Call | Behavior |
|---|---|
| `xclip -t TARGETS -o` | Query `wl-paste --list-types`; advertise `image/png` when only BMP is present |
| `xclip -t image/png -o` | Pass wl-paste PNG through, or decode wl-paste BMP and emit PNG |
| `xclip -t image/bmp -o` | Pass wl-paste BMP through |
| `xclip -t text/plain -o` | Pass wl-paste text through |

Every invocation is independent and stateless. Fresh wl-paste call each time, no cache, no config.

### Safety + timeout

`#![forbid(unsafe_code)]`. Every `wl-paste` call is wrapped in a 5-second timeout with thread-based stdout reading to avoid pipe-buffer deadlocks.

## Installation

Primary: `brew install camjac251/tap/wsl-clip-bridge`. Tap auto-updates on release via the release workflow.

Secondary: `mise use -g ubi:camjac251/wsl-clip-bridge`, GitHub release binary download, or `cargo build --release` + manual install to `~/.local/bin/xclip`.

## Release

GitHub Actions (`release.yml`) handles:
- release-plz PR + release (conventional commits drive the version bump)
- Multi-arch musl builds: `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
- Stripped binaries + SHA256 attestation
- Tarball containing the binary + README
- Homebrew tap auto-update via the `camjac251/homebrew-tap` repo

## Testing

- `cargo test` runs the hermetic BMP -> PNG round-trip test (one real test, no fixtures).
- Smoke test the installed binary against a real WSLg clipboard:

  ```bash
  # With a screenshot on the Windows clipboard:
  xclip -selection clipboard -t TARGETS -o         # expect: image/png ...
  xclip -selection clipboard -t image/png -o | file -  # expect: PNG image data
  ```

## Binary naming

Binary output is `xclip` (not `wsl-clip-bridge`) so it acts as a drop-in replacement when Claude Code's paste pipeline invokes `xclip ...` with `~/.local/bin` ahead of `/home/linuxbrew/.linuxbrew/bin` on `PATH`.
