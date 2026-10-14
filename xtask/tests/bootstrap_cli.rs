use std::process::Command;

#[test]
fn xtask_unknown_task_and_broken_cargo_exit_nonzero() {
    let binary = env!("CARGO_BIN_EXE_xtask");
    let unknown = Command::new(binary)
        .arg("unknown")
        .output()
        .expect("run xtask");
    assert_eq!(unknown.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(unknown.stderr).expect("UTF-8"),
        "usage: cargo xtask <check|ci|dashboard-assets|layers|deploy [wrangler-args...]>\n"
    );
    for task in ["check", "ci", "layers"] {
        let failed = Command::new(binary)
            .arg(task)
            .env("CARGO", "/no-such-eez-cargo-executable")
            .output()
            .expect("run xtask with broken prerequisite");
        assert_eq!(failed.status.code(), Some(1));
        assert!(String::from_utf8(failed.stderr)
            .expect("UTF-8")
            .starts_with("cannot execute Cargo:"));
    }
}

#[test]
fn eez_cli_help_is_honest_and_configuration_errors_fail() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let cargo = std::env::var_os("CARGO").expect("Cargo executable");
    let help = Command::new(&cargo)
        .args([
            "run", "--locked", "--quiet", "-p", "eez-cli", "--", "--help",
        ])
        .current_dir(root)
        .output()
        .expect("run CLI help");
    assert_eq!(help.status.code(), Some(0));
    let help = String::from_utf8(help.stdout).expect("UTF-8");
    assert!(help.starts_with("Eez Funnel CLI\nUsage:"));
    assert!(help.contains("eez funnels create <name>"));
    assert!(help.contains("eez deploy [wrangler-args...]"));
    let dir = std::env::temp_dir().join(format!("eez-bootstrap-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let missing = Command::new(&cargo)
        .args([
            "run", "--locked", "--quiet", "-p", "eez-cli", "--", "funnels", "list",
        ])
        .env("EEZ_CONFIG_DIR", &dir)
        .current_dir(root)
        .output()
        .expect("run missing-config CLI command");
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8(missing.stderr)
        .expect("UTF-8")
        .contains("eez config set endpoint"));
    let failed = Command::new(&cargo)
        .args([
            "run",
            "--locked",
            "--quiet",
            "-p",
            "eez-cli",
            "--",
            "--unknown",
        ])
        .current_dir(root)
        .output()
        .expect("run unsupported CLI command");
    assert_eq!(failed.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(failed.stderr).expect("UTF-8"),
        "unsupported command or arguments; use eez --help\n"
    );
}
