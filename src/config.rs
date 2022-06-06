use crate::APP_NAME;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// Show toast notifications when DNS update is applied
    #[serde(default = "r#true")]
    pub show_notifications: bool,
    /// Fallback settings if named distribution setting is not specified
    #[serde(default)]
    defaults: DistributionSetting,
    /// Per distribution settings
    #[serde(default)]
    distributions: HashMap<String, DistributionSetting>,
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
            log::info!("Config file doesn't exist, creating default");
            let new = Self::default();
            new.save(&config_path);
            return new;
        }
        match fs::read_to_string(config_path) {
            Ok(data) => match toml::from_str(&data) {
                Ok(config) => return config,
                Err(err) => log::error!("Unable to parse config file: {err}"),
            },
            Err(err) => log::error!("Unable to read config file: {err}"),
        }
        log::warn!("Falling back to config defaults");
        Self::default()
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
