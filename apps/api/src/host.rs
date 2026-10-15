use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::str::FromStr;

use axum::http::{uri::Authority, HeaderMap, HeaderValue};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostAllowlist {
    hosts: HashSet<String>,
}

impl HostAllowlist {
    pub fn parse(hosts: &str) -> Self {
        let hosts = hosts
            .split(',')
            .filter_map(|host| normalize_host(host.trim()))
            .collect::<HashSet<_>>();
        Self { hosts }
    }

    pub fn allows(&self, host: &str) -> bool {
        normalize_host(host).is_some_and(|host| self.hosts.contains(&host))
    }

    pub fn allows_headers(&self, headers: &HeaderMap<HeaderValue>) -> bool {
        let mut values = headers.get_all("host").iter();
        let Some(host) = values.next() else {
            return false;
        };
        values.next().is_none() && host.to_str().ok().is_some_and(|host| self.allows(host))
    }
}

fn normalize_host(host: &str) -> Option<String> {
    if host.is_empty()
        || host
            .chars()
            .any(|character| character.is_ascii_whitespace() || character.is_control())
        || host
            .chars()
            .any(|character| matches!(character, '*' | '@' | '/' | '\\' | '?' | '#'))
    {
        return None;
    }

    let authority = Authority::from_str(host).ok()?;
    let host_part = authority.host();
    let port = if host_part.starts_with('[') {
        let closing_bracket = authority.as_str().find(']')?;
        let suffix = &authority.as_str()[closing_bracket + 1..];
        if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':')?)
        }
    } else {
        authority.as_str().split_once(':').map(|(_, port)| port)
    };
    if port.is_some_and(|port| {
        port.is_empty()
            || !port.bytes().all(|byte| byte.is_ascii_digit())
            || port.parse::<u16>().is_err()
    }) {
        return None;
    }

    if let Some(ipv6) = host_part
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
    {
        Ipv6Addr::from_str(ipv6).ok()?;
    } else if host_part.contains(':')
        || host_part.split('.').any(|label| {
            label.is_empty()
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                || !label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                || !label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
    {
        return None;
    }

    Some(host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::HostAllowlist;

    #[test]
    fn allowlist_accepts_only_exact_hosts_and_ports() {
        let hosts = HostAllowlist::parse("eezfunnel.workers.dev, localhost:8787");

        assert!(hosts.allows("eezfunnel.workers.dev"));
        assert!(hosts.allows("localhost:8787"));
        assert!(!hosts.allows("eezfunnel.workers.dev.attacker.example"));
        assert!(!hosts.allows("localhost:8788"));
    }

    #[test]
    fn allowlist_compares_hostnames_without_case_sensitivity() {
        let hosts = HostAllowlist::parse("eezfunnel.workers.dev");

        assert!(hosts.allows("EEZFunnel.Workers.Dev"));
    }

    #[test]
    fn empty_or_wildcard_entries_do_not_allow_hosts() {
        let hosts = HostAllowlist::parse(" , *.workers.dev, ");

        assert!(!hosts.allows("eezfunnel.workers.dev"));
        assert!(!hosts.allows(""));
    }

    #[test]
    fn duplicate_host_headers_are_rejected() {
        let hosts = HostAllowlist::parse("localhost:8787");
        let mut headers = HeaderMap::new();
        headers.append("host", HeaderValue::from_static("localhost:8787"));
        headers.append("host", HeaderValue::from_static("attacker.example"));

        assert!(!hosts.allows_headers(&headers));
    }

    #[test]
    fn malformed_authorities_are_never_allowlisted() {
        let hosts = HostAllowlist::parse("localhost:bad, example.com/path");

        assert!(!hosts.allows("localhost:bad"));
        assert!(!hosts.allows("example.com/path"));
    }
}
