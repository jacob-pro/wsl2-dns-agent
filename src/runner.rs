use crate::config::{Config, DistributionSetting};
use crate::dns;
use crate::tray::TrayHandle;
use crate::wsl;
use crate::wsl::WslDistribution;
use configparser::ini::Ini;
use std::sync::mpsc;
use std::thread::spawn;
use std::time::{Duration, Instant};
use thiserror::Error;

const RESOLV_CONF: &str = "/etc/resolv.conf";
const WSL_CONF: &str = "/etc/wsl.conf";

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

pub fn start_runner(config: Config, rx: mpsc::Receiver<RunReason>, tray: TrayHandle) {
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
        match update_dns(&config) {
            Err(e) => log::error!("Error running: {e}"),
            Ok(_) => {
                if config.show_notifications {
                    tray.notify_dns_updated();
                }
            }
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
    let resolv = dns.generate_resolv();
    log::info!("Applying DNS config: {dns:?}");
    let wsl = wsl::get_distributions()?
        .into_iter()
        .filter(|d| d.version == 2)
        .collect::<Vec<_>>();
    log::info!("Found {} WSL2 distributions", wsl.len());
    for d in wsl {
        let dist_config = config.get_distribution_setting(&d.name);
        if dist_config.apply_dns {
            log::info!("Updating DNS for {}", d.name);
            if let Err(e) = update_distribution(&d, dist_config, &resolv) {
                log::error!("Failed to update DNS for {}, due to: {}", d.name, e);
            }
        }
    }
    Ok(())
}

fn update_distribution(
    distribution: &WslDistribution,
    config: &DistributionSetting,
    resolv: &str,
) -> Result<(), Error> {
    // Ensure that generateResolvConf is disabled, otherwise further steps will fail
    if config.patch_wsl_conf {
        let mut config = Ini::new_cs();
        let wsl_conf = distribution.read_file(WSL_CONF).ok();
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
            log::warn!("Updating {} for {}", WSL_CONF, distribution.name);
            config.set("network", "generateResolvConf", Some("false".to_string()));
            let new_conf = config.writes().replace("\r\n", "\n");
            distribution.write_file(WSL_CONF, &new_conf)?;
            // Distribution needs to be restarted to take effect
            distribution.terminate()?;
        }
    }

    // Replace the /etc/resolv.conf file
    // Removing read only is expected to fail if the file doesn't exist
    // Read only needs to be set because of bug:
    // https://github.com/microsoft/WSL/issues/6977
    distribution.set_read_only(RESOLV_CONF, false).ok();
    distribution.write_file(RESOLV_CONF, resolv)?;
    distribution.set_read_only(RESOLV_CONF, true)?;

    // Optionally shutdown the WSL2 distribution once finished
    if config.shutdown && distribution.was_stopped() {
        log::info!("Terminating {}", distribution.name);
        distribution.terminate()?;
    }

    Ok(())
}
