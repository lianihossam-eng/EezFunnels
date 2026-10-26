use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Product(u8),
    Tool,
}

const UNITS: [(&str, &str, Role); 9] = [
    ("eez-domain", "crates/domain", Role::Product(0)),
    ("eez-render", "crates/render", Role::Product(1)),
    (
        "eez-dashboard-presentation",
        "crates/dashboard-presentation",
        Role::Product(1),
    ),
    ("eez-storage", "crates/storage", Role::Product(2)),
    ("eez-commands", "crates/commands", Role::Product(3)),
    ("eez-api", "apps/api", Role::Product(4)),
    ("eez-dashboard", "apps/dashboard", Role::Product(4)),
    ("eez-cli", "apps/cli", Role::Product(4)),
    ("xtask", "xtask", Role::Tool),
];

const ALLOWED_DEPENDENCIES: [(&str, &str); 13] = [
    ("eez-render", "eez-domain"),
    ("eez-storage", "eez-domain"),
    ("eez-commands", "eez-domain"),
    ("eez-commands", "eez-render"),
    ("eez-commands", "eez-storage"),
    ("eez-dashboard-presentation", "eez-domain"),
    ("eez-api", "eez-domain"),
    ("eez-api", "eez-render"),
    ("eez-api", "eez-storage"),
    ("eez-api", "eez-commands"),
    ("eez-api", "eez-dashboard-presentation"),
    ("eez-dashboard", "eez-dashboard-presentation"),
    ("eez-cli", "eez-domain"),
];

struct Dependency {
    name: String,
    path: Option<PathBuf>,
}

struct Package {
    name: String,
    role: Role,
    directory: PathBuf,
    dependencies: Vec<Dependency>,
}

pub(super) struct WorkspaceGraph {
    packages: Vec<Package>,
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, String> {
    value
        .get(name)
        .ok_or_else(|| format!("Cargo metadata missing {name}"))
}

fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    field(value, name)?
        .as_str()
        .ok_or_else(|| format!("Cargo metadata {name} must be a string"))
}

fn array<'a>(value: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    field(value, name)?
        .as_array()
        .ok_or_else(|| format!("Cargo metadata {name} must be an array"))
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|error| format!("cannot identify {}: {error}", path.display()))
}

