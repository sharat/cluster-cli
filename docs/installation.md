# Installation

## Prerequisites

- **kubectl** configured with cluster access
- **Terminal** with truecolor support (recommended)
- **Rust 1.85+** (only for building from source)

## Quick Install (Recommended)

Install the latest release using the installer script:

```bash
curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | bash
```

This installs to `~/.local/bin` by default (no sudo required) and records the installation method for future upgrades.

### Custom Install Directory

```bash
curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | INSTALL_DIR=$HOME/.local/bin bash
```

The installer will:
1. Detect your platform (Linux x86_64, macOS ARM64, Windows x86_64)
2. Download the latest release binary
3. Verify SHA256 checksum
4. Install to the specified directory
5. Record the install method for `cluster --upgrade`

## Build from Source

### Clone and Build

```bash
git clone https://github.com/sharat/cluster-cli.git
cd cluster-cli
cargo build --release
```

The binary will be available at `target/release/cluster`.

### Install Locally

```bash
cargo install --path .
```

This installs to `~/.cargo/bin/cluster`.

### Development Build

```bash
cargo build
cargo run
```

Development builds are not optimized and may be slower.

## Release Builds

Release binaries are optimized with:
- `opt-level = "z"` (size optimization)
- `lto = true` (link-time optimization)
- `codegen-units = 1` (better optimization)
- `panic = "abort"` (smaller binary)
- `strip = true` (debug symbols removed)

## Platform Support

### Officially Supported

Pre-built binaries are available for:

- **Linux x86_64** (GNU libc)
- **macOS ARM64** (Apple Silicon)
- **Windows x86_64** (64-bit)

### Building for Other Platforms

cluster-cli should compile on any platform supported by Rust and its dependencies. Cross-compilation is possible using [cross](https://github.com/cross-rs/cross):

```bash
cargo install cross
cross build --release --target <target-triple>
```

## Verifying Installation

After installation, verify cluster-cli works:

```bash
cluster --version
cluster --info
```

The `--info` flag shows:
- Version number
- Installation method
- Config directory location

## Updating

### Automatic Upgrade

```bash
cluster --upgrade
```

This uses the detected installation method (curl, homebrew, or cargo) to upgrade to the latest release.

### Check for Updates

```bash
cluster --check-update
```

Shows available updates without installing.

### Manual Update

Download the latest release from [GitHub Releases](https://github.com/sharat/cluster-cli/releases) and replace your existing binary.

## Uninstalling

### curl-based installation

```bash
rm ~/.local/bin/cluster
rm -rf ~/.config/cluster-cli/
```

### Cargo-based installation

```bash
cargo uninstall cluster-cli
rm -rf ~/.config/cluster-cli/
```

## Configuration Directory

cluster-cli stores configuration at:

- **Linux/macOS**: `~/.config/cluster-cli/`
- **Windows**: `%APPDATA%\cluster-cli\`

No files are created until you modify settings or the app saves install metadata.

## Next Steps

- [Quick Start Guide](quick-start.md)
- [Configuration](configuration.md)
- [User Interface Guide](ui.md)
