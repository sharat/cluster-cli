use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

const GITHUB_API_URL: &str = "https://api.github.com/repos/sharat/cluster-cli/releases/latest";
const INSTALL_SCRIPT_URL: &str =
    "https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh";
const INSTALL_METHOD_FILE: &str = "install_method";

/// Cap on how long any upgrade subprocess may run before it is killed. Generous
/// because `cargo install` compiles from source, but bounded so a wedged
/// `brew`/`curl` can't hang the process forever.
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(900);

/// Cap on the install-script download.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// A trusted SHA-256 (hex) for the install script. Upgrades fail closed unless
/// the caller supplies this out-of-band value.
const INSTALL_SHA256_ENV: &str = "CLUSTER_INSTALL_SHA256";

/// Run a subprocess to completion under `UPGRADE_TIMEOUT`, surfacing stderr on
/// failure. `kill_on_drop` means a timeout actually terminates the child rather
/// than leaking it, since dropping the future drops the `Child`.
async fn run_checked(label: &str, mut cmd: Command) -> Result<()> {
    cmd.kill_on_drop(true);

    let output = tokio::time::timeout(UPGRADE_TIMEOUT, cmd.output())
        .await
        .map_err(|_| {
            anyhow!(
                "{label} timed out after {}s and was terminated",
                UPGRADE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| anyhow!("Failed to run {label}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("{label} failed: {}", stderr.trim()));
    }

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InstallMethod {
    Curl,
    Homebrew,
    Cargo,
    Unknown,
}

impl InstallMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallMethod::Curl => "curl",
            InstallMethod::Homebrew => "homebrew",
            InstallMethod::Cargo => "cargo",
            InstallMethod::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "curl" => Some(InstallMethod::Curl),
            "homebrew" => Some(InstallMethod::Homebrew),
            "cargo" => Some(InstallMethod::Cargo),
            "unknown" => Some(InstallMethod::Unknown),
            // Anything else is a corrupt/stale file. Return None so the caller
            // falls back to path-based detection instead of silently pinning
            // the method to Unknown.
            _ => None,
        }
    }
}

/// Lowercase hex SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn parse_install_digest(digest: &str) -> Result<String> {
    let digest = digest.trim().to_ascii_lowercase();

    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow!(
            "{INSTALL_SHA256_ENV} must be a 64-character hexadecimal SHA-256 digest"
        ));
    }

    Ok(digest)
}

