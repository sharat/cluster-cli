use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cluster-rs", about = "AKS Cluster Health TUI")]
struct Args {
    /// Kubernetes namespace to monitor
    #[arg(short, long)]
    namespace: Option<String>,

    /// Azure resource group
    #[arg(short = 'g', long)]
    resource_group: Option<String>,

    /// Cluster name
    #[arg(short, long)]
    cluster: Option<String>,

    /// Refresh interval in seconds
    #[arg(short = 'r', long, visible_alias = "frequency")]
    refresh: Option<u64>,

    /// Node pool name filter (e.g. "nodepool1")
    #[arg(long)]
    node_pool_filter: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub namespace: String,
    pub resource_group: Option<String>,
    pub cluster_name: Option<String>,
    pub refresh_interval_secs: u64,
    pub node_pool_filter: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            namespace: String::new(),
            resource_group: None,
            cluster_name: None,
            refresh_interval_secs: 60, // 1 minute, aligned to clock boundaries
            node_pool_filter: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let args = Args::parse();
        Self::load_from_args(args)
    }

    fn load_from_args(args: Args) -> Result<Self> {
        let config_path = config_file_path();
        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content).unwrap_or_default()
        } else {
            Config::default()
        };

        if let Some(ns) = args.namespace {
            config.namespace = ns;
        }
        if let Some(rg) = args.resource_group {
            config.resource_group = Some(rg);
        }
        if let Some(cluster) = args.cluster {
            config.cluster_name = Some(cluster);
        }
        if let Some(refresh) = args.refresh {
            config.refresh_interval_secs = refresh;
        }
        if let Some(filter) = args.node_pool_filter {
            config.node_pool_filter = Some(filter);
        }

        Ok(config)
    }
}

fn config_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cluster-rs")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::{Args, Config};
    use clap::Parser;

    #[test]
    fn cli_refresh_does_not_override_config_when_flag_is_absent() {
        let args = Args::parse_from(["cluster-rs"]);
        let config = Config::load_from_args(args).expect("config should load");

        assert_eq!(config.refresh_interval_secs, 60);
    }

    #[test]
    fn cli_refresh_overrides_when_explicitly_provided() {
        let args = Args::parse_from(["cluster-rs", "--refresh", "15"]);
        let config = Config::load_from_args(args).expect("config should load");

        assert_eq!(config.refresh_interval_secs, 15);
    }
}
