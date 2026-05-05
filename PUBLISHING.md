# Publishing Guide

This guide covers how to publish `cluster-cli` via GitHub Releases and Homebrew.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Publishing to Homebrew](#publishing-to-homebrew)
- [Automated Release Process](#automated-release-process)

---

## Prerequisites

Before publishing, ensure you have:

1. **Homebrew tap repository**: Create a separate repo (e.g., `sharat/homebrew-tap`)
2. **GitHub token**: For automated releases (already configured in GitHub Actions)

---

## crates.io

`cluster-cli` is not currently published to crates.io. Users should install via GitHub Release binaries, `install.sh`, or Homebrew.

If crates.io distribution is added later, restore a `cargo publish --dry-run`/`cargo publish` step and configure a `CARGO_REGISTRY_TOKEN` secret.

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

# 4. GitHub Actions automatically builds binaries and creates the GitHub release

# 5. After release is created, update Homebrew formula with SHA256 hashes
```

### Option 2: Using Justfile

```bash
# Interactive release (bumps version, creates tag, pushes)
just release

# Or specify version type
just release patch   # 0.1.0 -> 0.1.1
just release minor   # 0.1.0 -> 0.2.0
just release major   # 0.1.0 -> 1.0.0

# Push a tag for the current version without bumping
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

Cargo is used for local build verification only; this project is not currently published to crates.io.

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings
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

### Homebrew Issues

**Error: "SHA256 mismatch"**
- Update the SHA256 in the formula with the correct value from the release.

**Error: "Formula not found"**
- Ensure the tap is added: `brew tap sharat/tap`

---

## Next Steps

1. ✅ Create `sharat/homebrew-tap` repository
2. ✅ Create first GitHub release (triggers automatically on tag)
3. ✅ Update Homebrew formula with SHA256 hashes
4. ✅ Test installation: `install.sh` and `brew install sharat/tap/cluster-cli`

---

## Resources

- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Homebrew Tap Guide](https://docs.brew.sh/Taps)
