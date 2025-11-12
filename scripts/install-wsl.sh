#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default values
INSTALL_LOCATION="user"
INSTALL_DIR="${HOME}/.local/bin"
NEEDS_SUDO=""

# Functions for colored output
print_step() {
    echo -e "${CYAN}[*]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_error() {
    echo -e "${RED}[!]${NC} $1"
}

print_prompt() {
    echo -e "${YELLOW}[?]${NC} $1"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --system)
            INSTALL_LOCATION="system"
            INSTALL_DIR="/usr/local/bin"
            NEEDS_SUDO="sudo"
            shift
            ;;
        --user)
            INSTALL_LOCATION="user"
            INSTALL_DIR="${HOME}/.local/bin"
            NEEDS_SUDO=""
            shift
            ;;
        --install-dir)
            INSTALL_DIR="$2"
            INSTALL_LOCATION="custom"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --user              Install to user directory ~/.local/bin (default)"
            echo "  --system            Install system-wide to /usr/local/bin (requires sudo)"
            echo "  --install-dir DIR   Custom installation directory"
            echo "  --help              Show this help message"
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo -e "${GREEN}WSL Clip Bridge - Build & Install Script${NC}"
echo "=========================================="
echo ""

# Show installation location
if [ "$INSTALL_LOCATION" = "user" ]; then
    print_success "Installing to user directory (no sudo required): $INSTALL_DIR"
elif [ "$INSTALL_LOCATION" = "system" ]; then
    print_success "Installing system-wide (sudo required): $INSTALL_DIR"
else
    print_success "Installing to custom directory: $INSTALL_DIR"
fi
echo ""

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    print_error "Cargo not found. Please install Rust first."
    echo "Visit: https://rustup.rs/"
    echo "Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Get script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Check if project directory exists
if [ ! -d "$PROJECT_DIR" ]; then
    print_error "Project directory not found at $PROJECT_DIR"
    exit 1
fi

# Detect architecture and set target
ARCH=$(uname -m)
if [ "$ARCH" = "x86_64" ]; then
    TARGET="x86_64-unknown-linux-musl"
    BINARY_PATH="target/x86_64-unknown-linux-musl/release/xclip"
elif [ "$ARCH" = "aarch64" ]; then
    TARGET="aarch64-unknown-linux-musl"
    BINARY_PATH="target/aarch64-unknown-linux-musl/release/xclip"
else
    print_error "Unsupported architecture: $ARCH"
    exit 1
fi

# Add musl target if not already installed
print_step "Adding musl target for $ARCH..."
rustup target add $TARGET

# Build the project with musl for static linking
print_step "Building wsl-clip-bridge (static musl binary)..."
cd "$PROJECT_DIR"
cargo build --release --target $TARGET --locked

# Check if build was successful
if [ ! -f "$BINARY_PATH" ]; then
    print_error "Build failed. Binary not found at $BINARY_PATH"
    exit 1
fi

# Strip the binary to reduce size
print_step "Stripping binary to reduce size..."
strip "$BINARY_PATH"

# Create install directory if it doesn't exist
if [ "$INSTALL_LOCATION" = "system" ]; then
    print_step "Creating system directory (requires sudo)..."
    sudo mkdir -p "$INSTALL_DIR"
else
    print_step "Creating install directory..."
    mkdir -p "$INSTALL_DIR"
fi

# Install the binary
print_step "Installing xclip to $INSTALL_DIR..."
if [ "$INSTALL_LOCATION" = "system" ]; then
    sudo cp "$BINARY_PATH" "$INSTALL_DIR/"
    sudo chmod +x "$INSTALL_DIR/xclip"
else
    cp "$BINARY_PATH" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/xclip"
fi

print_success "Binary installed to $INSTALL_DIR/xclip"

# Copy example config if user doesn't have one
CONFIG_DIR="$HOME/.config/wsl-clip-bridge"
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    print_step "Creating default config file..."
    mkdir -p "$CONFIG_DIR"
    
    # Create default config
    cat > "$CONFIG_DIR/config.toml" << 'EOF'
# WSL Clip Bridge Configuration

# Clipboard TTL in seconds (default: 300)
ttl_secs = 300

# Maximum image dimension in pixels (0 = no downscaling)
# Recommended: 1568 for optimal Claude API performance
max_image_dimension = 1568

# Security Settings

# Maximum file size in MB (default: 100)
max_file_size_mb = 100

# Directory access restrictions
# If not configured, all paths are allowed
# To restrict access to specific directories (and their subdirectories):
#
# allowed_directories = [
#   "/mnt/c/Users/YOUR_USERNAME/Documents/ShareX",
#   "/home/YOUR_USERNAME",
#   "/tmp"
# ]
EOF
    
    print_success "Config file created at $CONFIG_DIR/config.toml"
fi

echo ""
print_success "Installation complete!"
echo ""

# Check if install directory is in PATH (only for user installation)
if [ "$INSTALL_LOCATION" = "user" ] && [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    print_prompt "$INSTALL_DIR is not in your PATH"
    echo ""
    
    # Function to add PATH to a shell config file
    add_to_path() {
        local config_file="$1"
        local shell_name="$2"
        
        if [ -f "$HOME/$config_file" ]; then
            # Check if PATH export already exists
            if ! grep -q "export PATH=.*$INSTALL_DIR" "$HOME/$config_file"; then
                echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$HOME/$config_file"
                print_success "Added to $config_file"
                return 0
            else
                print_success "PATH already configured in $config_file"
                return 1
            fi
        fi
        return 2
    }
    
    # Try to detect the current shell
    CURRENT_SHELL=$(basename "$SHELL")
    PATH_ADDED=false
    
    case "$CURRENT_SHELL" in
        bash)
            add_to_path ".bashrc" "bash" && PATH_ADDED=true
            ;;
        zsh)
            add_to_path ".zshrc" "zsh" && PATH_ADDED=true
            ;;
        *)
            # Try common shell configs
            add_to_path ".bashrc" "bash" && PATH_ADDED=true
            add_to_path ".profile" "profile" && PATH_ADDED=true
            ;;
    esac
    
    if [ "$PATH_ADDED" = true ]; then
        echo ""
        echo "To use xclip immediately, run:"
        echo "  source ~/.$CURRENT_SHELL"
        echo ""
        echo "Or restart your terminal session."
    fi
elif [ "$INSTALL_LOCATION" = "system" ]; then
    print_success "$INSTALL_DIR is in the system PATH"
else
    print_success "$INSTALL_DIR is already in your PATH"
fi

echo ""
echo "You can verify the installation by running:"
echo "  xclip -version"
echo ""
echo "Configuration:"
echo "  Config file: ~/.config/wsl-clip-bridge/config.toml"
echo "  Storage: ~/.cache/wsl-clip-bridge/"
echo ""

# Test the installation
print_step "Testing installation..."
if "$INSTALL_DIR/xclip" -version &> /dev/null; then
    print_success "xclip is working correctly!"
else
    print_error "xclip test failed. Please check the installation."
    exit 1
fi

echo ""
echo "Next steps:"
echo "  1. Run the Windows installer: setup.cmd (from Windows)"
echo "  2. Configure ShareX integration (optional)"
echo "  3. Test with: echo 'Hello' | xclip -i && xclip -o"