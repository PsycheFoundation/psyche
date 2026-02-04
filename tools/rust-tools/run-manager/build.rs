use std::process::Command;

fn main() {
    // Get git hash
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Check if repo is dirty
    let is_dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map(|output| !output.stdout.is_empty())
        .unwrap_or(false);

    let git_hash = if is_dirty {
        format!("{}-dirty", git_hash)
    } else {
        git_hash
    };

    // Get build timestamp
    let build_timestamp = chrono::Utc::now().to_rfc3339();

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_timestamp);
}
