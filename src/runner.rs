use crate::config::Config;
use crate::dns;
use crate::wsl;
use std::sync::mpsc;
use std::thread::spawn;

#[derive(Debug)]
pub enum RunReason {
    Startup,
    RouteChange,
}

pub fn channel() -> (mpsc::Sender<RunReason>, mpsc::Receiver<RunReason>) {
    mpsc::channel()
}

pub fn start_runner(config: Config, rx: mpsc::Receiver<RunReason>) {
    spawn(move || loop {
        let msg = rx.recv().unwrap();
        log::info!("recv: {:?}", msg);

        let dns = dns::get_configuration().unwrap();
        println!("{:?}", dns);
        println!("{:?}", wsl::get_distributions().unwrap());
    });
}
