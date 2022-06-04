use crate::dns::get_configuration;

mod dns;

fn main() {
    let dns = get_configuration().unwrap();
    println!("{:?}", dns);
}
