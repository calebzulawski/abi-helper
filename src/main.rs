extern crate clap;
mod abi_reader;
use abi_reader::*;

fn main() {
    let yaml = clap::load_yaml!("cli.yml");
    let m = clap::App::from_yaml(yaml)
        .name(clap::crate_name!())
        .about(clap::crate_description!())
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .get_matches();

    if let Some(files) = m.values_of("export") {
        for file in files {
            abi_reader::Export::new().run(file).unwrap();
        }
    }

    if let Some(files) = m.values_of("strip") {
        for file in files {
            abi_reader::Strip::new().run(file).unwrap();
        }
    }
}
