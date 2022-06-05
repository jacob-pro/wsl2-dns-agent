#![windows_subsystem = "windows"]

use crate::runner::{start_runner, RunReason};
use crate::tray::{run_tray, TrayProperties};
use log::LevelFilter;
use simplelog::WriteLogger;
use std::ffi::c_void;
use std::fs;
use std::fs::File;
use std::sync::mpsc::Sender;
use windows::Win32::Foundation::{BOOLEAN, HANDLE};
use windows::Win32::NetworkManagement::IpHelper::{
    NotifyRouteChange2, MIB_IPFORWARD_ROW2, MIB_NOTIFICATION_TYPE,
};
use windows::Win32::Networking::WinSock::AF_UNSPEC;

mod config;
mod dns;
mod runner;
mod tray;
mod wsl;

pub const APP_NAME: &str = "WSL2 DNS Agent";

fn main() {
    // Setup logging to "AppData\Local\WSL2 DNS Agent\log.txt"
    let local_appdata = dirs::data_local_dir().unwrap().join(APP_NAME);
    fs::create_dir_all(&local_appdata).unwrap();
    let log_path = local_appdata.join("log.txt");
    let log_file = File::create(&log_path).unwrap();
    WriteLogger::init(LevelFilter::Info, simplelog::Config::default(), log_file).unwrap();

    // Load config file
    let config = config::Config::load();
    log::info!("Loaded config: {:?}", config);

    // Listen to route table notifications
    let (tx, rx) = runner::channel();
    let tx_notify = Box::new(tx.clone());
    let mut handle = HANDLE::default();
    unsafe {
        NotifyRouteChange2(
            AF_UNSPEC.0 as u16,
            Some(callback),
            (tx_notify.as_ref() as *const Sender<RunReason>) as *const c_void,
            BOOLEAN(0),
            &mut handle,
        )
        .unwrap();
    }

    // Apply DNS changes on notifications
    start_runner(config, rx);
    // Run automatically on startup
    tx.send(RunReason::Startup).ok();

    // Run Windows tray icon
    run_tray(TrayProperties {
        log_file_path: log_path,
        sender: tx,
    })
}

unsafe extern "system" fn callback(
    callercontext: *const c_void,
    _: *const MIB_IPFORWARD_ROW2,
    _: MIB_NOTIFICATION_TYPE,
) {
    log::info!("NotifyRouteChange called");
    let tx = &*(callercontext as *const Sender<RunReason>);
    tx.send(RunReason::RouteChange).ok();
}
