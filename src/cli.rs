use clap::{Parser, Subcommand};

use crate::config::ConfigOverrides;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "cluster")]
#[command(about = "A fast, interactive terminal UI for monitoring Kubernetes cluster health")]
#[command(version = VERSION)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub config: ConfigOverrides,

    /// Upgrade cluster-cli in place using the method it was installed with
    #[arg(long, global = true)]
    pub upgrade: bool,

    /// Check whether a newer release is available, without installing it
    #[arg(long, global = true)]
    pub check_update: bool,

    /// Show version, detected install method, and config directory
    #[arg(long, global = true)]
    pub info: bool,

    /// Record the installer used by this binary
    #[arg(
        long,
        hide = true,
        value_name = "METHOD",
        value_parser = ["curl", "homebrew", "cargo"],
        global = true
    )]
    pub record_install_method: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Get cluster health status with optional JSON output
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Include detailed findings (with --json)
        #[arg(long)]
        verbose: bool,

        #[command(flatten)]
        kubectl_opts: KubectlOptions,
    },

    /// List cluster incidents
    Incidents {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Include all incidents instead of top 10
        #[arg(long)]
        verbose: bool,

        #[command(flatten)]
        kubectl_opts: KubectlOptions,
    },

    /// List pods in the cluster
    Pods {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Include container details (with --json)
        #[arg(long)]
        verbose: bool,

        #[command(flatten)]
        kubectl_opts: KubectlOptions,
    },

    /// List nodes in the cluster
    Nodes {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Include node conditions (with --json)
        #[arg(long)]
        verbose: bool,

        #[command(flatten)]
        kubectl_opts: KubectlOptions,
    },

    /// Stream logs from a pod
    Logs {
        /// Pod name
        pod: String,

        /// Container name (if pod has multiple containers)
        #[arg(short, long)]
        container: Option<String>,

        /// Number of lines to show from the end
        #[arg(short = 'n', long, default_value = "100")]
        tail: usize,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,

        /// Include timestamps
        #[arg(long)]
        timestamps: bool,

        #[command(flatten)]
        kubectl_opts: KubectlOptions,
    },

    /// Start an MCP server over stdio
    Mcp,
}

#[derive(Parser, Clone)]
pub struct KubectlOptions {
    /// Kubernetes context to use
    #[arg(long)]
    pub context: Option<String>,

    /// Kubernetes namespace (defaults to context namespace or "default")
    #[arg(short = 'n', long)]
    pub namespace: Option<String>,
}

impl Cli {
    #[allow(dead_code)]
    pub fn is_terminal_command(&self) -> bool {
        self.upgrade
            || self.check_update
            || self.info
            || self.record_install_method.is_some()
            || self.command.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli_parsing() {
        let cli = Cli::try_parse_from(["cluster", "status", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Status { json: true, .. })
        ));
    }

    #[test]
    fn hidden_record_install_method_accepts_supported_source() {
        let cli = Cli::try_parse_from(["cluster", "--record-install-method", "curl"])
            .expect("curl should be accepted");

        assert_eq!(cli.record_install_method.as_deref(), Some("curl"));
    }

    #[test]
    fn hidden_record_install_method_rejects_unknown_source() {
        assert!(Cli::try_parse_from(["cluster", "--record-install-method", "apt"]).is_err());
    }
}