fn role(value: &Value, name: &str) -> Result<Role, String> {
    let classification = value
        .get("metadata")
        .and_then(|metadata| metadata.get("eez"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("missing classification for {name}"))?;
    if classification.len() == 1 {
        if classification.get("role").and_then(Value::as_str) == Some("tool") {
            return Ok(Role::Tool);
        }
        if let Some(layer) = classification.get("layer").and_then(Value::as_u64) {
            if let Ok(layer @ 0..=4) = u8::try_from(layer) {
                return Ok(Role::Product(layer));
            }
        }
    }
    Err(format!("invalid classification for {name}"))
}

fn verify_edge(from: u8, to: u8) -> Result<(), String> {
    if to >= from {
        return Err(format!(
            "product dependencies must strictly descend: L{from} -> L{to}"
        ));
    }
    Ok(())
}

impl WorkspaceGraph {
    pub(super) fn read(root: &Path, cargo: &OsStr) -> Result<Self, String> {
        let output = Command::new(cargo)
            .args([
                "metadata",
                "--all-features",
                "--format-version",
                "1",
                "--locked",
            ])
            .current_dir(root)
            .output()
            .map_err(|error| format!("cannot execute Cargo: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Self::from_metadata(root, &output.stdout)
    }

    fn from_metadata(root: &Path, bytes: &[u8]) -> Result<Self, String> {
        let metadata: Value = serde_json::from_slice(bytes)
            .map_err(|error| format!("invalid Cargo metadata JSON: {error}"))?;
        let root = canonical(root)?;
        if canonical(Path::new(string(&metadata, "workspace_root")?))? != root {
            return Err("Cargo metadata workspace root does not match requested root".to_owned());
        }
        let members = array(&metadata, "workspace_members")?;
        let mut packages = Vec::new();
        for member in members {
            let id = member
                .as_str()
                .ok_or_else(|| "workspace member ID must be a string".to_owned())?;
            let mut matches = array(&metadata, "packages")?
                .iter()
                .filter(|package| package.get("id").and_then(Value::as_str) == Some(id));
            let package = matches
                .next()
                .ok_or_else(|| format!("workspace member missing package: {id}"))?;
            if matches.next().is_some() {
                return Err(format!("duplicate Cargo package ID: {id}"));
            }
            let name = string(package, "name")?;
            let (_, expected_path, expected_role) = UNITS
                .iter()
                .find(|(known, _, _)| *known == name)
                .ok_or_else(|| format!("unclassified workspace member: {name}"))?;
            let role = role(package, name)?;
            if role != *expected_role {
                return Err(format!(
                    "classification disagrees with frozen architecture: {name}"
                ));
            }
            let manifest = Path::new(string(package, "manifest_path")?);
            let directory = canonical(
                manifest
                    .parent()
                    .ok_or_else(|| format!("manifest has no parent: {name}"))?,
            )?;
            if directory != canonical(&root.join(expected_path))? {
                return Err(format!(
                    "workspace package location disagrees with architecture: {name}"
                ));
            }
            if packages.iter().any(|known: &Package| known.name == name) {
                return Err(format!("duplicate workspace member: {name}"));
            }
            let dependencies = array(package, "dependencies")?
                .iter()
                .map(|dependency| {
                    let path = match dependency.get("path") {
                        Some(Value::String(path)) => Some(canonical(Path::new(path))?),
                        None | Some(Value::Null) => None,
                        _ => return Err("dependency path must be a string".to_owned()),
                    };
                    Ok(Dependency {
                        name: string(dependency, "name")?.to_owned(),
                        path,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            packages.push(Package {
                name: name.to_owned(),
                role,
                directory,
                dependencies,
            });
        }
        for (name, _, _) in UNITS {
            if !packages.iter().any(|package| package.name == name) {
                return Err(format!("required workspace member is missing: {name}"));
            }
        }
        for package in array(&metadata, "packages")? {
            let id = string(package, "id")?;
            match field(package, "source")? {
                Value::Null => {
                    if !members.iter().any(|member| member.as_str() == Some(id)) {
                        return Err(format!(
                            "unclassified resolved local package: {}",
                            string(package, "name")?
                        ));
                    }
                }
                Value::String(_) => {}
                _ => return Err("Cargo metadata source must be a string or null".to_owned()),
            }
        }
        Ok(Self { packages })
    }

    pub(super) fn verify_layers(&self) -> Result<(), String> {
        for package in &self.packages {
            for dependency in &package.dependencies {
                let target = match &dependency.path {
                    Some(path) => self
                        .packages
                        .iter()
                        .find(|target| target.directory == *path),
                    None => {
                        if self
                            .packages
                            .iter()
                            .any(|target| target.name == dependency.name)
                        {
                            return Err(format!(
                                "ambiguous workspace-named dependency requires explicit path: {} -> {}",
                                package.name, dependency.name
                            ));
                        }
                        None
                    }
                };
                let Some(target) = target else {
                    if dependency.path.is_some() {
                        return Err(format!(
                            "unclassified path dependency: {} -> {}",
                            package.name, dependency.name
                        ));
                    }
                    continue;
                };
                match (package.role, target.role) {
                    (Role::Product(from), Role::Product(to)) => verify_edge(from, to)
                        .map_err(|error| format!("{} -> {}: {error}", package.name, target.name))?,
                    (Role::Product(_), Role::Tool) => {
                        return Err(format!(
                            "product cannot depend on tool: {} -> {}",
                            package.name, target.name
                        ));
                    }
                    (Role::Tool, _) => {}
                }
                if !ALLOWED_DEPENDENCIES.contains(&(package.name.as_str(), target.name.as_str())) {
                    return Err(format!(
                        "internal dependency is not explicitly allowed: {} -> {}",
                        package.name, target.name
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
