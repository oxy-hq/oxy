fn main() {
    // Tell Cargo when to re-run this build script.
    // Without these, Cargo re-runs on every file change, producing a new
    // BUILD_TIMESTAMP each time and forcing unnecessary recompilation.
    // PROFILE is set by Cargo itself and already triggers rebuilds on profile changes.
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_SERVER_URL");
    println!("cargo:rerun-if-env-changed=GITHUB_REPOSITORY");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_ID");

    // Capture git commit hash from environment variable (set by CI)
    // Default to "dev" for local development builds
    let git_hash_long = std::env::var("GITHUB_SHA").unwrap_or_else(|_| "dev".to_string());

    // Short hash (first 7 chars)
    let git_hash = if git_hash_long.len() >= 7 {
        git_hash_long[..7].to_string()
    } else {
        git_hash_long.clone()
    };

    // Capture build timestamp (UTC)
    let build_timestamp = chrono::Utc::now().to_rfc3339();

    // Capture build profile
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());

    // Capture GitHub CI environment variables (if building in CI)
    let github_server_url = std::env::var("GITHUB_SERVER_URL").unwrap_or_default();
    let github_repository = std::env::var("GITHUB_REPOSITORY").unwrap_or_default();
    let github_run_id = std::env::var("GITHUB_RUN_ID").unwrap_or_default();

    // Set environment variables for use in the binary
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=GIT_HASH_LONG={}", git_hash_long);
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_timestamp);
    println!("cargo:rustc-env=BUILD_PROFILE={}", profile);
    println!("cargo:rustc-env=GITHUB_SERVER_URL={}", github_server_url);
    println!("cargo:rustc-env=GITHUB_REPOSITORY={}", github_repository);
    println!("cargo:rustc-env=GITHUB_RUN_ID={}", github_run_id);
}
