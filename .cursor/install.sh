#!/usr/bin/env bash
# Idempotent Cloud Agent bootstrap for cluster-cli.
# Safe to run repeatedly: it only installs what is missing and warms the build cache.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Rust toolchain"
# rust-toolchain.toml pins the channel; this materializes it (no-op if already present).
rustc --version
cargo --version

echo "==> kubectl (runtime dependency of the TUI)"
if command -v kubectl >/dev/null 2>&1; then
  echo "kubectl already installed: $(kubectl version --client 2>/dev/null | head -1)"
else
  kver="$(curl -fsSL https://dl.k8s.io/release/stable.txt)"
  arch="$(uname -m)"
  case "$arch" in
    x86_64) karch=amd64 ;;
    aarch64 | arm64) karch=arm64 ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
  esac
  tmp="$(mktemp)"
  curl -fsSLo "$tmp" "https://dl.k8s.io/release/${kver}/bin/linux/${karch}/kubectl"
  if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    sudo install -m 0755 "$tmp" /usr/local/bin/kubectl
  else
    mkdir -p "$HOME/.local/bin"
    install -m 0755 "$tmp" "$HOME/.local/bin/kubectl"
    echo "Installed kubectl to ~/.local/bin (ensure it is on PATH)."
  fi
  rm -f "$tmp"
  echo "kubectl installed: $(kubectl version --client 2>/dev/null | head -1)"
fi

echo "==> Fetch Rust dependencies"
cargo fetch --locked

echo "==> Build (warms the compilation cache)"
cargo build

echo "==> Bootstrap complete"
