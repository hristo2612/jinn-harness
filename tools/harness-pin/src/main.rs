//! CLI for the pin-bump procedure (see `KERNEL-PIN.md`).
//!
//! `harness-pin compute <dir>` — contract hash of a directory on disk.
//! `harness-pin compute-git <repo> <commit> <subdir>` — contract hash of a
//! subdirectory as recorded at a commit (working tree never consulted).

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match strs.as_slice() {
        ["compute", dir] => harness_pin::contract_hash(Path::new(dir)).map_err(|e| e.to_string()),
        ["compute-git", repo, commit, subdir] => {
            harness_pin::contract_hash_of_git_tree(Path::new(repo), commit, subdir)
        }
        _ => Err("usage: harness-pin compute <dir> | compute-git <repo> <commit> <subdir>".into()),
    };
    match result {
        Ok(hash) => {
            println!("{hash}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("harness-pin: {e}");
            ExitCode::FAILURE
        }
    }
}
