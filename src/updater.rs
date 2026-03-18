use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

const GITHUB_API_URL: &str = "https://api.github.com/repos/sharat/cluster-cli/releases/latest";
const INSTALL_METHOD_FILE: &str = "install_method";

#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub published_at: String,
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
            _ => Some(InstallMethod::Unknown),
        }
    }
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
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cluster-rs")
            .join(INSTALL_METHOD_FILE)
    }

    /// Store the installation method for future reference
    pub fn store_install_method(method: InstallMethod) -> Result<()> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cluster-rs");
        
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
            InstallMethod::Homebrew => {
                self.upgrade_via_homebrew().await
            }
            InstallMethod::Curl | InstallMethod::Unknown => {
                self.upgrade_via_curl().await
            }
            InstallMethod::Cargo => {
                self.upgrade_via_cargo().await
            }
        }
    }

    async fn upgrade_via_homebrew(&self) -> Result<()> {
        println!("Upgrading cluster-cli via Homebrew...");
        
        let output = Command::new("brew")
            .args(&["upgrade", "cluster-cli"])
            .output()
            .map_err(|e| anyhow!("Failed to run brew upgrade: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Homebrew upgrade failed: {}", stderr));
        }

        println!("✅ Upgrade successful!");
        Ok(())
    }

    async fn upgrade_via_curl(&self) -> Result<()> {
        println!("Upgrading cluster-cli via install script...");
        
        let install_script = r#"curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | bash"#;
        
        let output = Command::new("bash")
            .arg("-c")
            .arg(install_script)
            .output()
            .map_err(|e| anyhow!("Failed to run install script: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Install script failed: {}", stderr));
        }

        println!("✅ Upgrade successful!");
        println!("The new version will be available when you next run cluster.");
        Ok(())
    }

    async fn upgrade_via_cargo(&self) -> Result<()> {
        println!("Upgrading cluster-cli via Cargo...");
        
        let output = Command::new("cargo")
            .args(&["install", "cluster-cli"])
            .output()
            .map_err(|e| anyhow!("Failed to run cargo install: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Cargo install failed: {}", stderr));
        }

        println!("✅ Upgrade successful!");
        Ok(())
    }

    /// Show current installation info
    pub fn show_info(&self) {
        println!("cluster-cli {}", self.current_version);
        println!("Installation method: {}", self.install_method.as_str());
        println!("Config directory: {}", 
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("cluster-rs")
                .display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_method_from_str() {
        assert_eq!(InstallMethod::from_str("curl"), Some(InstallMethod::Curl));
        assert_eq!(InstallMethod::from_str("homebrew"), Some(InstallMethod::Homebrew));
        assert_eq!(InstallMethod::from_str("cargo"), Some(InstallMethod::Cargo));
        assert_eq!(InstallMethod::from_str("unknown"), Some(InstallMethod::Unknown));
    }
}
