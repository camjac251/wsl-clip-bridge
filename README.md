# WSL Clip Bridge

[![CI](https://github.com/camjac251/wsl-clip-bridge/actions/workflows/ci.yml/badge.svg)](https://github.com/camjac251/wsl-clip-bridge/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A tiny `xclip` shim that makes Ctrl+V image paste work in [Claude Code](https://claude.ai/code) on WSL.

## Why this exists

Windows screenshot clipboards arrive on WSLg as 32-bpp BMPs with `compression = BI_BITFIELDS`. sharp and libvips only support `BI_RGB`, `BI_RLE8`, and `BI_RLE4`, so Claude Code's paste pipeline throws `Input buffer contains unsupported image format` on a clearly-copied screenshot and you see "No image found in clipboard".

This tool plugs that one gap. When Claude Code runs `xclip -t image/png -o`, we fetch the BMP via `wl-paste`, decode it with the Rust `image` crate (which handles `BI_BITFIELDS`), and emit a PNG that sharp is happy to consume.

## Install

### Homebrew (recommended)

```bash
brew install camjac251/tap/wsl-clip-bridge
```

### mise

```bash
mise use -g ubi:camjac251/wsl-clip-bridge
```

### Binary download

```bash
ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')
curl -fsSL "https://github.com/camjac251/wsl-clip-bridge/releases/latest/download/xclip-${ARCH}" -o xclip
chmod +x xclip
mkdir -p ~/.local/bin && mv xclip ~/.local/bin/
# ensure ~/.local/bin is on PATH
```

### Build from source

Requires Rust 1.92+:

```bash
git clone https://github.com/camjac251/wsl-clip-bridge
cd wsl-clip-bridge
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl --locked
install -m 755 target/x86_64-unknown-linux-musl/release/xclip ~/.local/bin/
```

For ARM64, use `aarch64-unknown-linux-musl`.

## Usage

1. Copy an image in Windows (screenshot, browser, etc.)
2. In the Claude Code terminal, press Ctrl+V

### Terminal setup

Your terminal must forward Ctrl+V to the application instead of intercepting it for its own paste action.

| Terminal | Configuration |
|----------|---------------|
| Windows Terminal | `settings.json`: add `{ "id": null, "keys": "ctrl+v" }` to `keybindings` |
| Warp | Keyboard Shortcuts: set "Paste" to `Ctrl+Shift+V` |
| Others | Unbind `Ctrl+V` from the paste action |

### Verify

With a screenshot on the Windows clipboard:

```bash
xclip -selection clipboard -t TARGETS -o    # expect: image/png ...
xclip -selection clipboard -t image/png -o | file -  # expect: PNG image data
```

## How it works

```
Windows clipboard
    -> WSLg wl-paste
        -> xclip shim (this tool)
            BMP (BI_BITFIELDS) -> PNG via Rust `image` crate
        -> Claude Code
            -> sharp resize (per-model cap)
        -> Anthropic API
```

Claude Code's paste pipeline runs four `xclip` calls. This tool answers them:

| Claude Code call | Behavior |
|---|---|
| `xclip -t TARGETS -o` | `wl-paste --list-types`, then advertise `image/png` if only BMP is present |
| `xclip -t image/png -o` | wl-paste PNG passthrough, or decode wl-paste BMP and emit PNG |
| `xclip -t image/bmp -o` | wl-paste BMP passthrough |
| `xclip -t text/plain -o` | wl-paste text passthrough |

No state, no config. Every invocation fetches fresh from `wl-paste`.

## Troubleshooting

### Ctrl+V does nothing

Your terminal is intercepting the keystroke before Claude Code sees it. See [Terminal setup](#terminal-setup).

### "No image found in clipboard" toast

Ctrl+V is reaching Claude Code but the wrapper isn't returning a PNG. Check:

```bash
which xclip                                          # should be this tool
wl-paste --list-types                                # should show image/bmp
xclip -selection clipboard -t image/png -o | file -  # should say PNG image data
```

If `which xclip` points somewhere other than this tool's install path, fix your PATH.

### `xclip: command not found`

Ensure the binary is in your PATH:

```bash
echo $PATH | tr ':' '\n'
```

## Development

```bash
cargo build            # dev build
cargo test             # run tests
cargo fmt              # format
cargo clippy --all-targets --all-features -- -D warnings
```

See [CLAUDE.md](CLAUDE.md) for scope, invariants, and guidance before changing anything.

## License

MIT
