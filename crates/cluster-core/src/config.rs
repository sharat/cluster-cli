use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Args, Debug, Default)]
pub struct ConfigOverrides {
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

    /// Disable live `kubectl --watch` updates and only poll on the refresh interval
    #[arg(long)]
    no_watch: bool,
}

/// Bounds for the refresh interval. Zero would divide by zero when aligning
/// ticks to clock boundaries, and huge values overflow `Instant` arithmetic.
pub const MIN_REFRESH_SECS: u64 = 1;
pub const MAX_REFRESH_SECS: u64 = 86_400;

pub fn clamp_refresh_secs(secs: u64) -> u64 {
    secs.clamp(MIN_REFRESH_SECS, MAX_REFRESH_SECS)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub namespace: String,
    pub resource_group: Option<String>,
    pub cluster_name: Option<String>,
    pub refresh_interval_secs: u64,
    pub node_pool_filter: Option<String>,
    /// Stream pod, node and event changes between polls with `kubectl --watch`.
    #[serde(default = "default_watch")]
    pub watch: bool,
    /// Custom resources (`plural.group`) whose Ready-style conditions feed the
    /// incident queue, e.g. `certificates.cert-manager.io`.
    #[serde(default)]
    pub crd_checks: Vec<String>,
}

fn default_watch() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            namespace: String::new(),
            resource_group: None,
            cluster_name: None,
            refresh_interval_secs: 60, // 1 minute, aligned to clock boundaries
            node_pool_filter: None,
            watch: default_watch(),
            crd_checks: Vec::new(),
        }
    }
}

impl Config {
    pub fn dir_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cluster-cli")
    }

    pub fn load(overrides: ConfigOverrides) -> Result<Self> {
        Self::load_from_overrides(overrides)
    }

    fn load_from_overrides(overrides: ConfigOverrides) -> Result<Self> {
        let config_path = Self::dir_path().join("config.toml");
        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content).unwrap_or_else(|err| {
                eprintln!(
                    "warning: ignoring invalid config {}: {err}",
                    config_path.display()
                );
                Config::default()
            })
        } else {
            Config::default()
        };

        if let Some(ns) = overrides.namespace {
            config.namespace = ns;
        }
        if let Some(rg) = overrides.resource_group {
            config.resource_group = Some(rg);
        }
        if let Some(cluster) = overrides.cluster {
            config.cluster_name = Some(cluster);
        }
        if let Some(refresh) = overrides.refresh {
            config.refresh_interval_secs = refresh;
        }
        if let Some(filter) = overrides.node_pool_filter {
            config.node_pool_filter = Some(filter);
        }
        if overrides.no_watch {
            config.watch = false;
        }
        config.refresh_interval_secs = clamp_refresh_secs(config.refresh_interval_secs);

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigOverrides};
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        config: ConfigOverrides,
    }

    #[test]
    fn cli_refresh_does_not_override_config_when_flag_is_absent() {
        let args = TestCli::parse_from(["cluster-cli"]);
        let config = Config::load_from_overrides(args.config).expect("config should load");

        assert_eq!(config.refresh_interval_secs, 60);
    }

    #[test]
    fn cli_refresh_overrides_when_explicitly_provided() {
        let args = TestCli::parse_from(["cluster-cli", "--refresh", "15"]);
        let config = Config::load_from_overrides(args.config).expect("config should load");

        assert_eq!(config.refresh_interval_secs, 15);
    }

    #[test]
    fn older_config_files_get_defaults_for_new_fields() {
        let config: Config =
            toml::from_str("namespace = \"default\"\nrefresh_interval_secs = 30\n")
                .expect("config without watch/crd_checks should parse");

        assert!(config.watch);
        assert!(config.crd_checks.is_empty());
    }

    #[test]
    fn crd_checks_parse_from_toml() {
        let config: Config = toml::from_str(
            "namespace = \"default\"\nrefresh_interval_secs = 30\nwatch = false\ncrd_checks = [\"certificates.cert-manager.io\"]\n",
        )
        .expect("config should parse");

        assert!(!config.watch);
        assert_eq!(config.crd_checks, vec!["certificates.cert-manager.io"]);
    }

    #[test]
    fn cli_no_watch_disables_watch() {
        let args = TestCli::parse_from(["cluster-cli", "--no-watch"]);
        let config = Config::load_from_overrides(args.config).expect("config should load");

        assert!(!config.watch);
    }

    #[test]
    fn cli_node_pool_filter_is_applied() {
        let args = TestCli::parse_from(["cluster-cli", "--node-pool-filter", "workers"]);
        let config = Config::load_from_overrides(args.config).expect("config should load");

        assert_eq!(config.node_pool_filter.as_deref(), Some("workers"));
    }

    #[test]
    fn zero_refresh_is_clamped() {
        let args = TestCli::parse_from(["cluster-cli", "--refresh", "0"]);
        let config = Config::load_from_overrides(args.config).expect("config should load");

        assert_eq!(config.refresh_interval_secs, super::MIN_REFRESH_SECS);
    }

    #[test]
    fn partial_toml_keeps_defaults_for_missing_fields() {
        let config: Config = toml::from_str("refresh_interval_secs = 30").expect("should parse");

        assert_eq!(config.refresh_interval_secs, 30);
        assert!(config.namespace.is_empty());
    }
}
