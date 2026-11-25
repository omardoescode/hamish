use std::process::Command;

#[test]
fn core_has_no_gpui_dependency() {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "ka-teb-core",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let tree = String::from_utf8_lossy(&out.stdout);
    assert!(
        !tree.lines().any(|line| line.starts_with("gpui ")),
        "ka-teb-core depends on gpui (ADR 0005)"
    );
}
