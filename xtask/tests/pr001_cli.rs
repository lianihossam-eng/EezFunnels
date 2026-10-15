#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("eez-xtask-pr001-{}-{sequence}", std::process::id()));
            fs::create_dir_all(root.join("bin")).expect("create fixture");
            let workspace = root.join("workspace");
            for package_directory in [
                "crates/domain",
                "crates/render",
                "crates/storage",
                "crates/commands",
                "apps/api",
                "crates/dashboard-presentation/assets",
                "apps/dashboard",
                "apps/cli",
                "xtask",
                "scripts",
            ] {
                fs::create_dir_all(workspace.join(package_directory))
                    .expect("create workspace fixture package");
            }
            fs::write(workspace.join("scripts/smoke-pr009.sh"), "")
                .expect("write smoke placeholder");
            fs::write(workspace.join("scripts/smoke-pr017.sh"), "")
                .expect("write MCP smoke placeholder");
            fs::write(
                workspace.join("crates/dashboard-presentation/assets/bootstrap.js"),
                "export {};\n",
            )
            .expect("write fixture bootstrap");
            fs::write(
                workspace.join("crates/dashboard-presentation/assets/dashboard.css"),
                "body {}\n",
            )
            .expect("write fixture stylesheet");
            Self { root }
        }

        fn workspace(&self) -> PathBuf {
            self.root.join("workspace")
        }

        fn wrangler(&self, body: &str) -> PathBuf {
            let path = self.root.join("bin/wrangler");
            fs::write(&path, body).expect("write fake wrangler");
            let mut permissions = fs::metadata(&path).expect("read permissions").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("make fake wrangler executable");
            path
        }

        fn cargo(&self, body: &str) -> PathBuf {
            let path = self.root.join("bin/cargo");
            fs::write(&path, body).expect("write fake cargo");
            let mut permissions = fs::metadata(&path).expect("read permissions").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("make fake cargo executable");
            path
        }

        fn wasm_bindgen(&self) -> PathBuf {
            let cargo_home = self.root.join("cargo-home");
            fs::create_dir_all(cargo_home.join("bin")).expect("create fake cargo home");
            let path = cargo_home.join("bin/wasm-bindgen");
            fs::write(
                &path,
                "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'wasm-bindgen 0.2.129'; exit 0; fi\nout_dir=\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = --out-dir ]; then out_dir=$2; shift 2; else shift; fi\ndone\nmkdir -p \"$out_dir\"\nprintf '%s\\n' 'export {};' > \"$out_dir/eezfunnel.js\"\nprintf '\\0' > \"$out_dir/eezfunnel_bg.wasm\"\n",
            )
            .expect("write fake wasm-bindgen");
            let mut permissions = fs::metadata(&path).expect("read permissions").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("make fake wasm-bindgen executable");
            path
        }

        fn invoke(&self, arguments: &[&str], capture: Option<&PathBuf>) -> std::process::Output {
            let system_path = std::env::var_os("PATH").unwrap_or_default();
            let path = format!(
                "{}:{}",
                self.root.join("bin").display(),
                system_path.to_string_lossy()
            );
            let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
            command.args(arguments).env("PATH", path);
            if let Some(capture) = capture {
                command.env("XTASK_CAPTURE", capture);
            }
            command.output().expect("run xtask")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace parent")
            .canonicalize()
            .expect("workspace root")
    }

    fn cargo_metadata(root: &PathBuf) -> serde_json::Value {
        let units = [
            (
                "eez-domain",
                "crates/domain",
                serde_json::json!({"layer": 0}),
            ),
            (
                "eez-render",
                "crates/render",
                serde_json::json!({"layer": 1}),
            ),
            (
                "eez-dashboard-presentation",
                "crates/dashboard-presentation",
                serde_json::json!({"layer": 1}),
            ),
            (
                "eez-storage",
                "crates/storage",
                serde_json::json!({"layer": 2}),
            ),
            (
                "eez-commands",
                "crates/commands",
                serde_json::json!({"layer": 3}),
            ),
            ("eez-api", "apps/api", serde_json::json!({"layer": 4})),
            (
                "eez-dashboard",
                "apps/dashboard",
                serde_json::json!({"layer": 4}),
            ),
            ("eez-cli", "apps/cli", serde_json::json!({"layer": 4})),
            ("xtask", "xtask", serde_json::json!({"role": "tool"})),
        ];
        let packages = units.iter().map(|(name, path, classification)| {
            serde_json::json!({
                "id": name,
                "name": name,
                "source": null,
                "manifest_path": root.join(path).join("Cargo.toml"),
                "metadata": {"eez": classification},
                "dependencies": []
            })
        });
        serde_json::json!({
            "workspace_root": root,
            "workspace_members": units.iter().map(|(name, _, _)| *name).collect::<Vec<_>>(),
            "packages": packages.collect::<Vec<_>>()
        })
    }

    fn run_ci_with_cargo(fixture: &Fixture, cargo_body: &str) -> std::process::Output {
        let cargo = fixture.cargo(cargo_body);
        fixture.wasm_bindgen();
        let metadata = fixture.root.join("metadata.json");
        fs::write(
            &metadata,
            serde_json::to_vec(&cargo_metadata(&fixture.workspace()))
                .expect("serialize Cargo metadata"),
        )
        .expect("write Cargo metadata");
        let capture = fixture.root.join("cargo-arguments.txt");
        let system_path = std::env::var_os("PATH").unwrap_or_default();
        let path = format!(
            "{}:{}",
            fixture.root.join("bin").display(),
            system_path.to_string_lossy()
        );
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("ci")
            .env("CARGO", cargo)
            .env("CARGO_HOME", fixture.root.join("cargo-home"))
            .env("CARGO_TARGET_DIR", fixture.root.join("target"))
            .env("XTASK_WORKSPACE_ROOT", fixture.workspace())
            .env("PATH", path)
            .env("XTASK_CAPTURE", &capture)
            .env("XTASK_METADATA", metadata)
            .output()
            .expect("run xtask ci")
    }

    #[test]
    fn deploy_forwards_arguments_from_workspace_root() {
        let fixture = Fixture::new();
        fixture.wrangler(
            "#!/bin/sh\nprintf 'cwd=%s\\n' \"$PWD\" > \"$XTASK_CAPTURE\"\nfor arg in \"$@\"; do printf 'arg=%s\\n' \"$arg\" >> \"$XTASK_CAPTURE\"; done\n",
        );
        let capture = fixture.root.join("invocation.txt");

        let output = fixture.invoke(
            &["deploy", "--dry-run", "--env", "production"],
            Some(&capture),
        );

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = fs::read_to_string(capture).expect("read invocation");
        assert_eq!(
            actual,
            format!(
                "cwd={}\narg=deploy\narg=--dry-run\narg=--env\narg=production\n",
                workspace_root().display()
            )
        );
    }

    #[test]
    fn deploy_propagates_wrangler_failures() {
        let fixture = Fixture::new();
        fixture.wrangler("#!/bin/sh\nexit 27\n");

        let output = fixture.invoke(&["deploy", "--dry-run"], None);

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("wrangler deploy --dry-run failed"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn ci_runs_all_gates_in_order_and_propagates_audit_failure() {
        let fixture = Fixture::new();
        let output = run_ci_with_cargo(
            &fixture,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$XTASK_CAPTURE\"\nif [ \"$1\" = metadata ]; then cat \"$XTASK_METADATA\"; fi\nif [ \"$1\" = build ] && [ \"$3\" = eez-dashboard ]; then mkdir -p \"$CARGO_TARGET_DIR/wasm32-unknown-unknown/release\"; : > \"$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/eez_dashboard.wasm\"; fi\nif [ \"$1\" = audit ]; then exit 19; fi\n",
        );

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("cargo audit failed"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = fs::read_to_string(fixture.root.join("cargo-arguments.txt"))
            .expect("read Cargo invocations");
        assert_eq!(
            actual,
            "fmt --all -- --check\nclippy --workspace --all-targets --locked -- -D warnings\ntest --workspace --locked\nmetadata --all-features --format-version 1 --locked\nbuild -p eez-api --target wasm32-unknown-unknown --locked\nbuild -p eez-dashboard --target wasm32-unknown-unknown --no-default-features --features browser --release --locked\naudit\n"
        );
    }

    #[test]
    fn ci_runs_funnel_then_mcp_smoke_before_audit() {
        let fixture = Fixture::new();
        fs::write(
            fixture.workspace().join("scripts/smoke-pr009.sh"),
            "printf 'funnel-smoke\\n' >> \"$XTASK_CAPTURE\"\n",
        )
        .expect("write funnel smoke fixture");
        fs::write(
            fixture.workspace().join("scripts/smoke-pr017.sh"),
            "printf 'mcp-smoke\\n' >> \"$XTASK_CAPTURE\"\n",
        )
        .expect("write MCP smoke fixture");
        let output = run_ci_with_cargo(
            &fixture,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$XTASK_CAPTURE\"\nif [ \"$1\" = metadata ]; then cat \"$XTASK_METADATA\"; fi\nif [ \"$1\" = build ] && [ \"$3\" = eez-dashboard ]; then mkdir -p \"$CARGO_TARGET_DIR/wasm32-unknown-unknown/release\"; : > \"$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/eez_dashboard.wasm\"; fi\n",
        );

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = fs::read_to_string(fixture.root.join("cargo-arguments.txt"))
            .expect("read CI and smoke gates");
        let lines = actual.lines().collect::<Vec<_>>();
        let smoke_position = lines
            .iter()
            .position(|line| *line == "funnel-smoke")
            .expect("funnel smoke gate");
        let mcp_smoke_position = lines
            .iter()
            .position(|line| *line == "mcp-smoke")
            .expect("MCP smoke gate");
        let audit_position = lines
            .iter()
            .position(|line| *line == "audit")
            .expect("audit gate");
        assert!(smoke_position < mcp_smoke_position, "{actual}");
        assert!(mcp_smoke_position < audit_position, "{actual}");
    }

    #[test]
    fn ci_propagates_worker_wasm_failure_and_skips_later_gates() {
        let fixture = Fixture::new();
        let output = run_ci_with_cargo(
            &fixture,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$XTASK_CAPTURE\"\nif [ \"$1\" = metadata ]; then cat \"$XTASK_METADATA\"; fi\nif [ \"$1\" = build ] && [ \"$3\" = eez-api ]; then exit 17; fi\n",
        );

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("cargo build -p eez-api --target wasm32-unknown-unknown --locked failed"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = fs::read_to_string(fixture.root.join("cargo-arguments.txt"))
            .expect("read Cargo invocations");
        assert_eq!(
            actual,
            "fmt --all -- --check\nclippy --workspace --all-targets --locked -- -D warnings\ntest --workspace --locked\nmetadata --all-features --format-version 1 --locked\nbuild -p eez-api --target wasm32-unknown-unknown --locked\n"
        );
    }

    #[test]
    fn ci_stops_on_first_failed_check() {
        let fixture = Fixture::new();
        let output = run_ci_with_cargo(
            &fixture,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$XTASK_CAPTURE\"\nexit 13\n",
        );

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("cargo fmt --all -- --check failed"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = fs::read_to_string(fixture.root.join("cargo-arguments.txt"))
            .expect("read Cargo invocations");
        assert_eq!(actual, "fmt --all -- --check\n");
    }
}
