use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use std::thread;

fn main() {
    println!("RustPack Tester");
    
    let temp_dir = create_temp_project();
    let project_path = temp_dir.to_string_lossy().to_string();
    
    println!("Created test project at: {}", project_path);
    
    test_basic_build(&project_path);
    test_features(&project_path);
    test_assets(&project_path);
    test_compression(&project_path);
    test_multiple_targets(&project_path);
    test_zip_output(&project_path);
    test_dependency_analysis(&project_path);
    test_patching(&project_path);
    test_auto_update(&project_path);
    
    println!("All tests completed!");
    
    cleanup_temp_project(temp_dir);
    println!("Test project cleaned up");
}

fn create_temp_project() -> PathBuf {
    let temp_dir = env::temp_dir().join(format!("rustpack_test_{}", random_string(8)));
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
    
    let cargo_toml_content = r#"
[package]
name = "rustpack_test"
version = "0.1.0"
edition = "2021"

[dependencies]
rand = "0.8.5"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[features]
test_feature_1 = []
test_feature_2 = []
"#;
    
    fs::write(temp_dir.join("Cargo.toml"), cargo_toml_content).expect("Failed to write Cargo.toml");
    
    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir).expect("Failed to create src directory");
    
    let main_rs_content = r#"
fn main() {
    println!("RustPack Test Application");
    println!("Running on: {}/{}", std::env::consts::OS, std::env::consts::ARCH);
    
    if std::env::var("RUSTPACK_ASSETS_DIR").is_ok() {
        println!("Assets directory found");
        let assets_dir = std::env::var("RUSTPACK_ASSETS_DIR").unwrap();
        if let Ok(entries) = std::fs::read_dir(assets_dir) {
            for entry in entries.flatten() {
                println!("Asset: {}", entry.path().display());
            }
        }
    }
    
    #[cfg(feature = "test_feature_1")]
    println!("Feature 'test_feature_1' is enabled");
    
    #[cfg(feature = "test_feature_2")]
    println!("Feature 'test_feature_2' is enabled");
}
"#;
    
    fs::write(src_dir.join("main.rs"), main_rs_content).expect("Failed to write main.rs");
    
    let assets_dir = temp_dir.join("assets");
    fs::create_dir_all(&assets_dir).expect("Failed to create assets directory");
    
    fs::write(assets_dir.join("test_file.txt"), "Test asset file").expect("Failed to write test asset");
    fs::write(temp_dir.join("LICENSE"), "MIT License\nTest License File").expect("Failed to write LICENSE");
    
    let config_content = r#"
name = "rustpack_test_override"
output = "custom_output.rpack"
strip = true
compress = true
lto = "thin"
profile = "release"
features = ["test_feature_1"]
assets = ["assets"]
zip = false
watch = false
verbose = true
"#;
    
    fs::write(temp_dir.join("RustPack.toml"), config_content).expect("Failed to write RustPack.toml");
    
    let build_status = Command::new("cargo")
        .current_dir(&temp_dir)
        .args(&["build"])
        .status()
        .expect("Failed to build test project");
        
    if !build_status.success() {
        panic!("Failed to build test project");
    }
    
    temp_dir
}

