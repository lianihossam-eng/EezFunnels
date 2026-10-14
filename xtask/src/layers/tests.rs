use super::*;
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    metadata: Value,
}

impl Fixture {
    fn new() -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("eez-layer-test-{}-{sequence}", std::process::id()));
        fs::create_dir(&root).expect("create fixture root");
        let mut packages = Vec::new();
        for (name, path, role) in UNITS {
            let directory = root.join(path);
            fs::create_dir_all(directory.join("src")).expect("create source directory");
            fs::write(directory.join("src/lib.rs"), "").expect("write source");
            let (eez, classification) = match role {
                Role::Product(layer) => (json!({"layer": layer}), format!("layer = {layer}")),
                Role::Tool => (json!({"role": "tool"}), "role = \"tool\"".to_owned()),
            };
            fs::write(directory.join("Cargo.toml"), format!(
                "[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2021\"\n[package.metadata.eez]\n{classification}\n"
            )).expect("write member manifest");
            packages.push(json!({
                "id": name,
                "name": name,
                "source": null,
                "manifest_path": directory.join("Cargo.toml"),
                "metadata": {"eez": eez},
                "dependencies": []
            }));
        }
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace]\nresolver = \"2\"\nmembers = [{}]\n",
                UNITS
                    .iter()
                    .map(|(_, path, _)| format!("{path:?}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        )
        .expect("write workspace manifest");
        let metadata = json!({
            "workspace_root": root,
            "workspace_members": UNITS.iter().map(|(name, _, _)| *name).collect::<Vec<_>>(),
            "packages": packages
        });
        Self { root, metadata }
    }

    fn package(&mut self, name: &str) -> &mut Value {
        self.metadata
            .get_mut("packages")
            .and_then(Value::as_array_mut)
            .expect("packages")
            .iter_mut()
            .find(|package| package.get("name").and_then(Value::as_str) == Some(name))
            .expect("fixture package exists")
    }

    fn edge(&mut self, from: &str, to: &str) {
        let path = UNITS
            .iter()
            .find(|(name, _, _)| *name == to)
            .expect("target exists")
            .1;
        let dependency = json!({"name": to, "rename": "alias", "path": self.root.join(path)});
        self.package(from)
            .get_mut("dependencies")
            .and_then(Value::as_array_mut)
            .expect("dependencies")
            .push(dependency);
    }

    fn validate(&self) -> Result<(), String> {
        WorkspaceGraph::from_metadata(
            &self.root,
            &serde_json::to_vec(&self.metadata).expect("JSON"),
        )?
        .verify_layers()
    }

    fn read_graph(&self) -> Result<WorkspaceGraph, String> {
        let output = Command::new(crate::cargo_executable())
            .args(["generate-lockfile", "--offline"])
            .current_dir(&self.root)
            .output()
            .expect("generate fixture lockfile");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        WorkspaceGraph::read(&self.root, &crate::cargo_executable())
    }

    fn append(&self, path: &str, text: &str) {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(self.root.join(path))
            .expect("open manifest");
        file.write_all(text.as_bytes()).expect("append declaration");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn upward_dependency_is_rejected() {
    assert_eq!(
        verify_edge(0, 3),
        Err("product dependencies must strictly descend: L0 -> L3".to_owned())
    );
}

#[test]
fn renamed_upward_path_dependency_is_rejected() {
    let mut fixture = Fixture::new();
    fixture.edge("eez-domain", "eez-commands");
    assert_eq!(
        fixture.validate(),
        Err(
            "eez-domain -> eez-commands: product dependencies must strictly descend: L0 -> L3"
                .to_owned()
        )
    );
}

#[test]
fn equal_layer_dependency_is_rejected() {
    let mut fixture = Fixture::new();
    fixture.edge("eez-api", "eez-cli");
    assert_eq!(
        fixture.validate(),
        Err("eez-api -> eez-cli: product dependencies must strictly descend: L4 -> L4".to_owned())
    );
}

#[test]
fn strictly_descending_graph_and_tool_caller_are_valid() {
    let mut fixture = Fixture::new();
    fixture.edge("eez-api", "eez-commands");
    fixture.edge("eez-commands", "eez-storage");
    fixture.edge("eez-storage", "eez-render");
    fixture.edge("eez-render", "eez-domain");
    fixture.edge("xtask", "eez-api");
    assert_eq!(fixture.validate(), Ok(()));
}

#[test]
fn worker_can_depend_on_the_lower_layer_dashboard_presentation_library() {
    let mut fixture = Fixture::new();
    fixture.edge("eez-api", "eez-dashboard");
    assert_eq!(fixture.validate(), Ok(()));
}

#[test]
fn product_cannot_depend_on_tool() {
    let mut fixture = Fixture::new();
    fixture.edge("eez-api", "xtask");
    assert_eq!(
        fixture.validate(),
        Err("product cannot depend on tool: eez-api -> xtask".to_owned())
    );
}

#[test]
fn missing_classification_is_rejected() {
    let mut fixture = Fixture::new();
    *fixture
        .package("eez-domain")
        .get_mut("metadata")
        .expect("metadata") = json!({});
    assert_eq!(
        fixture.validate(),
        Err("missing classification for eez-domain".to_owned())
    );
}

#[test]
fn classification_cannot_rewrite_frozen_architecture() {
    for classification in [json!({"layer": 4}), json!({"role": "tool"})] {
        let mut fixture = Fixture::new();
        *fixture
            .package("eez-domain")
            .get_mut("metadata")
            .expect("metadata") = json!({"eez": classification});
        assert_eq!(
            fixture.validate(),
            Err("classification disagrees with frozen architecture: eez-domain".to_owned())
        );
    }
}

#[test]
fn invalid_classifications_are_rejected() {
    for classification in [
        json!({"layer": 5}),
        json!({"layer": -1}),
        json!({"layer": "0"}),
        json!({"layer": 0, "role": "tool"}),
        json!({"role": "unknown"}),
    ] {
        let mut fixture = Fixture::new();
        *fixture
            .package("eez-domain")
            .get_mut("metadata")
            .expect("metadata") = json!({"eez": classification});
        assert_eq!(
            fixture.validate(),
            Err("invalid classification for eez-domain".to_owned())
        );
    }
}

#[test]
fn unknown_and_missing_members_are_rejected() {
    let mut fixture = Fixture::new();
    *fixture.package("eez-domain").get_mut("name").expect("name") = json!("mystery");
    assert_eq!(
        fixture.validate(),
        Err("unclassified workspace member: mystery".to_owned())
    );
    let mut fixture = Fixture::new();
    fixture
        .metadata
        .get_mut("workspace_members")
        .and_then(Value::as_array_mut)
        .expect("members")
        .remove(0);
    assert_eq!(
        fixture.validate(),
        Err("required workspace member is missing: eez-domain".to_owned())
    );
}

#[test]
fn nonmember_local_dependency_is_rejected() {
    let mut fixture = Fixture::new();
    let path = fixture.root.join("hidden");
    fs::create_dir(&path).expect("create hidden directory");
    *fixture
        .package("eez-domain")
        .get_mut("dependencies")
        .expect("dependencies") = json!([{"name": "hidden", "path": path}]);
    assert_eq!(
        fixture.validate(),
        Err("unclassified path dependency: eez-domain -> hidden".to_owned())
    );
}

#[test]
fn malformed_metadata_fails_without_crashing() {
    let fixture = Fixture::new();
    for bytes in [b"{".as_slice(), b"\xff".as_slice()] {
        let error = WorkspaceGraph::from_metadata(&fixture.root, bytes)
            .err()
            .expect("invalid JSON fails");
        assert!(error.starts_with("invalid Cargo metadata JSON:"), "{error}");
    }
    let error = WorkspaceGraph::from_metadata(&fixture.root, b"{}")
        .err()
        .expect("missing fields fail");
    assert_eq!(error, "Cargo metadata missing workspace_root");
}

#[test]
fn live_metadata_covers_renamed_optional_target_dev_and_build_edges() {
    for declaration in [
        "\n[dependencies]\nalias = { package = \"eez-commands\", path = \"../commands\", optional = true }\n",
        "\n[target.'cfg(windows)'.dependencies]\nalias = { package = \"eez-commands\", path = \"../commands\", optional = true }\n",
        "\n[dev-dependencies]\nalias = { package = \"eez-commands\", path = \"../commands\" }\n",
        "\n[build-dependencies]\nalias = { package = \"eez-commands\", path = \"../commands\" }\n",
    ] {
        let fixture = Fixture::new();
        fixture.append("crates/domain/Cargo.toml", declaration);
        let graph = fixture.read_graph().expect("real Cargo metadata succeeds");
        assert_eq!(graph.verify_layers(), Err("eez-domain -> eez-commands: product dependencies must strictly descend: L0 -> L3".to_owned()));
    }
}

#[test]
fn live_metadata_accepts_descending_renamed_path_edge() {
    let fixture = Fixture::new();
    fixture.append(
        "crates/render/Cargo.toml",
        "\n[dependencies]\nalias = { package = \"eez-domain\", path = \"../domain\" }\n",
    );
    let graph = fixture.read_graph().expect("real Cargo metadata succeeds");
    assert_eq!(graph.verify_layers(), Ok(()));
}

#[test]
fn live_patch_redirect_cannot_hide_a_forbidden_edge() {
    let fixture = Fixture::new();
    fixture.append(
        "Cargo.toml",
        "\n[patch.crates-io]\neez-commands = { path = \"crates/commands\" }\n",
    );
    fixture.append(
        "crates/domain/Cargo.toml",
        "\n[dependencies]\nalias = { package = \"eez-commands\", version = \"0.1.0\" }\n",
    );
    let graph = fixture.read_graph().expect("real Cargo metadata succeeds");
    assert_eq!(graph.verify_layers(), Err("ambiguous workspace-named dependency requires explicit path: eez-domain -> eez-commands".to_owned()));
}

#[test]
fn external_path_dependency_cannot_hide_a_workspace_backdoor() {
    let mut fixture = Fixture::new();
    let external = Fixture::new();
    *fixture
        .package("eez-domain")
        .get_mut("dependencies")
        .expect("dependencies") = json!([{
        "name": "outside-helper", "path": external.root.join("crates/domain")
    }]);
    assert_eq!(
        fixture.validate(),
        Err("unclassified path dependency: eez-domain -> outside-helper".to_owned())
    );
}

#[test]
fn live_registry_patch_helper_cannot_hide_a_workspace_backdoor() {
    for declaration in [
        "\n[dependencies]\nhidden-helper = \"0.1.0\"\n",
        "\n[dependencies]\nhidden-helper = { version = \"0.1.0\", optional = true }\n",
        "\n[target.'cfg(windows)'.dependencies]\nalias = { package = \"hidden-helper\", version = \"0.1.0\", optional = true }\n",
    ] {
        let fixture = Fixture::new();
        fixture.append("Cargo.toml", "exclude = [\"helper\"]\n\n[patch.crates-io]\nhidden-helper = { path = \"helper\" }\n");
        fs::create_dir_all(fixture.root.join("helper/src")).expect("create helper");
        fs::write(fixture.root.join("helper/src/lib.rs"), "").expect("write helper source");
        fs::write(fixture.root.join("helper/Cargo.toml"), "[package]\nname = \"hidden-helper\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\neez-commands = { path = \"../crates/commands\" }\n").expect("write helper manifest");
        fixture.append("crates/domain/Cargo.toml", declaration);
        let result = fixture.read_graph().and_then(|graph| graph.verify_layers());
        assert_eq!(result, Err("unclassified resolved local package: hidden-helper".to_owned()));
    }
}
