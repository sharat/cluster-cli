# Publishing Guide

This guide covers how to publish `cluster-cli` to Cargo (crates.io) and Homebrew.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Publishing to Cargo (crates.io)](#publishing-to-cargo-crateio)
- [Publishing to Homebrew](#publishing-to-homebrew)
- [Automated Release Process](#automated-release-process)

---

## Prerequisites

Before publishing, ensure you have:

1. **Cargo account**: Sign up at [crates.io](https://crates.io) and get an API token
2. **Homebrew tap repository**: Create a separate repo (e.g., `sharat/homebrew-tap`)
3. **GitHub token**: For automated releases (already configured in GitHub Actions)

---

## Publishing to Cargo (crates.io)

### Step 1: Prepare Your Package

Ensure your `Cargo.toml` has all required metadata:

```toml
[package]
name = "cluster-cli"
version = "0.1.0"
edition = "2021"
authors = ["Your Name <your.email@example.com>"]
description = "A fast, interactive terminal UI for monitoring Kubernetes cluster health"
readme = "README.md"
license = "MIT"
repository = "https://github.com/sharat/cluster-cli"
homepage = "https://github.com/sharat/cluster-cli"
keywords = ["kubernetes", "k8s", "monitoring", "tui", "terminal"]
categories = ["command-line-utilities", "development-tools"]
rust-version = "1.70"
```

### Step 2: Test Before Publishing

```bash
# Verify the package builds
cargo build --release

# Check for any issues
cargo check
cargo clippy -- -D warnings
cargo fmt --check

# Test the package
cargo test

# Dry run publish (checks for issues without uploading)
cargo publish --dry-run
```

### Step 3: Login to crates.io

```bash
# Get your API token from https://crates.io/settings/tokens
cargo login
# Paste your API token when prompted
```

### Step 4: Publish

```bash
# Publish to crates.io
cargo publish

# Or use the justfile command (if added)
just publish-cargo
```

### Step 5: Verify

```bash
# Check your crate is live
cargo search cluster-cli

# Install from crates.io
cargo install cluster-cli
```

---

## Publishing to Homebrew

### Step 1: Create a Homebrew Tap Repository

Create a new repository on GitHub:
- Name: `homebrew-tap` (or any name, but `homebrew-tap` is conventional)
- URL: `https://github.com/sharat/homebrew-tap`

### Step 2: Create the Formula

In your `homebrew-tap` repo, create `Formula/cluster-cli.rb`:

```ruby
class ClusterCli < Formula
  desc "Fast, interactive terminal UI for monitoring Kubernetes cluster health"
  homepage "https://github.com/sharat/cluster-cli"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/sharat/cluster-cli/releases/download/v0.1.0/cluster-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_ARM64"
    else
      url "https://github.com/sharat/cluster-cli/releases/download/v0.1.0/cluster-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/sharat/cluster-cli/releases/download/v0.1.0/cluster-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  depends_on "kubectl"

  def install
    bin.install "cluster"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cluster --version 2>&1 || true")
  end
end
```

### Step 3: Calculate SHA256 Hashes

After creating a GitHub release, download each binary and calculate SHA256:

```bash
# Download release artifacts
curl -L -o cluster-x86_64-unknown-linux-gnu.tar.gz \
  https://github.com/sharat/cluster-cli/releases/download/v0.1.0/cluster-x86_64-unknown-linux-gnu.tar.gz

curl -L -o cluster-aarch64-apple-darwin.tar.gz \
  https://github.com/sharat/cluster-cli/releases/download/v0.1.0/cluster-aarch64-apple-darwin.tar.gz

curl -L -o cluster-x86_64-pc-windows-msvc.zip \
  https://github.com/sharat/cluster-cli/releases/download/v0.1.0/cluster-x86_64-pc-windows-msvc.zip

# Calculate SHA256
shasum -a 256 cluster-x86_64-unknown-linux-gnu.tar.gz
shasum -a 256 cluster-aarch64-apple-darwin.tar.gz
shasum -a 256 cluster-x86_64-pc-windows-msvc.zip
```

Update the formula with the correct SHA256 values.

### Step 4: Commit and Push

```bash
cd homebrew-tap
git add Formula/cluster-cli.rb
git commit -m "Add cluster-cli formula v0.1.0"
git push origin main
```

### Step 5: Users Can Now Install

```bash
# Add your tap
brew tap sharat/tap

# Install cluster-cli
brew install cluster-cli

# Or install directly without tapping
brew install sharat/tap/cluster-cli
```

---

## Automated Release Process

### Option 1: Manual Release (Current)

```bash
# 1. Update version in Cargo.toml
# 2. Commit changes
git add .
git commit -m "chore: bump version to 0.1.0"

# 3. Create and push tag
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# 4. GitHub Actions automatically builds and creates release

# 5. After release is created, update Homebrew formula with SHA256 hashes

# 6. Publish to Cargo (optional - can be done before or after GitHub release)
cargo publish
```

### Option 2: Using Justfile

```bash
# Interactive release (bumps version, creates tag, pushes)
just release

# Or specify version type
just release patch   # 0.1.0 -> 0.1.1
just release minor   # 0.1.0 -> 0.2.0
just release major   # 0.1.0 -> 1.0.0

# Publish current version without bumping
just publish

# Auto-confirm (no prompts)
just release patch --yes
just publish --yes
```

### Option 3: Fully Automated (Recommended Enhancement)

Add a GitHub Action to automatically update the Homebrew tap:

Create `.github/workflows/update-homebrew.yml`:

```yaml
name: Update Homebrew Formula

on:
  release:
    types: [published]

jobs:
  update-homebrew:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout homebrew-tap repo
        uses: actions/checkout@v4
        with:
          repository: sharat/homebrew-tap
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
          
      - name: Download release artifacts and calculate SHA256
        run: |
          VERSION=${{ github.event.release.tag_name }}
          VERSION=${VERSION#v}
          
          # Download and calculate hashes
          curl -L -o linux.tar.gz https://github.com/sharat/cluster-cli/releases/download/${{ github.event.release.tag_name }}/cluster-x86_64-unknown-linux-gnu.tar.gz
          curl -L -o macos.tar.gz https://github.com/sharat/cluster-cli/releases/download/${{ github.event.release.tag_name }}/cluster-aarch64-apple-darwin.tar.gz
          
          LINUX_SHA=$(shasum -a 256 linux.tar.gz | cut -d' ' -f1)
          MACOS_SHA=$(shasum -a 256 macos.tar.gz | cut -d' ' -f1)
          
          # Update formula
          sed -i "s/version \".*\"/version \"$VERSION\"/" Formula/cluster-cli.rb
          sed -i "s/PLACEHOLDER_SHA256_LINUX/$LINUX_SHA/" Formula/cluster-cli.rb
          sed -i "s/PLACEHOLDER_SHA256_MACOS/$MACOS_SHA/" Formula/cluster-cli.rb
          
      - name: Commit and push
        run: |
          git config user.name "GitHub Actions"
          git config user.email "actions@github.com"
          git add Formula/cluster-cli.rb
          git commit -m "Update cluster-cli to ${{ github.event.release.tag_name }}"
          git push
```

---

## Quick Reference

### Cargo Commands

```bash
cargo login                    # Login to crates.io
cargo publish --dry-run       # Test publish
cargo publish                 # Publish to crates.io
cargo search cluster-cli      # Verify published
cargo install cluster-cli     # Install from crates.io
```

### Homebrew Commands

```bash
# For maintainers
brew tap sharat/tap
brew install cluster-cli

# For users
brew tap sharat/tap
brew install cluster-cli

# Upgrade
brew upgrade cluster-cli

# Uninstall
brew uninstall cluster-cli
```

---

## Troubleshooting

### Cargo Issues

**Error: "already uploaded"**
- You cannot re-publish the same version. Bump the version in `Cargo.toml`.

**Error: "crate name already taken"**
- The name `cluster-cli` might be taken. Check with `cargo search cluster-cli`.

**Error: "missing metadata"**
- Ensure all required fields are in `Cargo.toml` (description, license, repository, etc.)

### Homebrew Issues

**Error: "SHA256 mismatch"**
- Update the SHA256 in the formula with the correct value from the release.

**Error: "Formula not found"**
- Ensure the tap is added: `brew tap sharat/tap`

---

## Next Steps

1. ✅ Create a crates.io account and get API token
2. ✅ Create `sharat/homebrew-tap` repository
3. ✅ Run `cargo publish --dry-run` to verify
4. ✅ Create first GitHub release (triggers automatically on tag)
5. ✅ Update Homebrew formula with SHA256 hashes
6. ✅ Publish to Cargo with `cargo publish`
7. ✅ Test installation: `cargo install cluster-cli` and `brew install sharat/tap/cluster-cli`

---

## Resources

- [Cargo Publishing Guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Homebrew Tap Guide](https://docs.brew.sh/Taps)
