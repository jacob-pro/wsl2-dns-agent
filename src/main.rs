use crate::dns::get_configuration;
use crate::wsl::get_distributions;
use log::LevelFilter;
use simplelog::WriteLogger;
use std::fs;
use std::fs::File;
use std::sync::Arc;

mod config;
mod dns;
mod wsl;

pub const APP_NAME: &str = "WSL2 DNS Agent";

fn main() {
    let local_appdata = dirs::data_local_dir().unwrap().join(APP_NAME);
    fs::create_dir_all(&local_appdata).unwrap();
    let log_path = local_appdata.join("log.txt");
    let log_file = File::create(log_path).unwrap();
    WriteLogger::init(LevelFilter::Info, simplelog::Config::default(), log_file).unwrap();

    let config = Arc::new(config::Config::load());
    log::info!("Loaded config: {:?}", config);

    let dns = get_configuration().unwrap();
    println!("{:?}", dns);
    println!("{:?}", get_distributions().unwrap());
}
