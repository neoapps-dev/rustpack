use clap::{Command, Arg, ArgAction};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use tar::Builder;
use walkdir::WalkDir;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::env;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use chrono::Local;
use notify::{Watcher, RecursiveMode, watcher};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use std::thread;
use zip::write::FileOptions;
use sha2::{Sha256, Digest};
use hmac::{Hmac, Mac};
use base64::encode;
use std::sync::Arc;
use semver::Version;
use toml;

type HmacSha256 = Hmac<Sha256>;

#[derive(Serialize, Deserialize, Clone)]
struct PackageInfo {
    name: String,
    version: String,
    description: Option<String>,
    targets: Vec<TargetInfo>,
    created_at: String,
    checksum: String,
    features: Vec<String>,
    metadata: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct TargetInfo {
    platform: String,
    arch: String,
    binary_path: String,
    features: Vec<String>,
    optimizations: Option<String>,
    compatibility: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct BuildConfig {
    strip: bool,
    compress: bool,
    lto: Option<String>,
    debug_symbols: bool,
    profile: String,
    features: Vec<String>,
    assets: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct RustPackConfig {
    name: Option<String>,
    output: Option<String>,
    targets: Option<Vec<String>>,
    strip: Option<bool>,
    compress: Option<bool>,
    lto: Option<String>,
    profile: Option<String>,
    features: Option<Vec<String>>,
    assets: Option<Vec<String>>,
    zip: Option<bool>,
    no_default_features: Option<bool>,
    watch: Option<bool>,
    sign: Option<String>,
    verbose: Option<bool>,
}

const BOOTSTRAP_SCRIPT: &str = r#"#!/bin/sh
PAYLOAD_LINE=$(awk '/^__PAYLOAD_BEGINS__/ { print NR + 1; exit 0; }' $0)
TEMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t rustpack)
tail -n+$PAYLOAD_LINE $0 | tar xzf - -C "$TEMP_DIR"
APP_NAME=$(jq -r '.name' "$TEMP_DIR/rustpack/info.json")

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

if [ -d "$TEMP_DIR/rustpack/assets" ]; then
    export RUSTPACK_ASSETS_DIR="$TEMP_DIR/rustpack/assets"
fi

BINARY_PATH=$(jq -r --arg platform "$PLATFORM" --arg arch "$ARCH" '.targets[] | select(.platform == $platform and .arch == $arch) | .binary_path' "$TEMP_DIR/rustpack/info.json")

if [ -n "$BINARY_PATH" ]; then
    chmod +x "$TEMP_DIR/rustpack/$BINARY_PATH"
    CLEANUP_OPT="--cleanup"
    if echo "$*" | grep -q -- "$CLEANUP_OPT"; then
        ARGS=$(echo "$*" | sed "s/$CLEANUP_OPT//")
        exec "$TEMP_DIR/rustpack/$BINARY_PATH" $ARGS
        trap "rm -rf $TEMP_DIR" EXIT
    else
        exec "$TEMP_DIR/rustpack/$BINARY_PATH" "$@"
    fi
else
    echo "Error: No compatible binary found for $PLATFORM-$ARCH"
    exit 1
fi

exit 0
__PAYLOAD_BEGINS__
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("RustPack")
        .version("0.2.0")
        .about("Bundle Rust applications for cross-platform execution")
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .help("Path to the Rust project directory")
                .required(true),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Output file name"),
        )
        .arg(
            Arg::new("targets")
                .short('t')
                .long("targets")
                .help("Target triples to build for (comma-separated)"),
        )
        .arg(
            Arg::new("strip")
                .long("strip")
                .help("Strip debug symbols from binaries")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("lto")
                .long("lto")
                .help("Enable Link Time Optimization (thin, fat, off)")
                .default_value("off"),
        )
        .arg(
            Arg::new("compress")
                .long("compress")
                .help("Compress binaries with UPX if available")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("watch")
                .long("watch")
                .help("Watch for changes and rebuild automatically")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("sign")
                .long("sign")
                .help("Sign the package with a key"),
        )
        .arg(
            Arg::new("features")
                .long("features")
                .help("Cargo features to enable (comma-separated)"),
        )
        .arg(
            Arg::new("profile")
                .long("profile")
                .help("Build profile (dev, release)")
                .default_value("release"),
        )
        .arg(
            Arg::new("no-default-features")
                .long("no-default-features")
                .help("Disable default features")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("zip")
                .long("zip")
                .help("Create a ZIP archive instead of a self-extracting executable")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose output")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("name")
                .long("name")
                .help("Override package name"),
        )
        .arg(
            Arg::new("assets")
                .long("assets")
                .help("Assets to include in the package (comma-separated)")
        )
        .get_matches();

