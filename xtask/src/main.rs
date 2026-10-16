#![forbid(unsafe_code)]

mod layers;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, PartialEq, Eq)]
enum Task {
    Check,
    Ci,
    DashboardAssets,
    Layers,
    Deploy(Vec<OsString>),
}

impl Task {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let task = match arguments.next().as_deref() {
            Some(argument) if argument == "check" => Self::Check,
            Some(argument) if argument == "ci" => Self::Ci,
            Some(argument) if argument == "dashboard-assets" => Self::DashboardAssets,
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
    DashboardAssets,
    FunnelSmoke,
    PublicFormsSmoke,
    FormContextMigration,
    McpSmoke,
    Layers,
    Wrangler(Vec<OsString>),
}

fn usage() -> String {
    "usage: cargo xtask <check|ci|dashboard-assets|layers|deploy [wrangler-args...]>".to_owned()
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
            steps.push(Step::DashboardAssets);
            steps.push(Step::FunnelSmoke);
            steps.push(Step::PublicFormsSmoke);
            steps.push(Step::FormContextMigration);
            steps.push(Step::McpSmoke);
            steps.push(cargo_step(&["audit"]));
            steps
        }
        Task::DashboardAssets => vec![Step::DashboardAssets],
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

fn run_funnel_smoke(root: &Path) -> Result<(), String> {
    run_command(
        root,
        OsStr::new("bash"),
        "Bash",
        "PR-009 funnel smoke",
        &[OsString::from("scripts/smoke-pr009.sh")],
    )
}

fn run_mcp_smoke(root: &Path) -> Result<(), String> {
    run_command(
        root,
        OsStr::new("bash"),
        "Bash",
        "PR-017 Worker MCP smoke",
        &[OsString::from("scripts/smoke-pr017.sh")],
    )
}

fn run_public_forms_smoke(root: &Path) -> Result<(), String> {
    run_command(
        root,
        OsStr::new("bash"),
        "Bash",
        "PR-033 Worker public form smoke",
        &[OsString::from("scripts/smoke-pr033-public-forms.sh")],
    )
}

fn run_form_context_migration(root: &Path) -> Result<(), String> {
    run_command(
        root,
        OsStr::new("python3"),
        "Python",
        "public form context migration regression test",
        &[OsString::from(
            "scripts/test-public-form-context-migration.py",
        )],
    )
}

fn run_dashboard_assets(root: &Path, cargo: &OsStr) -> Result<(), String> {
    let wasm_bindgen = wasm_bindgen_executable();
    let wasm_bindgen_version = Command::new(&wasm_bindgen)
        .arg("--version")
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "wasm-bindgen 0.2.129 is required; run scripts/install-wasm-bindgen.sh".to_owned()
            } else {
                format!("cannot execute wasm-bindgen: {error}")
            }
        })?;
    let found_version = String::from_utf8_lossy(&wasm_bindgen_version.stdout);
    if !wasm_bindgen_version.status.success() || found_version.trim() != "wasm-bindgen 0.2.129" {
        return Err(format!(
            "wasm-bindgen 0.2.129 is required, found {}; run scripts/install-wasm-bindgen.sh",
            found_version.trim()
        ));
    }

    run_cargo(
        root,
        cargo,
        &[
            OsString::from("build"),
            OsString::from("-p"),
            OsString::from("eez-dashboard"),
            OsString::from("--target"),
            OsString::from("wasm32-unknown-unknown"),
            OsString::from("--no-default-features"),
            OsString::from("--features"),
            OsString::from("browser"),
            OsString::from("--release"),
            OsString::from("--locked"),
        ],
    )?;

    let output_directory = root.join("apps/dashboard/dist/assets");
    if output_directory.exists() {
        fs::remove_dir_all(&output_directory)
            .map_err(|error| format!("cannot clear dashboard assets: {error}"))?;
    }
    fs::create_dir_all(&output_directory)
        .map_err(|error| format!("cannot create dashboard assets directory: {error}"))?;

    let target_directory = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let target_directory = if target_directory.is_absolute() {
        target_directory
    } else {
        root.join(target_directory)
    };
    let input_wasm = target_directory.join("wasm32-unknown-unknown/release/eez_dashboard.wasm");
    let wasm_bindgen_arguments = [
        OsString::from("--target"),
        OsString::from("web"),
        OsString::from("--no-typescript"),
        OsString::from("--out-name"),
        OsString::from("eezfunnel"),
        OsString::from("--out-dir"),
        output_directory.clone().into_os_string(),
        input_wasm.into_os_string(),
    ];
    run_command(
        root,
        wasm_bindgen.as_os_str(),
        "wasm-bindgen",
        "wasm-bindgen dashboard bundle",
        &wasm_bindgen_arguments,
    )?;

    for name in ["bootstrap.js", "dashboard.css"] {
        fs::copy(
            root.join("crates/dashboard-presentation/assets").join(name),
            output_directory.join(name),
        )
        .map_err(|error| format!("cannot copy dashboard asset {name}: {error}"))?;
    }

    for name in [
        "bootstrap.js",
        "dashboard.css",
        "eezfunnel.js",
        "eezfunnel_bg.wasm",
    ] {
        let path = output_directory.join(name);
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("dashboard asset {name} was not generated: {error}"))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!("dashboard asset {name} is empty or not a file"));
        }
    }

    validate_module_imports(&output_directory)?;

    writeln!(
        io::stdout().lock(),
        "dashboard assets generated at {}",
        output_directory.display()
    )
    .map_err(|error| format!("cannot write output: {error}"))
}