/// Stage an install script in the user-owned config directory without ever
/// following an existing path. `create_new` rejects a pre-existing symlink,
/// preventing a local path redirection from clobbering an arbitrary file.
fn stage_install_script(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn expected_install_digest() -> Result<String> {
    let digest = std::env::var(INSTALL_SHA256_ENV).with_context(|| {
        format!(
            "Refusing to run an unverified install script: set {INSTALL_SHA256_ENV} to a trusted SHA-256"
        )
    })?;
    parse_install_digest(&digest)
}

pub struct Updater {
    current_version: String,
    install_method: InstallMethod,
}

impl Updater {
    pub fn new(current_version: &str) -> Self {
        let install_method = Self::detect_install_method();
        Self {
            current_version: current_version.to_string(),
            install_method,
        }
    }

    /// Detect how the binary was installed
    fn detect_install_method() -> InstallMethod {
        // First check if we have a stored install method
        if let Some(method) = Self::get_stored_install_method() {
            return method;
        }

        // Detect based on binary location
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_str = exe_path.to_string_lossy();

            // Check if installed via Homebrew
            if exe_str.contains("/homebrew/") || exe_str.contains("/Cellar/") {
                return InstallMethod::Homebrew;
            }

            // Check if installed via cargo
            if exe_str.contains("/.cargo/") || exe_str.contains("cargo/") {
                return InstallMethod::Cargo;
            }

            // Check if in user local bin (curl install)
            if exe_str.contains("/.local/bin/") || exe_str.contains("/usr/local/bin/") {
                return InstallMethod::Curl;
            }
        }

        InstallMethod::Unknown
    }

    fn get_stored_install_method() -> Option<InstallMethod> {
        let path = Self::install_method_file_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            return InstallMethod::from_str(content.trim());
        }
        None
    }

    fn install_method_file_path() -> PathBuf {
        crate::config::Config::dir_path().join(INSTALL_METHOD_FILE)
    }

    /// Store the installation method for future reference
    pub fn store_install_method(method: InstallMethod) -> Result<()> {
        let config_dir = crate::config::Config::dir_path();

        std::fs::create_dir_all(&config_dir)?;

        let path = config_dir.join(INSTALL_METHOD_FILE);
        std::fs::write(&path, method.as_str())?;

        Ok(())
    }

    /// Check if an update is available
    pub async fn check_update(&self) -> Result<Option<GithubRelease>> {
        let client = reqwest::Client::builder()
            .user_agent("cluster-cli")
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        let release: GithubRelease = client
            .get(GITHUB_API_URL)
            .send()
            .await
            .context("Failed to fetch latest release")?
            .json()
            .await
            .context("Failed to parse release JSON")?;

        let latest_version = release.tag_name.trim_start_matches('v');
        let current_version = self.current_version.trim_start_matches('v');

        // Compare versions using semver
        if let (Ok(latest), Ok(current)) = (
            semver::Version::parse(latest_version),
            semver::Version::parse(current_version),
        ) {
            if latest > current {
                return Ok(Some(release));
            }
        }

        Ok(None)
    }

    /// Get update notification message for display
    pub fn get_update_notification(&self, release: &GithubRelease) -> String {
        let latest = &release.tag_name;
        let current = &self.current_version;

        match self.install_method {
            InstallMethod::Homebrew => {
                format!(
                    "Update available: {} → {}. Run 'brew upgrade cluster-cli' or 'cluster --upgrade'",
                    current, latest
                )
            }
            InstallMethod::Curl | InstallMethod::Unknown => {
                format!(
                    "Update available: {} → {}. Run 'cluster --upgrade' to update",
                    current, latest
                )
            }
            InstallMethod::Cargo => {
                format!(
                    "Update available: {} → {}. Run 'cargo install cluster-cli' to update",
                    current, latest
                )
            }
        }
    }

    /// Perform the upgrade
    pub async fn upgrade(&self) -> Result<()> {
        match self.install_method {
            InstallMethod::Homebrew => self.upgrade_via_homebrew().await?,
            InstallMethod::Curl | InstallMethod::Unknown => self.upgrade_via_curl().await?,
            InstallMethod::Cargo => self.upgrade_via_cargo().await?,
        }

        // Persist the method now that it's proven to work, so later runs use it
        // directly instead of re-deriving it from the binary's path. Best-effort:
        // a successful upgrade shouldn't be reported as a failure just because
        // the config directory isn't writable.
        if self.install_method != InstallMethod::Unknown {
            if let Err(e) = Self::store_install_method(self.install_method) {
                eprintln!("Note: could not record install method: {e}");
            }
        }

        Ok(())
    }

    async fn upgrade_via_homebrew(&self) -> Result<()> {
        println!("Upgrading cluster-cli via Homebrew...");

        let mut cmd = Command::new("brew");
        cmd.args(["upgrade", "cluster-cli"]);
        run_checked("Homebrew upgrade", cmd).await?;

        println!("✅ Upgrade successful!");
        Ok(())
    }

    /// Upgrade via the install script.
    ///
    /// Deliberately *not* `curl … | bash`. Piping straight into a shell executes
    /// whatever bytes have arrived so far, so a connection that drops mid-transfer
    /// runs a truncated script. Instead the script is downloaded in full to a
    /// temp file, checked, optionally verified against a pinned digest, and only
    /// then executed. The expected digest is supplied out of band via
    /// `CLUSTER_INSTALL_SHA256`; without it this operation fails closed.
    async fn upgrade_via_curl(&self) -> Result<()> {
        println!("Upgrading cluster-cli via install script...");

        let client = reqwest::Client::builder()
            .user_agent("cluster-cli")
            .timeout(DOWNLOAD_TIMEOUT)
            .https_only(true)
            .build()?;

        let script = client
            .get(INSTALL_SCRIPT_URL)
            .send()
            .await
            .context("Failed to download install script")?
            .error_for_status()
            .context("Install script download returned an error status")?
            .text()
            .await
            .context("Failed to read install script body")?;

        if script.trim().is_empty() {
            return Err(anyhow!("Install script was empty — refusing to execute"));
        }

        let digest = sha256_hex(script.as_bytes());

        let expected = expected_install_digest()?;
        if expected != digest {
            return Err(anyhow!(
                "Install script digest mismatch — refusing to execute.\n  expected: {expected}\n  actual:   {digest}"
            ));
        }
        println!("Install script digest verified against {INSTALL_SHA256_ENV}.");

        // The config directory is user-owned, unlike the system temp directory.
        // `create_new` rejects a pre-created file or symlink rather than opening
        // it, so the verified script cannot be redirected to another path.
        let config_dir = crate::config::Config::dir_path();
        std::fs::create_dir_all(&config_dir).context("Failed to create config directory")?;
        let script_path = config_dir.join(format!(
            "install-script-{}-{}.sh",
            &digest[..16],
            std::process::id()
        ));
        stage_install_script(&script_path, script.as_bytes()).with_context(|| {
            format!(
                "Failed to securely stage install script at {}",
                script_path.display()
            )
        })?;

        let mut cmd = Command::new("bash");
        cmd.arg(&script_path);
        let result = run_checked("Install script", cmd).await;
        let _ = std::fs::remove_file(&script_path);
        result?;

        println!("✅ Upgrade successful!");
        println!("The new version will be available when you next run cluster.");
        Ok(())
    }

    async fn upgrade_via_cargo(&self) -> Result<()> {
        println!("Upgrading cluster-cli via Cargo...");

        let mut cmd = Command::new("cargo");
        cmd.args(["install", "cluster-cli"]);
        run_checked("Cargo install", cmd).await?;

        println!("✅ Upgrade successful!");
        Ok(())
    }

    /// Show current installation info
    pub fn show_info(&self) {
        println!("cluster-cli {}", self.current_version);
        println!("Installation method: {}", self.install_method.as_str());
        println!(
            "Config directory: {}",
            crate::config::Config::dir_path().display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_method_from_str() {
        assert_eq!(InstallMethod::from_str("curl"), Some(InstallMethod::Curl));
        assert_eq!(
            InstallMethod::from_str("homebrew"),
            Some(InstallMethod::Homebrew)
        );
        assert_eq!(InstallMethod::from_str("cargo"), Some(InstallMethod::Cargo));
        assert_eq!(
            InstallMethod::from_str("unknown"),
            Some(InstallMethod::Unknown)
        );
    }

    #[test]
    fn install_method_from_str_rejects_unrecognized_values() {
        // A corrupt or stale file must not silently read as `Unknown` — None
        // lets the caller fall back to path-based detection.
        assert_eq!(InstallMethod::from_str(""), None);
        assert_eq!(InstallMethod::from_str("apt"), None);
        assert_eq!(InstallMethod::from_str("garbage"), None);
    }

    #[test]
    fn install_method_round_trips_through_as_str() {
        for method in [
            InstallMethod::Curl,
            InstallMethod::Homebrew,
            InstallMethod::Cargo,
            InstallMethod::Unknown,
        ] {
            assert_eq!(InstallMethod::from_str(method.as_str()), Some(method));
        }
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // Standard SHA-256 test vectors.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parse_install_digest_accepts_hex_and_rejects_invalid_values() {
        let valid = "A".repeat(64);
        assert_eq!(parse_install_digest(&valid).unwrap(), "a".repeat(64));
        assert!(parse_install_digest("not-a-digest").is_err());
        assert!(parse_install_digest(&"a".repeat(63)).is_err());
    }
}
