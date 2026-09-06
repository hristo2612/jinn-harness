//! Resolve the npm launcher at kit time: the runtime must own the native PID.
use std::path::{Path, PathBuf};

/// Accept a native binary or the installed OpenAI npm entry point. Unknown
/// launchers fail closed: waiting for their PID cannot confirm worker exit.
/// Package paths follow codex-cli's published platform-package layout.
pub fn executable(path: &Path) -> Result<PathBuf, String> {
    let path = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
    if native(&path) {
        return Ok(path);
    }
    let package = path
        .parent()
        .and_then(Path::parent)
        .ok_or("Codex package missing")?;
    let metadata: serde_json::Value = std::fs::read(package.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or("Text Chat needs a native Codex executable or its OpenAI npm launcher")?;
    let entry = metadata["bin"]["codex"]
        .as_str()
        .ok_or("Codex npm bin entry missing")?;
    if metadata["name"] != "@openai/codex"
        || package.join(entry).canonicalize().ok().as_ref() != Some(&path)
    {
        return Err("Unrecognized Codex launcher; supply the native executable".into());
    }
    let (platform, triple) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => ("darwin-arm64", "aarch64-apple-darwin"),
        ("macos", "x86_64") => ("darwin-x64", "x86_64-apple-darwin"),
        ("linux", "aarch64") => ("linux-arm64", "aarch64-unknown-linux-musl"),
        ("linux", "x86_64") => ("linux-x64", "x86_64-unknown-linux-musl"),
        _ => return Err("Unsupported npm platform; supply a native Codex executable".into()),
    };
    // Node resolves dependencies from the nearest node_modules directory.
    // Canonicalization also handles npm/pnpm dependency symlinks.
    let dependency = package
        .ancestors()
        .map(|dir| dir.join(format!("node_modules/@openai/codex-{platform}")))
        .find(|dir| dir.join("package.json").is_file());
    let vendor = dependency
        .unwrap_or_else(|| package.to_path_buf())
        .join("vendor");
    let worker = vendor.join(triple).join("bin/codex");
    if !native(&worker) {
        return Err("Native Codex dependency is missing or not executable; reinstall Codex or supply its native executable".into());
    }
    worker.canonicalize().map_err(|error| error.to_string())
}

fn native(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    let mut magic = [0; 4];
    file.read_exact(&mut magic).is_ok()
        && matches!(
            &magic,
            b"\x7fELF"
                | b"\xcf\xfa\xed\xfe"
                | b"\xfe\xed\xfa\xcf"
                | b"\xca\xfe\xba\xbe"
                | b"\xbe\xba\xfe\xca"
        )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn resolves_npm_launcher_and_refuses_unknown_scripts() {
        let root = std::env::temp_dir().join(format!("ui-kit-codex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("node_modules/@openai/codex");
        std::fs::create_dir_all(package.join("bin")).unwrap();
        std::fs::write(
            package.join("package.json"),
            r#"{"name":"@openai/codex","bin":{"codex":"bin/codex.js"}}"#,
        )
        .unwrap();
        let launcher = package.join("bin/codex.js");
        std::fs::write(&launcher, "#!/usr/bin/env node\n").unwrap();
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (platform, triple) = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => ("darwin-arm64", "aarch64-apple-darwin"),
            ("macos", "x86_64") => ("darwin-x64", "x86_64-apple-darwin"),
            ("linux", "aarch64") => ("linux-arm64", "aarch64-unknown-linux-musl"),
            ("linux", "x86_64") => ("linux-x64", "x86_64-unknown-linux-musl"),
            _ => return,
        };
        let native = package.join(format!(
            "node_modules/@openai/codex-{platform}/vendor/{triple}/bin/codex"
        ));
        std::fs::create_dir_all(native.parent().unwrap()).unwrap();
        std::fs::write(
            package.join(format!(
                "node_modules/@openai/codex-{platform}/package.json"
            )),
            "{}",
        )
        .unwrap();
        std::fs::write(&native, b"\x7fELFfixture").unwrap();
        std::fs::set_permissions(&native, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = root.join("codex");
        symlink(&launcher, &link).unwrap();
        assert_eq!(
            executable(&link).unwrap(),
            native.canonicalize().unwrap(),
            "the grant must own the worker, not its launcher"
        );
        assert_eq!(executable(&native).unwrap(), native.canonicalize().unwrap());
        std::fs::write(&native, b"#!/bin/sh\n").unwrap();
        assert!(executable(&link).is_err(), "never resolve another launcher");
        std::fs::remove_file(&native).unwrap();
        assert!(
            executable(&link).is_err(),
            "missing native dependency is actionable failure"
        );
        std::fs::write(&native, b"\x7fELFfixture").unwrap();
        std::fs::set_permissions(&native, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(executable(&native).is_err(), "native must be executable");
        std::fs::set_permissions(&native, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bundled = package.join(format!("vendor/{triple}/bin/codex"));
        std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
        std::fs::rename(&native, &bundled).unwrap();
        std::fs::remove_dir_all(package.join("node_modules")).unwrap();
        assert_eq!(executable(&link).unwrap(), bundled.canonicalize().unwrap());
        std::fs::write(
            package.join("package.json"),
            r#"{"name":"unrelated","bin":{"codex":"bin/codex.js"}}"#,
        )
        .unwrap();
        assert!(executable(&link).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
