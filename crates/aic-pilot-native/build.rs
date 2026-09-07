use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn sources(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, files)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some()
    {
        // Resolve CPython symbols from the importing interpreter, only for this cdylib.
        println!("cargo:rustc-link-arg-cdylib=-undefined");
        println!("cargo:rustc-link-arg-cdylib=dynamic_lookup");
    }
    let directory = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let root = directory
        .parent()
        .and_then(Path::parent)
        .ok_or("Missing workspace root")?;
    let mut files = vec![
        directory.join("Cargo.toml"),
        directory.join("build.rs"),
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
    ];
    sources(&directory.join("src"), &mut files)?;
    files.sort();
    let mut hashes = BTreeMap::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let hash: String = Sha256::digest(std::fs::read(&path)?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        hashes.insert(
            path.strip_prefix(root)?.to_string_lossy().into_owned(),
            hash,
        );
    }
    let compiler = std::process::Command::new(std::env::var("RUSTC")?)
        .arg("--version")
        .output()?;
    if !compiler.status.success() {
        return Err("Failed to query the native compiler".into());
    }
    let info = serde_json::json!({"schema_version":1,"files_sha256":hashes,
        "rustc":String::from_utf8(compiler.stdout)?.trim(),"profile":std::env::var("PROFILE")?,
        "target":std::env::var("TARGET")?,"extension_module":std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some()});
    println!("cargo:rustc-env=AIC_PILOT_BUILD_INFO={info}");
    Ok(())
}
