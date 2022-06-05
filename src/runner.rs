use crate::config::Config;
use crate::dns;
use crate::wsl;
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
        while let Ok(_) = rx.recv_timeout(timeout.saturating_duration_since(Instant::now())) {
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
}

fn update_dns(config: &Config) -> Result<(), Error> {
    let dns = dns::get_configuration()?;
    log::info!("Applying DNS config: {dns:?}");
    let wsl = wsl::get_distributions()?
        .into_iter()
        .filter(|d| d.version == 2)
        .collect::<Vec<_>>();
    log::info!("Found {} WSL2 distributions", wsl.len());

    Ok(())
}
