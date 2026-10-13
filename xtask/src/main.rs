#![forbid(unsafe_code)]

mod layers;

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, PartialEq)]
enum Task {
    Check,
    Layers,
}

impl Task {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let task = match arguments.next().as_deref() {
            Some(argument) if argument == "check" => Self::Check,
            Some(argument) if argument == "layers" => Self::Layers,
            _ => return Err("usage: cargo xtask <check|layers>".to_owned()),
        };
        if arguments.next().is_some() {
            return Err("usage: cargo xtask <check|layers>".to_owned());
        }
        Ok(task)
    }
}

fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn run_cargo(root: &Path, cargo: &OsStr, arguments: &[&str]) -> Result<(), String> {
    let status = Command::new(cargo)
        .args(arguments)
        .current_dir(root)
        .status()
        .map_err(|error| format!("cannot execute Cargo: {error}"))?;
    if !status.success() {
        return Err(format!("cargo {} failed ({status})", arguments.join(" ")));
    }
    Ok(())
}

fn run(task: Task, root: &Path, cargo: &OsStr) -> Result<(), String> {
    if task == Task::Check {
        for arguments in [
            &["fmt", "--all", "--", "--check"][..],
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
            &["test", "--workspace", "--locked"],
        ] {
            run_cargo(root, cargo, arguments)?;
        }
    }
    layers::WorkspaceGraph::read(root, cargo)?.verify_layers()?;
    writeln!(io::stdout().lock(), "workspace checks passed: {task:?}")
        .map_err(|error| format!("cannot write output: {error}"))
}

fn main() -> ExitCode {
    let result = Task::parse(std::env::args_os().skip(1)).and_then(|task| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = root
            .parent()
            .ok_or_else(|| "xtask manifest has no workspace parent".to_owned())?;
        run(task, root, &cargo_executable())
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_arguments_are_closed() {
        assert_eq!(Task::parse(["check".into()]), Ok(Task::Check));
        assert_eq!(Task::parse(["layers".into()]), Ok(Task::Layers));
        for arguments in [vec![], vec!["ci"], vec!["check", "ignored"]] {
            assert_eq!(
                Task::parse(arguments.into_iter().map(OsString::from)),
                Err("usage: cargo xtask <check|layers>".to_owned())
            );
        }
    }

    #[test]
    fn broken_cargo_prerequisite_fails_check_and_layers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let missing_cargo = root.join("no-such-cargo-executable");
        for task in [Task::Check, Task::Layers] {
            let error = run(task, root, missing_cargo.as_os_str()).expect_err("must fail");
            assert!(error.starts_with("cannot execute Cargo:"), "{error}");
        }
    }

    #[test]
    fn failing_cargo_status_is_preserved() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let error = run_cargo(root, &cargo_executable(), &["no-such-eez-command"])
            .expect_err("Cargo must reject an unknown command");
        assert!(error.starts_with("cargo no-such-eez-command failed ("));
    }
}
