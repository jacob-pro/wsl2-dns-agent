use crate::dns::get_configuration;
use crate::wsl::get_distributions;

mod dns;
mod wsl;

fn main() {
    let dns = get_configuration().unwrap();
    println!("{:?}", dns);
    println!("{:?}", get_distributions().unwrap());
}
