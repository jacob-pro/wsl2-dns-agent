use crate::config::{Config, DistributionSetting};
use crate::dns;
use crate::wsl;
use crate::wsl::WslDistribution;
use configparser::ini::Ini;
use std::sync::mpsc;
use std::thread::spawn;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug)]
pub enum RunReason {
    Startup,
    RouteChange,
    TrayButton,
}

const DEBOUNCE: Duration = Duration::from_millis(300);

pub fn channel() -> (mpsc::Sender<RunReason>, mpsc::Receiver<RunReason>) {
    mpsc::channel()
}

pub fn start_runner(config: Config, rx: mpsc::Receiver<RunReason>) {
    spawn(move || loop {
        let msg = rx.recv().unwrap();
        let timeout = Instant::now() + DEBOUNCE;
        let mut debounced = 0;
        while rx
            .recv_timeout(timeout.saturating_duration_since(Instant::now()))
            .is_ok()
        {
            debounced += 1;
        }
        log::info!("Running due to {msg:?} message (and {debounced} debounced messages)");
        if let Err(err) = update_dns(&config) {
            log::error!("Error running: {err}");
        }
    });
}

#[derive(Debug, Error)]
enum Error {
    #[error("Win32 DNS error: {0}")]
    Dns(
        #[source]
        #[from]
        dns::Error,
    ),
    #[error("Calling WSL error: {0}")]
    Wsl(
        #[source]
        #[from]
        wsl::Error,
    ),
    #[error("wsl.conf error: {0}")]
    WslConfError(String),
}

fn update_dns(config: &Config) -> Result<(), Error> {
    let dns = dns::get_configuration()?;
    log::info!("Applying DNS config: {dns:?}");
    let wsl = wsl::get_distributions()?
        .into_iter()
        .filter(|d| d.version == 2)
        .collect::<Vec<_>>();
    log::info!("Found {} WSL2 distributions", wsl.len());
    for d in wsl {
        log::info!("Updating DNS for {}", d.name);
        if let Err(e) = update_distribution(&d, config.get_distribution_setting(&d.name)) {
            log::error!("Failed to update DNS for {}, due to: {}", d.name, e);
        }
    }
    Ok(())
}

fn update_distribution(
    distribution: &WslDistribution,
    config: &DistributionSetting,
) -> Result<(), Error> {
    if config.patch_wsl_conf {
        let wsl_conf = distribution.read_wsl_conf()?;
        let mut config = Ini::new_cs();
        let needs_update = if let Some(wsl_conf) = wsl_conf {
            config.read(wsl_conf).map_err(Error::WslConfError)?;
            config
                .get("network", "generateResolvConf")
                .unwrap_or_else(|| "true".to_string())
                != "false"
        } else {
            true
        };
        if needs_update {
            log::info!("Updating /etc/wsl.conf");
            config.set("network", "generateResolvConf", Some("false".to_string()));
            distribution.write_wsl_conf(&config.writes())?;
        }
    }
    Ok(())
}
