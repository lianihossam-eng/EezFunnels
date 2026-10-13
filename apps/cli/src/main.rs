#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

const HELP: &str = "Eez Funnel CLI (PR-000 workspace bootstrap)\nUsage: eez [--help]\nFunnel commands are not implemented.\n";

fn response(arguments: impl IntoIterator<Item = OsString>) -> Result<&'static str, String> {
    let mut arguments = arguments.into_iter();
    let first = arguments.next();
    if arguments.next().is_some() {
        return Err(
            "Funnel commands are not implemented in the PR-000 bootstrap. Use eez --help."
                .to_owned(),
        );
    }
    match first.as_deref() {
        None => Ok(HELP),
        Some(argument) if argument == "--help" || argument == "-h" => Ok(HELP),
        _ => Err(
            "Funnel commands are not implemented in the PR-000 bootstrap. Use eez --help."
                .to_owned(),
        ),
    }
}

fn main() -> ExitCode {
    match response(std::env::args_os().skip(1)) {
        Ok(help) => {
            if io::stdout().lock().write_all(help.as_bytes()).is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            ExitCode::FAILURE
        }
    }
}
