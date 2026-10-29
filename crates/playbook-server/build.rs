use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let version_file = manifest.join("../../VERSION");
    println!("cargo:rerun-if-changed={}", version_file.display());
    let version = fs::read_to_string(version_file)
        .expect("workspace VERSION is required")
        .trim()
        .to_string();
    let mut components = version.split('.');
    let valid = components.by_ref().all(|component| {
        !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
    }) && components.count() == 0
        && version.split('.').count() == 3;
    assert!(valid, "workspace VERSION must be A.B.C");
    println!("cargo:rustc-env=PLAYBOOK_VERSION={version}");
}
