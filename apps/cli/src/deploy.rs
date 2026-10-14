use crate::types::Result;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Eq, PartialEq)]
struct Launcher {
    executable: PathBuf,
    prefix: Vec<OsString>,
}

#[cfg(any(windows, test))]
fn windows_launcher(paths: impl IntoIterator<Item = PathBuf>) -> Result<Launcher> {
    let paths: Vec<_> = paths.into_iter().collect();
    for directory in &paths {
        let native = directory.join("wrangler.exe");
        if native.is_file() {
            return Ok(Launcher {
                executable: native,
                prefix: vec![],
            });
        }
        if directory.join("wrangler.cmd").is_file() {
            let global = directory.join("node_modules/wrangler/bin/wrangler.js");
            let local = directory
                .parent()
                .map(|parent| parent.join("wrangler/bin/wrangler.js"));
            let script = std::iter::once(global)
                .chain(local)
                .find(|script| script.is_file())
                .ok_or(
                    "Wrangler npm shim has no known JavaScript entrypoint; reinstall Wrangler",
                )?;
            let node = std::iter::once(directory.join("node.exe"))
                .chain(paths.iter().map(|directory| directory.join("node.exe")))
                .find(|node| node.is_file())
                .ok_or("Wrangler requires node.exe on PATH")?;
            return Ok(Launcher {
                executable: node,
                prefix: vec![script.into_os_string()],
            });
        }
    }
    Err("Wrangler is missing; install Wrangler and add it to PATH".into())
}

#[cfg(not(windows))]
fn launcher() -> Result<Launcher> {
    Ok(Launcher {
        executable: PathBuf::from("wrangler"),
        prefix: vec![],
    })
}
#[cfg(windows)]
fn launcher() -> Result<Launcher> {
    let paths = std::env::var_os("PATH")
        .ok_or("Wrangler is missing; install Wrangler and add it to PATH")?;
    windows_launcher(std::env::split_paths(&paths))
}

pub fn deploy(arguments: &[OsString]) -> Result<i32> {
    let launcher = launcher()?;
    let status = launch(&launcher, arguments)?;
    Ok(status.code().unwrap_or(1))
}
fn launch(launcher: &Launcher, arguments: &[OsString]) -> Result<std::process::ExitStatus> {
    Command::new(&launcher.executable)
        .args(&launcher.prefix)
        .arg(OsStr::new("deploy"))
        .args(arguments)
        .status()
        .map_err(|_| "cannot execute Wrangler; install Wrangler and add it to PATH".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_global_and_local_npm_shims_launch_node_directly() {
        for local in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let bin = if local {
                dir.path().join("node_modules/.bin")
            } else {
                dir.path().to_owned()
            };
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(
                bin.join("wrangler.cmd"),
                "untrusted shell text is never executed",
            )
            .unwrap();
            let script = dir.path().join("node_modules/wrangler/bin/wrangler.js");
            std::fs::create_dir_all(script.parent().unwrap()).unwrap();
            std::fs::write(&script, "").unwrap();
            let node_dir = dir.path().join("node location");
            std::fs::create_dir_all(&node_dir).unwrap();
            std::fs::write(node_dir.join("node.exe"), "").unwrap();
            let actual = windows_launcher(vec![bin, node_dir.clone()]).unwrap();
            assert_eq!(
                actual,
                Launcher {
                    executable: node_dir.join("node.exe"),
                    prefix: vec![script.into_os_string()]
                }
            );
        }
    }
    #[test]
    fn windows_native_and_incomplete_shims_are_classified() {
        let dir = tempfile::tempdir().unwrap();
        assert!(windows_launcher(vec![dir.path().to_owned()]).is_err());
        std::fs::write(dir.path().join("wrangler.cmd"), "").unwrap();
        assert!(windows_launcher(vec![dir.path().to_owned()]).is_err());
        std::fs::write(dir.path().join("wrangler.exe"), "").unwrap();
        assert_eq!(
            windows_launcher(vec![dir.path().to_owned()])
                .unwrap()
                .prefix,
            Vec::<OsString>::new()
        );
    }
}
