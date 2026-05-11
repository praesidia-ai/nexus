#!/usr/bin/env bash
#
# Nexus Installer — local-first, zero cloud dependencies
#
# Usage:
#   curl -fsSL https://your-domain.com/install.sh | bash
#   # or
#   bash install.sh
#
# What it does:
#   1. Detects OS (macOS / Linux)
#   2. Installs Rust (via rustup) if missing
#   3. Installs Node.js (via nvm) if missing
#   4. Builds the Nexus binaries (nexus-server + nexus CLI)
#   5. Adds to PATH
#
# Requirements: curl, git

set -euo pipefail

# Preflight: required tools
for cmd in curl git; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "[nexus] ERROR: '$cmd' is required but not installed." >&2
        exit 1
    fi
done

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}[nexus]${NC} $1"; }
warn()  { echo -e "${YELLOW}[nexus]${NC} $1"; }
error() { echo -e "${RED}[nexus]${NC} $1"; exit 1; }

# ---------------------------------------------------------------------------
# 1. Detect OS
# ---------------------------------------------------------------------------
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin) PLATFORM="macos" ;;
    Linux)  PLATFORM="linux" ;;
    *)      error "Unsupported OS: $OS. Nexus supports macOS and Linux." ;;
esac

info "Detected: $PLATFORM ($ARCH)"

# ---------------------------------------------------------------------------
# 2. Install Rust if missing
# ---------------------------------------------------------------------------
if command -v rustc &>/dev/null; then
    RUST_VER=$(rustc --version | awk '{print $2}')
    info "Rust already installed: $RUST_VER"
else
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
    info "Rust installed: $(rustc --version | awk '{print $2}')"
fi

# ---------------------------------------------------------------------------
# 3. Install Node.js if missing
# ---------------------------------------------------------------------------
if command -v node &>/dev/null; then
    NODE_VER=$(node --version)
    info "Node.js already installed: $NODE_VER"
else
    info "Installing Node.js via nvm..."
    if ! command -v nvm &>/dev/null; then
        curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
        export NVM_DIR="$HOME/.nvm"
        [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
    fi
    nvm install --lts
    nvm use --lts
    info "Node.js installed: $(node --version)"
fi

# ---------------------------------------------------------------------------
# 4. Build Nexus
# ---------------------------------------------------------------------------
NEXUS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ ! -f "$NEXUS_DIR/Cargo.toml" ]; then
    error "Cannot find Cargo.toml. Run this script from the nexus-rust directory."
fi

info "Building Nexus (release mode)..."
cd "$NEXUS_DIR"
cargo build --release --package nexus-http --package nexus-cli 2>&1 | tail -5

NEXUS_BIN="$NEXUS_DIR/target/release"

if [ ! -f "$NEXUS_BIN/nexus-server" ] && [ ! -f "$NEXUS_BIN/nexus" ]; then
    error "Build failed — binaries not found in $NEXUS_BIN"
fi

info "Build complete."

# ---------------------------------------------------------------------------
# 5. Install to PATH
# ---------------------------------------------------------------------------
INSTALL_DIR="$HOME/.nexus/bin"
mkdir -p "$INSTALL_DIR"

# Copy binaries
[ -f "$NEXUS_BIN/nexus-server" ] && cp "$NEXUS_BIN/nexus-server" "$INSTALL_DIR/"
[ -f "$NEXUS_BIN/nexus" ] && cp "$NEXUS_BIN/nexus" "$INSTALL_DIR/"

# Add to PATH if not already there
SHELL_RC=""
case "$SHELL" in
    */zsh)  SHELL_RC="$HOME/.zshrc" ;;
    */bash) SHELL_RC="$HOME/.bashrc" ;;
    *)      SHELL_RC="$HOME/.profile" ;;
esac

if ! grep -q "$INSTALL_DIR" "$SHELL_RC" 2>/dev/null; then
    echo "" >> "$SHELL_RC"
    echo "# Nexus" >> "$SHELL_RC"
    echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_RC"
    info "Added $INSTALL_DIR to PATH in $SHELL_RC"
fi

# Create data directory
mkdir -p "$HOME/.nexus"

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}${GREEN}Nexus installed successfully!${NC}"
echo ""
echo "  Binaries:  $INSTALL_DIR/"
echo "  Data dir:  ~/.nexus/"
echo ""
echo "  Quick start:"
echo "    nexus-server                                         # Start the API server (port 8020)"
echo "    nexus generate \"Build a task manager with React\"      # Generate an app from a prompt"
echo "    nexus projects                                       # List generated projects"
echo "    nexus --help                                         # See all commands"
echo ""
echo "  Web UI:"
echo "    cd web && npm install && npm run dev    # http://localhost:8080 (dev)"
echo ""
echo -e "  ${YELLOW}Restart your shell or run: source $SHELL_RC${NC}"
echo ""