fn validate_module_imports(output_directory: &Path) -> Result<(), String> {
    let entry_point = output_directory.join("eezfunnel.js");
    let source = fs::read_to_string(&entry_point).map_err(|error| {
        format!(
            "cannot read dashboard module {}: {error}",
            entry_point.display()
        )
    })?;

    for line in source.lines() {
        let Some((_, specifier)) = line.split_once("from ") else {
            continue;
        };
        let Some(quote) = specifier.chars().next() else {
            continue;
        };
        if quote != '\'' && quote != '"' {
            continue;
        }
        let Some(after_quote) = specifier.get(quote.len_utf8()..) else {
            continue;
        };
        let Some((module, _)) = after_quote.split_once(quote) else {
            continue;
        };
        let Some(relative_module) = module.strip_prefix("./") else {
            continue;
        };
        if Path::new(relative_module)
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(format!("unsafe relative dashboard module import: {module}"));
        }
        let module_path = output_directory.join(relative_module);
        let metadata = fs::metadata(&module_path)
            .map_err(|error| format!("dashboard module import {module} is missing: {error}"))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!(
                "dashboard module import {module} is empty or not a file"
            ));
        }
    }

    Ok(())
}

fn wasm_bindgen_executable() -> PathBuf {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .map(|directory| directory.join("bin/wasm-bindgen"))
        .unwrap_or_else(|| PathBuf::from("wasm-bindgen"))
}

fn run(task: &Task, root: &Path, cargo: &OsStr) -> Result<(), String> {
    let steps = steps(task);
    execute_steps(&steps, |step| match step {
        Step::Cargo(arguments) => run_cargo(root, cargo, arguments),
        Step::DashboardAssets => run_dashboard_assets(root, cargo),
        Step::FunnelSmoke => run_funnel_smoke(root),
        Step::PublicFormsSmoke => run_public_forms_smoke(root),
        Step::FormContextMigration => run_form_context_migration(root),
        Step::McpSmoke => run_mcp_smoke(root),
        Step::Layers => layers::WorkspaceGraph::read(root, cargo)?.verify_layers(),
        Step::Wrangler(arguments) => run_wrangler(root, arguments),
    })?;
    writeln!(io::stdout().lock(), "xtask task completed: {task:?}")
        .map_err(|error| format!("cannot write output: {error}"))
}

fn main() -> ExitCode {
    let result = Task::parse(std::env::args_os().skip(1)).and_then(|task| {
        let root = match std::env::var_os("XTASK_WORKSPACE_ROOT") {
            Some(root) => PathBuf::from(root),
            None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .ok_or_else(|| "xtask manifest has no workspace parent".to_owned())?
                .to_path_buf(),
        };
        run(&task, &root, &cargo_executable())
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
        assert_eq!(
            Task::parse(["dashboard-assets".into()]),
            Ok(Task::DashboardAssets)
        );
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
    fn ci_steps_run_check_then_wasm_then_product_smokes_and_migrations_then_audit() {
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
                Step::DashboardAssets,
                Step::FunnelSmoke,
                Step::PublicFormsSmoke,
                Step::FormContextMigration,
                Step::McpSmoke,
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

    #[test]
    fn dashboard_assets_task_runs_only_the_asset_bundle_step() {
        assert_eq!(steps(&Task::DashboardAssets), vec![Step::DashboardAssets]);
        assert_eq!(
            Task::parse(["dashboard-assets".into(), "ignored".into()]),
            Err(usage())
        );
    }
}
