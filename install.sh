#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REPO="sharat/cluster-cli"
BINARY_NAME="cluster"
# Default to user-local bin directory to avoid sudo
INSTALL_DIR="${HOME}/.local/bin"

# Print error and exit
error() {
    echo -e "${RED}Error: $1${NC}" >&2
    exit 1
}

# Print success message
success() {
    echo -e "${GREEN}$1${NC}"
}

# Print info message
info() {
    echo -e "${YELLOW}$1${NC}"
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux";;
        Darwin*)    echo "darwin";;
        CYGWIN*|MINGW*|MSYS*) echo "windows";;
        *)          error "Unsupported operating system: $(uname -s)";;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64";;
        arm64|aarch64) echo "aarch64";;
        *)          error "Unsupported architecture: $(uname -m)";;
    esac
}

# Get latest release version
get_latest_version() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

verify_checksum() {
    local file="$1"
    local checksum_file="$2"
    local expected
    local actual

    expected=$(grep -Eo '[a-fA-F0-9]{64}' "$checksum_file" | head -1 | tr 'A-F' 'a-f')
    if [ -z "$expected" ]; then
        error "Could not parse checksum from $checksum_file"
    fi

    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    elif command -v certutil >/dev/null 2>&1; then
        actual=$(certutil -hashfile "$file" SHA256 | grep -Eo '[a-fA-F0-9]{64}' | head -1 | tr 'A-F' 'a-f')
    else
        error "No SHA256 tool found (need sha256sum, shasum, or certutil)"
    fi

    if [ "$actual" != "$expected" ]; then
        error "Checksum mismatch for $file"
    fi

    success "✓ Checksum verified"
}

# Main installation
main() {
    info "Installing cluster-cli..."
    
    # Detect platform
    OS=$(detect_os)
    ARCH=$(detect_arch)
    
    info "Detected: $OS/$ARCH"
    
    # Get latest version
    VERSION=$(get_latest_version)
    if [ -z "$VERSION" ]; then
        error "Could not determine latest version"
    fi
    
    info "Latest version: $VERSION"
    
    # Determine download URL and filename
    if [ "$OS" = "windows" ]; then
        FILENAME="cluster-x86_64-pc-windows-msvc.zip"
        BINARY_NAME="cluster.exe"
    elif [ "$OS" = "darwin" ]; then
        if [ "$ARCH" = "aarch64" ]; then
            FILENAME="cluster-aarch64-apple-darwin.tar.gz"
        else
            error "macOS Intel (x86_64) binaries are not published yet. Build from source with: cargo install cluster-cli"
        fi
    else
        FILENAME="cluster-x86_64-unknown-linux-gnu.tar.gz"
    fi
    
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$FILENAME"
    CHECKSUM_URL="$DOWNLOAD_URL.sha256"
    
    # Create temporary directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT
    
    # Download
    info "Downloading from: $DOWNLOAD_URL"
    if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$FILENAME"; then
        error "Failed to download $FILENAME"
    fi

    info "Downloading checksum..."
    if ! curl -fsSL "$CHECKSUM_URL" -o "$TMP_DIR/$FILENAME.sha256"; then
        error "Failed to download checksum for $FILENAME"
    fi

    verify_checksum "$TMP_DIR/$FILENAME" "$TMP_DIR/$FILENAME.sha256"
    
    # Extract
    info "Extracting..."
    cd "$TMP_DIR"
    if [ "${FILENAME##*.}" = "zip" ]; then
        unzip -q "$FILENAME"
    else
        tar -xzf "$FILENAME"
    fi
    
    # Create install directory if it doesn't exist
    if [ ! -d "$INSTALL_DIR" ]; then
        info "Creating install directory $INSTALL_DIR..."
        mkdir -p "$INSTALL_DIR"
    fi
    
    # Check if we need sudo (shouldn't for ~/.local/bin)
    if [ -w "$INSTALL_DIR" ]; then
        SUDO=""
    else
        info "Installing to $INSTALL_DIR requires sudo privileges"
        SUDO="sudo"
    fi
    
    # Install binary
    info "Installing binary to $INSTALL_DIR..."
    if ! $SUDO mv "$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"; then
        error "Failed to install binary to $INSTALL_DIR"
    fi
    
    # Make executable (not needed for Windows but harmless)
    if [ "$OS" != "windows" ]; then
        $SUDO chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi
    
    # Verify installation
    if command -v "$BINARY_NAME" &> /dev/null; then
        success "✓ cluster-cli installed successfully!"
        echo ""
        echo "Run 'cluster --help' to get started"
        echo ""
        echo "To uninstall: rm $INSTALL_DIR/$BINARY_NAME"
    else
        success "✓ cluster-cli installed to $INSTALL_DIR"
        echo ""
        echo "⚠️  $INSTALL_DIR is not in your PATH"
        echo ""
        echo "Add this to your shell configuration file:"
        echo ""
        if [ -n "$ZSH_VERSION" ]; then
            echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
            echo "  source ~/.zshrc"
        else
            echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
            echo "  source ~/.bashrc"
        fi
        echo ""
        echo "Then run: cluster --help"
    fi
}

# Allow overriding install directory
while [[ $# -gt 0 ]]; do
    case $1 in
        --install-dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --install-dir DIR    Install to custom directory (default: ~/.local/bin)"
            echo "  --help, -h          Show this help message"
            echo ""
            echo "Environment variables:"
            echo "  INSTALL_DIR         Override install directory"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Allow INSTALL_DIR from environment
if [ -n "$INSTALL_DIR" ]; then
    INSTALL_DIR="$INSTALL_DIR"
fi

main
