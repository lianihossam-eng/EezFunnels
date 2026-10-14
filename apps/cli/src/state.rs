use crate::types::{Endpoint, OperationId, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STATE_LIMIT: u64 = 16 * 1024;
pub const RETRY_WINDOW: u64 = 23 * 60 * 60;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    version: u8,
    endpoint: Endpoint,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetryTicket {
    pub version: u8,
    pub operation_id: OperationId,
    pub created_at: u64,
    pub endpoint: Endpoint,
    pub path: String,
    pub body: String,
}

pub fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| "system clock precedes Unix epoch".into())
}

pub struct LocalState {
    root: PathBuf,
}
impl LocalState {
    pub fn discover() -> Result<Self> {
        let root = match std::env::var_os("EEZ_CONFIG_DIR") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            Some(_) => return Err("EEZ_CONFIG_DIR must not be empty".into()),
            None => directories::ProjectDirs::from("", "", "eez")
                .map(|dirs| dirs.config_dir().to_owned())
                .ok_or("cannot find user config directory; set EEZ_CONFIG_DIR")?,
        };
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()
                .map_err(|_| "cannot resolve CLI state directory")?
                .join(root)
        };
        Ok(Self { root })
    }
    pub fn set_endpoint(&self, endpoint: Endpoint) -> Result<()> {
        self.prepare(&self.root)?;
        let bytes = serde_json::to_vec(&Config {
            version: 1,
            endpoint,
        })
        .map_err(|_| "cannot encode config")?;
        if bytes.len() > STATE_LIMIT as usize {
            return Err("endpoint config exceeds local size limit".into());
        }
        self.persist(&self.root, &self.root.join("config.json"), &bytes, false)
    }
    pub fn endpoint(&self) -> Result<Endpoint> {
        let bytes = read_bounded(&self.root.join("config.json")).map_err(|_| {
            "endpoint config is missing or unreadable; run eez config set endpoint <url>"
        })?;
        let config: Config = serde_json::from_slice(&bytes)
            .map_err(|_| "endpoint config is malformed; run eez config set endpoint <url>")?;
        if config.version != 1 {
            return Err("unsupported config version".into());
        }
        Ok(config.endpoint)
    }
    pub fn save(&self, ticket: &RetryTicket) -> Result<()> {
        let directory = self.root.join("operations");
        self.prepare(&directory)?;
        self.prune_expired(&directory);
        let bytes = serde_json::to_vec(ticket).map_err(|_| "cannot encode retry ticket")?;
        if bytes.len() > STATE_LIMIT as usize {
            return Err("retry ticket exceeds local size limit".into());
        }
        self.persist(
            &directory,
            &directory.join(format!("{}.json", ticket.operation_id.as_str())),
            &bytes,
            true,
        )
    }
    pub fn load(&self, operation_id: &OperationId) -> Result<RetryTicket> {
        let path = self
            .root
            .join("operations")
            .join(format!("{}.json", operation_id.as_str()));
        let bytes =
            read_bounded(&path).map_err(|_| "retry ticket is missing, oversized, or unreadable")?;
        let ticket: RetryTicket =
            serde_json::from_slice(&bytes).map_err(|_| "retry ticket is malformed")?;
        if ticket.version != 1 || ticket.operation_id.as_str() != operation_id.as_str() {
            return Err("retry ticket version or operation ID is invalid".into());
        }
        let age = now()?
            .checked_sub(ticket.created_at)
            .ok_or("retry ticket is in the future; check the system clock")?;
        if age >= RETRY_WINDOW {
            let _ = fs::remove_file(&path);
            if let Some(directory) = path.parent() {
                let _ = sync_directory(directory);
            }
            return Err("retry ticket is at least 23 hours old; inspect the server state before starting a new operation".into());
        }
        Ok(ticket)
    }
    fn prune_expired(&self, directory: &Path) {
        let Ok(current_time) = now() else {
            return;
        };
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        let mut removed = false;
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let Ok(bytes) = read_bounded(&entry.path()) else {
                continue;
            };
            let Ok(ticket) = serde_json::from_slice::<RetryTicket>(&bytes) else {
                continue;
            };
            let expected_name = format!("{}.json", ticket.operation_id.as_str());
            if ticket.version != 1 || entry.file_name().to_str() != Some(expected_name.as_str()) {
                continue;
            }
            if current_time
                .checked_sub(ticket.created_at)
                .is_some_and(|age| age >= RETRY_WINDOW)
                && fs::remove_file(entry.path()).is_ok()
            {
                removed = true;
            }
        }
        if removed {
            let _ = sync_directory(directory);
        }
    }
    fn prepare(&self, directory: &Path) -> Result<()> {
        let mut missing = Vec::new();
        for ancestor in directory.ancestors() {
            if ancestor.is_dir() {
                break;
            }
            missing.push(ancestor);
        }
        for path in missing.into_iter().rev() {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
                Err(_) => return Err("cannot create CLI state directory".into()),
            }
            sync_directory(path.parent().ok_or("invalid CLI state directory")?)?;
        }
        Ok(())
    }
    fn persist(
        &self,
        directory: &Path,
        target: &Path,
        bytes: &[u8],
        immutable: bool,
    ) -> Result<()> {
        let mut temporary = tempfile::NamedTempFile::new_in(directory)
            .map_err(|_| "cannot create temporary CLI state")?;
        temporary
            .write_all(bytes)
            .map_err(|_| "cannot write CLI state")?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| "cannot sync CLI state")?;
        if immutable {
            temporary
                .persist_noclobber(target)
                .map_err(|_| "cannot save immutable operation ticket")?;
        } else {
            temporary
                .persist(target)
                .map_err(|_| "cannot replace endpoint config")?;
        }
        sync_directory(directory)
    }
}
fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "cannot open state")?
        .take(STATE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read state")?;
    if bytes.len() > STATE_LIMIT as usize {
        return Err("state is oversized".into());
    }
    Ok(bytes)
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "cannot sync CLI state directory")?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tickets_are_immutable_and_config_replacement_is_valid() {
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState {
            root: directory.path().join("nested/config"),
        };
        state
            .set_endpoint(Endpoint::try_from("https://one.example".to_owned()).unwrap())
            .unwrap();
        state
            .set_endpoint(Endpoint::try_from("https://two.example".to_owned()).unwrap())
            .unwrap();
        assert_eq!(
            state.endpoint().unwrap().route("/api/funnels"),
            "https://two.example/api/funnels"
        );
        let mut ticket = RetryTicket {
            version: 1,
            operation_id: OperationId::fresh(),
            created_at: now().unwrap(),
            endpoint: state.endpoint().unwrap(),
            path: "/api/funnels".into(),
            body: "original".into(),
        };
        state.save(&ticket).unwrap();
        ticket.body = "replacement".into();
        assert!(state.save(&ticket).is_err());
        assert_eq!(state.load(&ticket.operation_id).unwrap().body, "original");
    }

    #[test]
    fn endpoint_config_boundary_always_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState {
            root: directory.path().join("nested/config"),
        };
        let endpoint = format!("https://{}{}", "a.".repeat(1019), "aa");
        assert_eq!(endpoint.len(), 2048);
        let parsed = Endpoint::try_from(endpoint.clone()).unwrap();
        state.set_endpoint(parsed.clone()).unwrap();
        assert_eq!(state.endpoint().unwrap().route(""), parsed.route(""));
        let config = fs::read(state.root.join("config.json")).unwrap();
        assert!(config.len() <= STATE_LIMIT as usize);

        assert!(Endpoint::try_from(format!("{endpoint}a")).is_err());
    }

    #[test]
    fn saving_a_new_ticket_prunes_only_expired_tickets() {
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState {
            root: directory.path().join("config"),
        };
        let endpoint = Endpoint::try_from("https://example.com".to_owned()).unwrap();
        let operations = state.root.join("operations");
        fs::create_dir_all(&operations).unwrap();
        let timestamp = now().unwrap();
        let expired = RetryTicket {
            version: 1,
            operation_id: OperationId::fresh(),
            created_at: timestamp - RETRY_WINDOW,
            endpoint: endpoint.clone(),
            path: "/api/funnels".into(),
            body: "{}".into(),
        };
        let live = RetryTicket {
            version: 1,
            operation_id: OperationId::fresh(),
            created_at: timestamp,
            endpoint: endpoint.clone(),
            path: "/api/funnels".into(),
            body: "{}".into(),
        };
        let expired_path = operations.join(format!("{}.json", expired.operation_id.as_str()));
        let live_path = operations.join(format!("{}.json", live.operation_id.as_str()));
        fs::write(&expired_path, serde_json::to_vec(&expired).unwrap()).unwrap();
        fs::write(&live_path, serde_json::to_vec(&live).unwrap()).unwrap();
        let new_ticket = RetryTicket {
            version: 1,
            operation_id: OperationId::fresh(),
            created_at: timestamp,
            endpoint,
            path: "/api/funnels".into(),
            body: "{}".into(),
        };
        state.save(&new_ticket).unwrap();
        assert!(!expired_path.exists());
        assert!(live_path.exists());
    }
}