    let project_path = matches.get_one::<String>("input").unwrap();
    let project_name = matches.get_one::<String>("name")
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_project_name(project_path).unwrap_or_else(|_| "unknown".to_string()));
        
    let config = read_config_file(project_path)?;
    let project_name = matches.get_one::<String>("name")
        .map(|s| s.to_string())
        .or_else(|| config.name.clone())
        .unwrap_or_else(|| get_project_name(project_path).unwrap_or_else(|_| "unknown".to_string()));
    
    let projectname = format!("{}.rpack", project_name);
    let output_name = matches
        .get_one::<String>("output")
        .map(|s| s.to_string())
        .or_else(|| config.output.clone())
        .unwrap_or(projectname);

    let targets = matches
        .get_one::<String>("targets")
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>())
        .or_else(|| config.assets.clone())
        .unwrap_or_else(|| vec![get_current_target()]);
        
    let assets = matches
        .get_one::<String>("assets")
        .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
        .or_else(|| config.assets.clone())
        .unwrap_or_else(Vec::new);

    let build_config = BuildConfig {
        strip: matches.get_flag("strip") || config.strip.unwrap_or(false),
        compress: matches.get_flag("compress") || config.compress.unwrap_or(false),
        lto: Some(matches.get_one::<String>("lto").unwrap_or(&config.lto.clone().unwrap_or_else(|| "off".to_string())).clone()),
        debug_symbols: !(matches.get_flag("strip") || config.strip.unwrap_or(false)),
        profile: matches.get_one::<String>("profile")
            .map(|s| s.to_string())
            .or_else(|| config.profile.clone())
            .unwrap_or_else(|| "release".to_string()),
        features: matches
            .get_one::<String>("features")
            .map(|f| f.split(',').map(|s| s.trim().to_string()).collect())
            .or_else(|| config.features.clone())
            .unwrap_or_else(Vec::new),
        assets,
    };

    let verbose = matches.get_flag("verbose") || config.verbose.unwrap_or(false);
    let create_zip = matches.get_flag("zip") || config.zip.unwrap_or(false);
    let watch_mode = matches.get_flag("watch") || config.watch.unwrap_or(false);
    
    if verbose {
        println!("{} Rust project: {}", "Packing".green(), project_path);
        println!("{} for targets: {:?}", "Building".green(), targets);
    }

    if watch_mode {
        watch_and_build(project_path, &output_name, &targets, &build_config, verbose)?;
    } else {
        build_package(project_path, &output_name, &targets, &build_config, verbose, create_zip)?;
    }

    if verbose {
        println!("{} created successfully: {}", "Package".green().bold(), output_name);
    }
    
    Ok(())
}

