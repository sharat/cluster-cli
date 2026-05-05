class ClusterCli < Formula
  desc "Fast, interactive terminal UI for monitoring Kubernetes cluster health"
  homepage "https://github.com/sharat/cluster-cli"
  version "0.1.4"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/sharat/cluster-cli/releases/download/v#{version}/cluster-aarch64-apple-darwin.tar.gz"
      sha256 "27d11ce25b524eb579469814aec1f90bc2838f396241d5dbe3d75bb1c6cd6919"
    else
      odie "cluster-cli only supports Apple Silicon (ARM64). Intel Macs are not supported."
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/sharat/cluster-cli/releases/download/v#{version}/cluster-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "31daed17acac88cf687b98993b8bd0eead995701efde934fcef04b62e081ced4"
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
