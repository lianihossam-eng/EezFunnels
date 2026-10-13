#![forbid(unsafe_code)]

mod layers;

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, PartialEq, Eq)]
enum Task {
    Check,
    Ci,
    Layers,
    Deploy(Vec<OsString>),
}

impl Task {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let task = match arguments.next().as_deref() {
            Some(argument) if argument == "check" => Self::Check,
            Some(argument) if argument == "ci" => Self::Ci,
            Some(argument) if argument == "layers" => Self::Layers,
            Some(argument) if argument == "deploy" => {
                return Ok(Self::Deploy(arguments.collect()));
            }
            _ => return Err(usage()),
        };
        if arguments.next().is_some() {
            return Err(usage());
        }
        Ok(task)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Cargo(Vec<OsString>),
    Layers,
    Wrangler(Vec<OsString>),
}

fn usage() -> String {
    "usage: cargo xtask <check|ci|layers|deploy [wrangler-args...]>".to_owned()
}

fn cargo_step(arguments: &[&str]) -> Step {
    Step::Cargo(arguments.iter().map(OsString::from).collect())
}

fn check_steps() -> Vec<Step> {
    vec![
        cargo_step(&["fmt", "--all", "--", "--check"]),
        cargo_step(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ]),
        cargo_step(&["test", "--workspace", "--locked"]),
        Step::Layers,
    ]
}

fn steps(task: &Task) -> Vec<Step> {
    match task {
        Task::Check => check_steps(),
        Task::Ci => {
            let mut steps = check_steps();
            steps.push(cargo_step(&[
                "build",
                "-p",
                "eez-api",
                "--target",
                "wasm32-unknown-unknown",
                "--locked",
            ]));
            steps.push(cargo_step(&[
                "build",
                "-p",
                "eez-dashboard",
                "--target",
                "wasm32-unknown-unknown",
                "--locked",
            ]));
            steps.push(cargo_step(&["audit"]));
            steps
        }
        Task::Layers => vec![Step::Layers],
        Task::Deploy(arguments) => {
            let mut wrangler_arguments = vec![OsString::from("deploy")];
            wrangler_arguments.extend(arguments.iter().cloned());
            vec![Step::Wrangler(wrangler_arguments)]
        }
    }
}

fn execute_steps(
    steps: &[Step],
    mut execute: impl FnMut(&Step) -> Result<(), String>,
) -> Result<(), String> {
    for step in steps {
        execute(step)?;
    }
    Ok(())
}

fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn display_arguments(arguments: &[OsString]) -> String {
    arguments
        .iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

fn run_command(
    root: &Path,
    executable: &OsStr,
    label: &str,
    description: &str,
    arguments: &[OsString],
) -> Result<(), String> {
    let status = Command::new(executable)
        .args(arguments)
        .current_dir(root)
        .status()
        .map_err(|error| format!("cannot execute {label}: {error}"))?;
    if !status.success() {
        return Err(format!("{description} failed ({status})"));
    }
    Ok(())
}

fn run_cargo(root: &Path, cargo: &OsStr, arguments: &[OsString]) -> Result<(), String> {
    let description = format!("cargo {}", display_arguments(arguments));
    run_command(root, cargo, "Cargo", &description, arguments)
}

fn run_wrangler(root: &Path, arguments: &[OsString]) -> Result<(), String> {
    let description = format!("wrangler {}", display_arguments(arguments));
    run_command(
        root,
        OsStr::new("wrangler"),
        "Wrangler",
        &description,
        arguments,
    )
}

fn run(task: &Task, root: &Path, cargo: &OsStr) -> Result<(), String> {
    let steps = steps(task);
    execute_steps(&steps, |step| match step {
        Step::Cargo(arguments) => run_cargo(root, cargo, arguments),
        Step::Layers => layers::WorkspaceGraph::read(root, cargo)?.verify_layers(),
        Step::Wrangler(arguments) => run_wrangler(root, arguments),
    })?;
    writeln!(io::stdout().lock(), "xtask task completed: {task:?}")
        .map_err(|error| format!("cannot write output: {error}"))
}

fn main() -> ExitCode {
    let result = Task::parse(std::env::args_os().skip(1)).and_then(|task| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = root
            .parent()
            .ok_or_else(|| "xtask manifest has no workspace parent".to_owned())?;
        run(&task, root, &cargo_executable())
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
    fn task_arguments_are_closed_except_for_deploy() {
        assert_eq!(Task::parse(["check".into()]), Ok(Task::Check));
        assert_eq!(Task::parse(["ci".into()]), Ok(Task::Ci));
        assert_eq!(Task::parse(["layers".into()]), Ok(Task::Layers));
        assert_eq!(Task::parse(["deploy".into()]), Ok(Task::Deploy(Vec::new())));
        assert_eq!(
            Task::parse(["deploy".into(), "--env".into(), "production".into()]),
            Ok(Task::Deploy(vec!["--env".into(), "production".into()]))
        );
        for arguments in [
            vec![],
            vec!["unknown"],
            vec!["check", "ignored"],
            vec!["ci", "ignored"],
            vec!["layers", "ignored"],
        ] {
            assert_eq!(
                Task::parse(arguments.into_iter().map(OsString::from)),
                Err(usage())
            );
        }
    }

    #[test]
    fn ci_steps_run_check_then_wasm_then_audit() {
        assert_eq!(
            steps(&Task::Ci),
            vec![
                cargo_step(&["fmt", "--all", "--", "--check"]),
                cargo_step(&[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ]),
                cargo_step(&["test", "--workspace", "--locked"]),
                Step::Layers,
                cargo_step(&[
                    "build",
                    "-p",
                    "eez-api",
                    "--target",
                    "wasm32-unknown-unknown",
                    "--locked",
                ]),
                cargo_step(&[
                    "build",
                    "-p",
                    "eez-dashboard",
                    "--target",
                    "wasm32-unknown-unknown",
                    "--locked",
                ]),
                cargo_step(&["audit"]),
            ]
        );
    }

    #[test]
    fn pipeline_stops_at_the_first_failure() {
        let steps = steps(&Task::Ci);
        let failure = "clippy failed".to_owned();
        let mut executed = Vec::new();
        let result = execute_steps(&steps, |step| {
            executed.push((*step).clone());
            if step
                == &cargo_step(&[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ])
            {
                Err(failure.clone())
            } else {
                Ok(())
            }
        });

        assert_eq!(result, Err(failure));
        assert_eq!(
            executed,
            vec![
                cargo_step(&["fmt", "--all", "--", "--check"]),
                cargo_step(&[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ]),
            ]
        );
    }

    #[test]
    fn deploy_prepends_wrangler_command_and_preserves_arguments() {
        assert_eq!(
            steps(&Task::Deploy(vec![
                "--dry-run".into(),
                "--env".into(),
                "production".into()
            ])),
            vec![Step::Wrangler(vec![
                "deploy".into(),
                "--dry-run".into(),
                "--env".into(),
                "production".into(),
            ])]
        );
    }

    #[test]
    fn broken_cargo_prerequisite_fails_each_cargo_task() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let missing_cargo = root.join("no-such-cargo-executable");
        for task in [Task::Check, Task::Ci, Task::Layers] {
            let error = run(&task, root, missing_cargo.as_os_str()).expect_err("must fail");
            assert!(error.starts_with("cannot execute Cargo:"), "{error}");
        }
    }

    #[test]
    fn failing_cargo_status_is_preserved() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let error = run_cargo(
            root,
            &cargo_executable(),
            &[OsString::from("no-such-eez-command")],
        )
        .expect_err("Cargo must reject an unknown command");
        assert!(error.starts_with("cargo no-such-eez-command failed ("));
    }

    #[test]
    fn missing_wrangler_is_reported() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let missing_wrangler = root.join("no-such-wrangler-executable");
        let error = run_command(
            root,
            missing_wrangler.as_os_str(),
            "Wrangler",
            "wrangler deploy",
            &[OsString::from("deploy")],
        )
        .expect_err("missing Wrangler must fail");
        assert!(error.starts_with("cannot execute Wrangler:"), "{error}");
    }
}