fn watch_and_build(
    project_path: &str, 
    output_name: &str, 
    targets: &[String],
    build_config: &BuildConfig,
    verbose: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
    
    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_secs(2))?;
    watcher.watch(project_path, RecursiveMode::Recursive)?;

    println!("{} for changes in {}...", "Watching".blue().bold(), project_path);
    
    build_package(project_path, output_name, targets, build_config, verbose, false)?;
    
    let mut last_build = Instant::now();
    
    loop {
        match rx.recv() {
            Ok(_) => {
                if last_build.elapsed() > Duration::from_secs(5) {
                    println!("{} changes, rebuilding...", "Detected".yellow().bold());
                    if let Err(e) = build_package(project_path, output_name, targets, build_config, verbose, false) {
                        println!("{}: {}", "Build failed".red().bold(), e);
                    } else {
                        println!("{}", "Rebuild successful".green().bold());
                    }
                    last_build = Instant::now();
                }
            }
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
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

fn get_project_version(project_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let cargo_toml = Path::new(project_path).join("Cargo.toml");
    let cargo_content = fs::read_to_string(cargo_toml)?;

    cargo_content
        .lines()
        .find_map(|line| {
            if line.trim().starts_with("version =") {
                let parts: Vec<&str> = line.split('=').collect();
                if parts.len() >= 2 {
                    return Some(parts[1].trim().trim_matches('"').to_string());
                }
            }
            None
        })
        .ok_or_else(|| "Could not determine project version from Cargo.toml".into())
}

fn get_project_description(project_path: &str) -> Option<String> {
    let cargo_toml = Path::new(project_path).join("Cargo.toml");
    if let Ok(cargo_content) = fs::read_to_string(cargo_toml) {
        for line in cargo_content.lines() {
            if line.trim().starts_with("description =") {
                let parts: Vec<&str> = line.split('=').collect();
                if parts.len() >= 2 {
                    return Some(parts[1].trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

fn get_current_target() -> String {
    let output = ProcessCommand::new("rustc")
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

fn parse_target(target: &str) -> (String, String, Vec<String>) {
    let parts: Vec<&str> = target.split('-').collect();

    if parts.len() < 2 {
        return ("unknown".to_string(), "unknown".to_string(), vec![]);
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

    let compatibility = match platform.as_str() {
        "windows" => vec!["nt6.1".to_string(), "pe".to_string()],
        "linux" => vec!["glibc-2.17".to_string(), "elf".to_string()],
        "macos" => vec!["10.7".to_string(), "mach-o".to_string()],
        _ => vec![],
    };

    (platform, arch, compatibility)
}

fn build_for_target(
    project_path: &str, 
    bin_dir: &Path, 
    target: &str, 
    project_name: &str, 
    build_config: &BuildConfig,
    verbose: bool,
) -> Result<(PathBuf, Vec<String>), Box<dyn std::error::Error>> {
    let features_args = if build_config.features.is_empty() {
        vec![]
    } else {
        vec!["--features".to_string(), build_config.features.join(",")]
    };

    let mut cargo_args = vec![
        "build".to_string(),
        format!("--{}", build_config.profile),
        "--target".to_string(), 
        target.to_string(),
    ];

    cargo_args.extend(features_args);

    if verbose {
        println!("Running: cargo {}", cargo_args.join(" "));
    }

    let pb = if !verbose {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner} {msg}").unwrap());
        pb.set_message(format!("Building for {}", target));
        Some(pb)
    } else {
        None
    };

    if let Some(lto_type) = &build_config.lto {
        if lto_type != "off" {
            fs::create_dir_all(Path::new(project_path).join(".cargo"))?;
            let config_content = format!(r#"
[profile.release]
lto = "{}"
codegen-units = 1
"#, lto_type);
            fs::write(Path::new(project_path).join(".cargo").join("config.toml"), config_content)?;
        }
    }

    let status = ProcessCommand::new("cargo")
        .current_dir(project_path)
        .args(&cargo_args)
        .status()?;

    if let Some(ref pb) = pb {
        pb.finish_and_clear();
    }

    if !status.success() {
        return Err(format!("Failed to build for target: {}", target).into());
    }

    let binary_path = Path::new(project_path)
        .join("target")
        .join(target)
        .join(&build_config.profile)
        .join(project_name);

    let ext = if target.contains("windows") { ".exe" } else { "" };
    let binary_with_ext = format!("{}{}", project_name, ext);
    let binary_path_with_ext = Path::new(project_path)
        .join("target")
        .join(target)
        .join(&build_config.profile)
        .join(format!("{}{}", project_name, ext));
    
    let dest_path = bin_dir.join(&binary_with_ext);
    fs::copy(&binary_path_with_ext, &dest_path)?;

    if build_config.strip {
        if let Some(pb) = pb.clone() {
            pb.set_message(format!("Stripping debug symbols for {}", target));
            pb.enable_steady_tick(Duration::from_millis(100));
        }
        
        let strip_tool = match target {
            t if t.contains("windows") => "strip",
            t if t.contains("apple") => "strip",
            _ => "strip",
        };

        let strip_status = ProcessCommand::new(strip_tool)
            .arg(&dest_path)
            .status();

        if let Ok(status) = strip_status {
            if verbose && status.success() {
                println!("Successfully stripped debug symbols");
            }
        }
        
        if let Some(pb) = pb.clone() {
            pb.finish_and_clear();
        }
    }

    if build_config.compress {
        if let Some(pb) = pb.clone() {
            pb.set_message(format!("Compressing binary for {}", target));
            pb.enable_steady_tick(Duration::from_millis(100));
        }
        
        let upx_status = ProcessCommand::new("upx")
            .arg("--best")
            .arg(&dest_path)
            .status();

        if let Ok(status) = upx_status {
            if verbose && status.success() {
                println!("Successfully compressed binary with UPX");
            }
        }
        
        if let Some(pb) = pb {
            pb.finish_and_clear();
        }
    }

    let features = build_config.features.clone();
    
    let rel_path = PathBuf::from("bin")
        .join(target)
        .join(&binary_with_ext);

    Ok((rel_path, features))
}

fn calculate_checksum(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    
    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    let result = hasher.finalize();
    
    Ok(format!("{:x}", result))
}

fn sign_package(path: &Path, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    let checksum = calculate_checksum(path)?;
    
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())?;
    mac.update(checksum.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();
    
    Ok(encode(&code_bytes))
}

fn build_package(
    project_path: &str, 
    output_name: &str, 
    targets: &[String],
    build_config: &BuildConfig,
    verbose: bool,
    create_zip: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let rustpack_dir = temp_dir.path().join("rustpack");
    fs::create_dir_all(&rustpack_dir)?;

    let mut target_infos = Vec::new();
    let project_name = get_project_name(project_path)?;
    let version = get_project_version(project_path).unwrap_or_else(|_| "0.1.0".to_string());
    let description = get_project_description(project_path);

    for target in targets {
        let (platform, arch, compatibility) = parse_target(target);
        let bin_dir = rustpack_dir.join("bin").join(target);
        fs::create_dir_all(&bin_dir)?;

        if verbose {
            println!("{} for {}", "Building".blue(), target);
        }
        
        let (binary_path, features) = build_for_target(
            project_path, 
            &bin_dir, 
            target, 
            &project_name, 
            build_config,
            verbose,
        )?;

        let optimizations = if build_config.lto.as_deref() != Some("off") {
            Some(format!("lto-{}", build_config.lto.as_deref().unwrap_or("off")))
        } else {
            None
        };

        target_infos.push(TargetInfo {
            platform,
            arch,
            binary_path: binary_path.to_string_lossy().to_string(),
            features,
            optimizations,
            compatibility,
        });
    }
    
    copy_assets(project_path, &rustpack_dir, &build_config.assets, verbose)?;    

    let mut metadata = HashMap::new();
    metadata.insert("created_with".to_string(), "rustpack".to_string());
    metadata.insert("rust_version".to_string(), get_rust_version());
    
    let checksum = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(16)
        .map(char::from)
        .collect::<String>();

    let enabled_features = vec![
        "cross_platform".to_string(),
        "self_extracting".to_string(),
        "binary_packaging".to_string(),
        "compression".to_string(),
        "auto_detection".to_string(),
    ];

    let package_info = PackageInfo {
        name: project_name,
        version,
        description,
        targets: target_infos,
        created_at: Local::now().to_rfc3339(),
        checksum,
        features: enabled_features,
        metadata,
    };

    let info_json = serde_json::to_string_pretty(&package_info)?;
    fs::write(rustpack_dir.join("info.json"), info_json)?;

    if create_zip {
        create_zip_package(&temp_dir.path(), output_name)?;  
    } else {
        create_self_extracting_package(&temp_dir.path(), output_name)?;
    }

    Ok(())
}

fn create_self_extracting_package(temp_dir: &Path, output_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let temp_archive = tempfile::NamedTempFile::new()?;

    let tar_gz = GzEncoder::new(temp_archive.reopen()?, Compression::default());
    let mut tar = Builder::new(tar_gz);

    for entry in WalkDir::new(temp_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path != temp_dir {
            let name = path.strip_prefix(temp_dir)?;
            if entry.file_type().is_dir() {
                tar.append_dir(name, path)?;
            } else {
                tar.append_path_with_name(path, name)?;
            }
        }
    }

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

    Ok(())
}

fn copy_assets(
    project_path: &str,
    rustpack_dir: &Path,
    assets: &[String],
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if assets.is_empty() {
        return Ok(());
    }
    
    let assets_dir = rustpack_dir.join("assets");
    fs::create_dir_all(&assets_dir)?;
    
    if verbose {
        println!("{} assets", "Copying".blue());
    }
    
    for asset in assets {
        let src_path = Path::new(project_path).join(asset);
        if !src_path.exists() {
            return Err(format!("Asset not found: {}", asset).into());
        }
        
        if src_path.is_dir() {
            let dest_dir = assets_dir.join(asset);
            fs::create_dir_all(&dest_dir)?;
            
            for entry in WalkDir::new(&src_path).into_iter().filter_map(|e| e.ok()) {
                let rel_path = entry.path().strip_prefix(&src_path)?;
                let dest_path = dest_dir.join(rel_path);
                
                if entry.file_type().is_dir() {
                    fs::create_dir_all(&dest_path)?;
                } else {
                    if verbose {
                        println!("  Copying asset: {}", rel_path.display());
                    }
                    fs::copy(entry.path(), &dest_path)?;
                }
            }
        } else {
            let file_name = src_path.file_name().unwrap();
            let dest_path = assets_dir.join(file_name);
            
            if verbose {
                println!("  Copying asset: {}", file_name.to_string_lossy());
            }
            fs::copy(&src_path, &dest_path)?;
        }
    }
    
    Ok(())
}

fn create_zip_package(temp_dir: &Path, output_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(output_name)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    for entry in WalkDir::new(temp_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path != temp_dir {
            let name = path.strip_prefix(temp_dir)?
                .to_string_lossy()
                .to_string();
            
            if entry.file_type().is_dir() {
                zip.add_directory(name, options)?;
            } else {
                zip.start_file(name, options)?;
                let mut f = File::open(path)?;
                let mut buffer = Vec::new();
                f.read_to_end(&mut buffer)?;
                zip.write_all(&buffer)?;
            }
        }
    }

    zip.finish()?;
    Ok(())
}

fn read_config_file(project_path: &str) -> Result<RustPackConfig, Box<dyn std::error::Error>> {
    let config_path = Path::new(project_path).join("RustPack.toml");
    if !config_path.exists() {
        return Ok(RustPackConfig::default());
    }
    
    let config_content = fs::read_to_string(config_path)?;
    let config: RustPackConfig = toml::from_str(&config_content)?;
    Ok(config)
}

fn get_rust_version() -> String {
    let output = ProcessCommand::new("rustc")
        .args(&["--version"])
        .output();
    
    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}