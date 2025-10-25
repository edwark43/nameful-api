use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=.git/refs/heads/main");
    let output = Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap_or_default();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
