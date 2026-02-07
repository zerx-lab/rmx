use std::path::PathBuf;
use std::process::Command;

fn main() {
    let version = if let Ok(ci_version) = std::env::var("CI_VERSION") {
        ci_version
    } else {
        std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string())
    };

    println!("cargo::rustc-env=APP_VERSION={}", version);

    if cfg!(windows) {
        build_rmx_shell();
    }
}

fn build_rmx_shell() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let shell_dir = manifest_dir.join("rmx-shell");

    if !shell_dir.exists() {
        panic!("rmx-shell directory not found at {}", shell_dir.display());
    }

    if let Ok(dll_path) = std::env::var("RMX_SHELL_DLL_PATH") {
        println!("cargo::rustc-env=RMX_SHELL_DLL_PATH={}", dll_path);
        println!("cargo::rerun-if-env-changed=RMX_SHELL_DLL_PATH");
        return;
    }

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let cargo_args: Vec<&str> = if profile == "release" {
        vec!["build", "--release"]
    } else {
        vec!["build"]
    };

    let status = Command::new("cargo")
        .args(&cargo_args)
        .current_dir(&shell_dir)
        .status()
        .expect("Failed to run cargo build for rmx-shell");

    if !status.success() {
        panic!("Failed to build rmx-shell");
    }

    let dll_path = manifest_dir
        .join("rmx-shell")
        .join("target")
        .join(&profile)
        .join("rmx_shell.dll");

    if !dll_path.exists() {
        panic!(
            "rmx-shell DLL not found at {}. Build may have failed.",
            dll_path.display()
        );
    }

    println!("cargo::rustc-env=RMX_SHELL_DLL_PATH={}", dll_path.display());

    println!("cargo::rerun-if-changed=rmx-shell/src/");
    println!("cargo::rerun-if-changed=rmx-shell/Cargo.toml");
}
