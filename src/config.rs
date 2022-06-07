use crate::APP_NAME;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const EXCLUDE_BY_DEFAULT: &[&str] = &["docker-desktop", "docker-desktop-data"];

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// Show toast notifications when DNS update is applied
    #[serde(default = "r#true")]
    pub show_notifications: bool,
    /// Fallback settings if named distribution setting is not specified
    #[serde(default)]
    defaults: DistributionSetting,
    /// Per distribution settings
    #[serde(default = "default_distributions")]
    distributions: HashMap<String, DistributionSetting>,
}

fn default_distributions() -> HashMap<String, DistributionSetting> {
    let mut map = HashMap::new();
    for d in EXCLUDE_BY_DEFAULT {
        map.insert(
            d.to_string(),
            DistributionSetting {
                apply_dns: false,
                shutdown: false,
                patch_wsl_conf: false,
            },
        );
    }
    map
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DistributionSetting {
    /// Whether to update the wsl.conf and resolv.conf files for this distribution
    #[serde(default = "r#true")]
    pub apply_dns: bool,
    /// If the distribution was previously Stopped, then shutdown once the DNS update is complete
    /// Note: This option is probably not needed on Windows 11 (because of vmIdleTimeout)
    #[serde(default)]
    pub shutdown: bool,
    /// Automatically patch /etc/wsl.conf to disable generateResolvConf
    /// Note this will trigger a restart of the distribution
    #[serde(default = "r#true")]
    pub patch_wsl_conf: bool,
}

pub fn r#true() -> bool {
    true
}

impl Config {
    /// Attempts to read config from AppData otherwise falls back to defaults
    pub fn load() -> Self {
        let roaming_appdata = dirs::config_dir().unwrap().join(APP_NAME);
        fs::create_dir_all(&roaming_appdata).unwrap();
        let config_path = roaming_appdata.join("config.toml");
        if !config_path.exists() {
            log::warn!("Config file doesn't exist, creating default");
            let new = Self::default();
            new.save(&config_path);
            return new;
        }
        let contents = match fs::read_to_string(&config_path) {
            Err(e) => panic!(
                "Unable to read config file: {}: {}",
                config_path.display(),
                e
            ),
            Ok(s) => s,
        };
        match toml::from_str(&contents) {
            Err(e) => panic!(
                "Unable to parse config file: {}: {}",
                config_path.display(),
                e
            ),
            Ok(config) => config,
        }
    }

    fn save(&self, path: &PathBuf) {
        let contents = toml::to_string(&self).unwrap();
        if let Err(err) = fs::write(path, contents) {
            log::error!("Failed to write config file: {err}");
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str("").unwrap()
    }
}

impl Default for DistributionSetting {
    fn default() -> Self {
        toml::from_str("").unwrap()
    }
}

impl Config {
    pub fn get_distribution_setting(&self, distribution: &str) -> &DistributionSetting {
        self.distributions
            .get(distribution)
            .unwrap_or(&self.defaults)
    }
}
