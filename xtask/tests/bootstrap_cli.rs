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
fn eez_cli_help_is_honest_and_unsupported_commands_fail() {
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
    assert_eq!(String::from_utf8(help.stdout).expect("UTF-8"), "Eez Funnel CLI (PR-000 workspace bootstrap)\nUsage: eez [--help]\nFunnel commands are not implemented.\n");
    for arguments in [["funnels", "list"].as_slice(), ["--unknown"].as_slice()] {
        let failed = Command::new(&cargo)
            .args(["run", "--locked", "--quiet", "-p", "eez-cli", "--"])
            .args(arguments)
            .current_dir(root)
            .output()
            .expect("run unsupported CLI command");
        assert_eq!(failed.status.code(), Some(1));
        assert_eq!(
            String::from_utf8(failed.stderr).expect("UTF-8"),
            "Funnel commands are not implemented in the PR-000 bootstrap. Use eez --help.\n"
        );
    }
}
