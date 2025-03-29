use clap::{App, Arg};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::Builder;
use walkdir::WalkDir;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Serialize, Deserialize)]
struct PackageInfo {
    name: String,
    targets: Vec<TargetInfo>,
}

#[derive(Serialize, Deserialize)]
struct TargetInfo {
    platform: String,
    arch: String,
    binary_path: String,
}

const BOOTSTRAP_SCRIPT: &str = r#"#!/bin/sh
PAYLOAD_LINE=$(awk '/^__PAYLOAD_BEGINS__/ { print NR + 1; exit 0; }' $0)
tail -n+$PAYLOAD_LINE $0 | tar xzf - -C /tmp
APP_NAME=$(jq -r '.name' /tmp/rustpack/info.json)

KERNEL=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [ "$KERNEL" = "darwin" ]; then
    PLATFORM="macos"
elif [ "$KERNEL" = "linux" ]; then
    PLATFORM="linux"
elif echo "$KERNEL" | grep -q "mingw\|cygwin\|msys"; then
    PLATFORM="windows"
else
    PLATFORM="unknown"
fi

if [ "$ARCH" = "x86_64" ] || [ "$ARCH" = "amd64" ]; then
    ARCH="x86_64"
elif [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
    ARCH="aarch64"
elif [ "$ARCH" = "i386" ] || [ "$ARCH" = "i686" ]; then
    ARCH="x86"
elif [ "$ARCH" = "arm" ] || [ "$ARCH" = "armv7l" ]; then
    ARCH="arm"
else
    ARCH="unknown"
fi

BINARY_PATH=$(jq -r --arg platform "$PLATFORM" --arg arch "$ARCH" '.targets[] | select(.platform == $platform and .arch == $arch) | .binary_path' /tmp/rustpack/info.json)

if [ -n "$BINARY_PATH" ]; then
    chmod +x "/tmp/rustpack/$BINARY_PATH"
    exec "/tmp/rustpack/$BINARY_PATH" "$@"
else
    echo "Error: No compatible binary found for $PLATFORM-$ARCH"
    exit 1
fi

exit 0
__PAYLOAD_BEGINS__
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("RustPack")
        .version("0.1.0")
        .about("Bundle Rust applications for cross-platform execution")
        .arg(
            Arg::with_name("input")
                .short("i")
                .long("input")
                .help("Path to the Rust project directory")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .help("Output file name")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("targets")
                .short("t")
                .long("targets")
                .help("Target triples to build for (comma-separated)")
                .takes_value(true),
        )
        .get_matches();

    let project_path = matches.value_of("input").unwrap();
    let project_name = get_project_name(project_path)?;
    let projectname = &format!("{}.rpack", project_name);
    let output_name = matches
        .value_of("output")
        .unwrap_or(projectname);

    let targets = matches
        .value_of("targets")
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![get_current_target()]);

    println!("Packing Rust project: {}", project_path);
    println!("Building for targets: {:?}", targets);

    let temp_dir = tempfile::tempdir()?;
    let rustpack_dir = temp_dir.path().join("rustpack");
    fs::create_dir_all(&rustpack_dir)?;

    let mut target_infos = Vec::new();

    for target in &targets {
        let (platform, arch) = parse_target(target);
        let bin_dir = rustpack_dir.join("bin").join(target);
        fs::create_dir_all(&bin_dir)?;

        println!("Building for {}", target);
        let binary_path = build_for_target(project_path, &bin_dir, target, &project_name)?;

        target_infos.push(TargetInfo {
            platform,
            arch,
            binary_path: binary_path.to_string_lossy().to_string(),
        });
    }

    let package_info = PackageInfo {
        name: project_name,
        targets: target_infos,
    };

    let info_json = serde_json::to_string_pretty(&package_info)?;
    fs::write(rustpack_dir.join("info.json"), info_json)?;

    create_package(&temp_dir.path(), output_name)?;

    println!("Package created successfully: {}", output_name);
    Ok(())
}

fn get_project_name(project_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let cargo_toml = Path::new(project_path).join("Cargo.toml");
    let cargo_content = fs::read_to_string(cargo_toml)?;

    cargo_content
        .lines()
        .find_map(|line| {
            if line.trim().starts_with("name =") {
                let parts: Vec<&str> = line.split('=').collect();
                if parts.len() >= 2 {
                    return Some(parts[1].trim().trim_matches('"').to_string());
                }
            }
            None
        })
        .ok_or_else(|| "Could not determine project name from Cargo.toml".into())
}

fn get_current_target() -> String {
    let output = Command::new("rustc")
        .args(&["-vV"])
        .output()
        .expect("Failed to execute rustc");

    let output_str = String::from_utf8_lossy(&output.stdout);

    for line in output_str.lines() {
        if line.starts_with("host:") {
            return line.split(':').nth(1).unwrap_or("unknown").trim().to_string();
        }
    }

    "unknown".to_string()
}

fn parse_target(target: &str) -> (String, String) {
    let parts: Vec<&str> = target.split('-').collect();

    if parts.len() < 2 {
        return ("unknown".to_string(), "unknown".to_string());
    }

    let arch = parts[0].to_string();

    let platform = if target.contains("windows") {
        "windows".to_string()
    } else if target.contains("linux") {
        "linux".to_string()
    } else if target.contains("darwin") || target.contains("apple") {
        "macos".to_string()
    } else {
        "unknown".to_string()
    };

    (platform, arch)
}

fn build_for_target(project_path: &str, bin_dir: &Path, target: &str, project_name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cargo_vendor_dir = tempfile::tempdir()?;

    Command::new("cargo")
        .current_dir(project_path)
        .args(&["vendor", "--versioned-dirs", cargo_vendor_dir.path().to_str().unwrap()])
        .status()?;

    let status = Command::new("cargo")
        .current_dir(project_path)
        .args(&[
            "build",
            "--release",
            "--target", target,
            "--offline",
            "--frozen",
        ])
        .env("CARGO_HOME", cargo_vendor_dir.path())
        .status()?;

    if !status.success() {
        return Err(format!("Failed to build for target: {}", target).into());
    }

    let binary_path = Path::new(project_path)
        .join("target")
        .join(target)
        .join("release")
        .join(project_name);

    let ext = if target.contains("windows") { ".exe" } else { "" };
    let binary_with_ext = format!("{}{}", project_name, ext);

    let dest_path = bin_dir.join(&binary_with_ext);
    fs::copy(&binary_path, &dest_path)?;

    let rel_path = PathBuf::from("bin")
        .join(target)
        .join(&binary_with_ext);

    Ok(rel_path)
}

fn create_package(temp_dir: &Path, output_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let temp_archive = tempfile::NamedTempFile::new()?;

    let tar_gz = GzEncoder::new(temp_archive.reopen()?, Compression::default());
    let mut tar = Builder::new(tar_gz);

    tar.append_dir_all("", temp_dir)?;

    let tar_gz = tar.into_inner()?;
    tar_gz.finish()?;

    let mut output_file = File::create(output_name)?;

    output_file.write_all(BOOTSTRAP_SCRIPT.as_bytes())?;

    io::copy(&mut File::open(temp_archive.path())?, &mut output_file)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(output_name)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(output_name, perms)?;
    }

    #[cfg(windows)]
    {
        fs::metadata(output_name)?;
    }

    Ok(())
}