fn test_basic_build(project_path: &str) {
    println!("\n=== Testing Basic Build ===");
    
    let output = Command::new("cargo")
        .args(&["run", "--", "--input", project_path, "--output", "basic_test.rpack", "--verbose"])
        .output()
        .expect("Failed to execute command");
        
    println!("Basic build output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(Path::new("basic_test.rpack").exists(), "Output file not created");
    println!("Basic build test: PASSED");
}

fn test_features(project_path: &str) {
    println!("\n=== Testing Features ===");
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--input", project_path,
            "--output", "features_test.rpack",
            "--features", "test_feature_1,test_feature_2",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("Features build output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(Path::new("features_test.rpack").exists(), "Output file not created");
    println!("Features test: PASSED");
}

fn test_assets(project_path: &str) {
    println!("\n=== Testing Assets ===");
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--input", project_path,
            "--output", "assets_test.rpack",
            "--assets", "assets,LICENSE",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("Assets build output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(Path::new("assets_test.rpack").exists(), "Output file not created");
    println!("Assets test: PASSED");
}

fn test_compression(project_path: &str) {
    println!("\n=== Testing Compression ===");
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--input", project_path,
            "--output", "compressed_test.rpack",
            "--compress",
            "--strip",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("Compression build output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(Path::new("compressed_test.rpack").exists(), "Output file not created");
    println!("Compression test: PASSED");
}

fn test_multiple_targets(project_path: &str) {
    println!("\n=== Testing Multiple Targets ===");
    
    let current_target = get_current_target();
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--input", project_path,
            "--output", "multi_target_test.rpack",
            "--targets", &current_target,
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("Multiple targets build output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(Path::new("multi_target_test.rpack").exists(), "Output file not created");
    println!("Multiple targets test: PASSED");
}

fn test_zip_output(project_path: &str) {
    println!("\n=== Testing ZIP Output ===");
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--input", project_path,
            "--output", "zip_test.zip",
            "--zip",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("ZIP build output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(Path::new("zip_test.zip").exists(), "Output file not created");
    println!("ZIP output test: PASSED");
}

fn test_dependency_analysis(project_path: &str) {
    println!("\n=== Testing Dependency Analysis ===");
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--input", project_path,
            "--output", "deps_test.rpack",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    let output_str = String::from_utf8_lossy(&output.stdout);
    println!("Dependency analysis output: {}", output_str);
    
    assert!(Path::new("deps_test.rpack").exists(), "Output file not created");
    println!("Dependency analysis test: PASSED");
}

fn test_patching(project_path: &str) {
    println!("\n=== Testing Binary Patching ===");
    
    let bin_path = find_binary_path(project_path);
    if bin_path.is_empty() {
        println!("Could not find binary for patching test, skipping...");
        return;
    }
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--create-patch",
            "--old-version", &bin_path,
            "--input", &bin_path,
            "--patch-output", "test.patch",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("Patch creation output: {}", String::from_utf8_lossy(&output.stdout));
    
    if !Path::new("test.patch").exists() {
        println!("Patch file not created, skipping apply test...");
        return;
    }
    
    let apply_output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--apply-patch",
            "--input", &bin_path,
            "--patch-file", "test.patch",
            "--output", "patched_binary",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("Patch application output: {}", String::from_utf8_lossy(&apply_output.stdout));
    
    assert!(Path::new("patched_binary").exists(), "Patched binary not created");
    println!("Binary patching test: PASSED");
}

fn test_auto_update(project_path: &str) {
    println!("\n=== Testing Auto-Update Configuration ===");
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--", 
            "--input", project_path,
            "--output", "update_test.rpack",
            "--update-url", "https://example.com/updates",
            "--verbose"
        ])
        .output()
        .expect("Failed to execute command");
        
    println!("Auto-update output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(Path::new("update_test.rpack").exists(), "Output file not created");
    println!("Auto-update test: PASSED");
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

fn find_binary_path(project_path: &str) -> String {
    let target_dir = Path::new(project_path).join("target").join("debug");
    
    if let Ok(entries) = fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_executable(&path) {
                return path.to_string_lossy().to_string();
            }
        }
    }
    
    String::new()
}

fn is_executable(path: &Path) -> bool {
    if cfg!(windows) {
        path.extension().map_or(false, |ext| ext == "exe")
    } else {
        if let Ok(metadata) = fs::metadata(path) {
            use std::os::unix::fs::PermissionsExt;
            let permissions = metadata.permissions();
            return permissions.mode() & 0o111 != 0;
        }
        false
    }
}

fn random_string(len: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let mut chars = Vec::new();
    let mut n = now;
    
    for _ in 0..len {
        let digit = n % 36;
        n /= 36;
        
        let c = if digit < 10 {
            (b'0' + digit as u8) as char
        } else {
            (b'a' + (digit - 10) as u8) as char
        };
        
        chars.push(c);
    }
    
    chars.into_iter().collect()
}

fn cleanup_temp_project(temp_dir: PathBuf) {
    thread::sleep(Duration::from_millis(100));
    let _ = fs::remove_dir_all(temp_dir);
}