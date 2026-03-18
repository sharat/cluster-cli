class ClusterCli < Formula
  desc "Fast, interactive terminal UI for monitoring Kubernetes cluster health"
  homepage "https://github.com/sharat/cluster-cli"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/sharat/cluster-cli/releases/download/v#{version}/cluster-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_ARM64"
    else
      url "https://github.com/sharat/cluster-cli/releases/download/v#{version}/cluster-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/sharat/cluster-cli/releases/download/v#{version}/cluster-x86_64-unknown-linux-gnu.tar.gz"
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
