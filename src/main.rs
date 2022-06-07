use crate::runner::{start_runner, RunReason};
use crate::tray::Tray;
use log::LevelFilter;
use simplelog::WriteLogger;
use std::ffi::c_void;
use std::fs;
use std::fs::File;
use std::sync::mpsc::Sender;
use win32_utils::console::hide_console_window_if_in_process;
use win32_utils::instance::UniqueInstance;
use win32_utils::str::ToWin32Str;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOLEAN, HANDLE, HWND};
use windows::Win32::NetworkManagement::IpHelper::{
    NotifyRouteChange2, MIB_IPFORWARD_ROW2, MIB_NOTIFICATION_TYPE,
};
use windows::Win32::Networking::WinSock::AF_UNSPEC;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONSTOP, MB_OK};

mod config;
mod dns;
mod runner;
mod tray;
mod wsl;

pub const APP_NAME: &str = "WSL2 DNS Agent";

fn main() {
    set_panic();
    hide_console_window_if_in_process();

    let _unique = match UniqueInstance::acquire_unique_to_session(APP_NAME) {
        Ok(u) => u,
        Err(win32_utils::instance::Error::AlreadyExists) => panic!("Application already running"),
        Err(e) => panic!("{}", e),
    };

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

    // Create tray
    let tray = Tray::new(log_path, tx.clone());

    // Apply DNS changes on notifications
    start_runner(config, rx, tray.get_handle());
    // Run automatically on startup
    tx.send(RunReason::Startup).ok();

    // Run Windows tray icon
    tray.run();
}

unsafe extern "system" fn callback(
    callercontext: *const c_void,
    _: *const MIB_IPFORWARD_ROW2,
    _: MIB_NOTIFICATION_TYPE,
) {
    let tx = &*(callercontext as *const Sender<RunReason>);
    tx.send(RunReason::RouteChange).ok();
}

fn set_panic() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| unsafe {
        before(info);
        let title = "Fatal Error".to_wchar();
        let text = format!("{}", info).to_wchar();
        MessageBoxW(
            HWND::default(),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONSTOP,
        );
    }));
}
