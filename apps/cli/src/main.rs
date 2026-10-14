#![forbid(unsafe_code)]

mod deploy;
mod funnels;
mod state;
mod types;

use eez_domain::FunnelBlueprint;
use funnels::{CreateInput, Funnels};
use state::LocalState;
use std::ffi::OsString;
use std::io::{self, Write};
use types::{Endpoint, FunnelId, FunnelName, OperationId, Result};

const HELP: &str = "Eez Funnel CLI\nUsage:\n  eez config set endpoint <url>\n  eez funnels list\n  eez funnels create <name> [--blueprint tripwire|vsl|webinar|launch|lead_gen]\n  eez funnels publish <id>\n  eez funnels retry <operation-uuid>\n  eez deploy [wrangler-args...]\n\nCreate defaults to tripwire. Funnel results are JSON.\nEndpoint config and immutable retry tickets use the user config directory;\nset EEZ_CONFIG_DIR to override it. Retry tickets expire after 23 hours.\nDeploy requires Wrangler on PATH and runs in the current directory.\n";

fn run(arguments: Vec<OsString>) -> Result<i32> {
    if arguments.is_empty()
        || arguments.len() == 1
            && arguments
                .first()
                .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        write_output(HELP)?;
        return Ok(0);
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "deploy")
    {
        return deploy::deploy(arguments.get(1..).ok_or("invalid deploy arguments")?);
    }
    let arguments: Vec<&str> = arguments
        .iter()
        .map(|argument| {
            argument.to_str().ok_or_else(|| {
                "command arguments must be valid UTF-8 (deploy arguments are OS-native)".to_owned()
            })
        })
        .collect::<Result<_>>()?;
    let output =
        match arguments.as_slice() {
            ["config", "set", "endpoint", endpoint] => {
                let endpoint = Endpoint::try_from((*endpoint).to_owned())?;
                LocalState::discover()?.set_endpoint(endpoint)?;
                "Endpoint saved.".to_owned()
            }
            ["funnels", "list"] => Funnels::new(LocalState::discover()?).list()?,
            ["funnels", "create", name] => create(name, "tripwire")?,
            ["funnels", "create", name, "--blueprint", blueprint] => create(name, blueprint)?,
            ["funnels", "publish", id] => Funnels::new(LocalState::discover()?)
                .publish(FunnelId::try_from((*id).to_owned())?)?,
            ["funnels", "retry", id] => Funnels::new(LocalState::discover()?)
                .retry(OperationId::try_from((*id).to_owned())?)?,
            _ => return Err("unsupported command or arguments; use eez --help".into()),
        };
    write_output(&format!("{output}\n"))?;
    Ok(0)
}
fn create(name: &str, blueprint: &str) -> Result<String> {
    let name = FunnelName::try_from(name.to_owned())?;
    let blueprint = FunnelBlueprint::ALL
        .into_iter()
        .find(|candidate| candidate.id() == blueprint)
        .ok_or("blueprint must be tripwire, vsl, webinar, launch, or lead_gen")?;
    Funnels::new(LocalState::discover()?).create(CreateInput { name, blueprint })
}
fn write_output(output: &str) -> Result<()> {
    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|_| "cannot write CLI output".into())
}
fn main() {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            std::process::exit(1)
        }
    }
}
