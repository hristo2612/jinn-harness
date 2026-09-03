//! The extension tier's kit builder (see Cargo.toml for usage): builds the
//! Boa provider and prints the exact number the card records.

use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: ext-kit build <artifacts-dir>");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("build"), Some(dir)) => {
            let (hash, size) = ext_kit::build(&PathBuf::from(dir));
            println!("{} {size} bytes sha256 {hash}", ext_kit::BOA_GUEST);
        }
        _ => usage(),
    }
}
