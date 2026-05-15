use std::process::Command;

#[test]
fn ok_version_prints_cargo_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_ok"))
        .arg("--version")
        .output()
        .expect("ok --version should run");

    assert!(
        output.status.success(),
        "ok --version should exit successfully"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("ok {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(
        output.stderr.is_empty(),
        "ok --version should not write to stderr"
    );
}
