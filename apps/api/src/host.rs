use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::str::FromStr;

use axum::http::{uri::Authority, HeaderMap, HeaderValue};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostAllowlist {
    hosts: HashSet<String>,
    blocked_hostnames: HashSet<String>,
}

impl HostAllowlist {
    pub fn parse(hosts: &str) -> Self {
        let hosts = hosts
            .split(',')
            .filter_map(|host| normalize_host(host.trim()))
            .collect::<HashSet<_>>();
        Self {
            hosts,
            blocked_hostnames: HashSet::new(),
        }
    }

    pub fn allows(&self, host: &str) -> bool {
        normalize_host(host).is_some_and(|host| {
            Authority::from_str(&host).is_ok_and(|authority| {
                !self
                    .blocked_hostnames
                    .contains(&authority.host().to_ascii_lowercase())
                    && self.hosts.contains(&host)
            })
        })
    }

    pub(crate) fn allows_hostname(&self, hostname: &str) -> bool {
        let Some(hostname) = normalize_dns_hostname(hostname) else {
            return false;
        };
        self.hosts.iter().any(|host| {
            Authority::from_str(host)
                .is_ok_and(|authority| authority.host().eq_ignore_ascii_case(&hostname))
        })
    }

    pub(crate) fn block_hostname(&mut self, hostname: &str) {
        if let Some(hostname) = normalize_dns_hostname(hostname) {
            self.blocked_hostnames.insert(hostname);
        }
    }

    pub fn allows_headers(&self, headers: &HeaderMap<HeaderValue>) -> bool {
        let mut values = headers.get_all("host").iter();
        let Some(host) = values.next() else {
            return false;
        };
        values.next().is_none() && host.to_str().ok().is_some_and(|host| self.allows(host))
    }
}

pub(crate) fn normalize_host(host: &str) -> Option<String> {
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

pub(crate) fn normalize_dns_hostname(host: &str) -> Option<String> {
    if host.contains(':') {
        return None;
    }
    let normalized = normalize_host(host)?;
    let labels = normalized.split('.').collect::<Vec<_>>();
    if normalized.len() > 253
        || labels.len() < 2
        || labels.iter().any(|label| label.len() > 63)
        || normalized.parse::<std::net::Ipv4Addr>().is_ok()
    {
        return None;
    }
    Some(normalized)
}

pub(crate) fn dns_hostname_from_authority(host: &str) -> Option<String> {
    let normalized = normalize_host(host)?;
    let authority = Authority::from_str(&normalized).ok()?;
    normalize_dns_hostname(authority.host())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{normalize_dns_hostname, HostAllowlist};

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
    fn hostname_conflicts_match_port_qualified_platform_hosts() {
        let hosts = HostAllowlist::parse("offers.example:443, other.example:8443");

        assert!(hosts.allows_hostname("OFFERS.EXAMPLE"));
        assert!(hosts.allows_hostname("other.example"));
        assert!(!hosts.allows_hostname("third.example"));
    }

    #[test]
    fn blocked_hostname_rejects_every_port_qualified_platform_request() {
        let mut hosts = HostAllowlist::parse("offers.example:443");
        hosts.block_hostname("offers.example");
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("offers.example:443"));

        assert!(!hosts.allows("offers.example:443"));
        assert!(!hosts.allows_headers(&headers));
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

    #[test]
    fn dns_hostnames_are_normalized_without_accepting_ports_or_ip_literals() {
        assert_eq!(
            normalize_dns_hostname("Funnels.Example"),
            Some("funnels.example".to_owned())
        );
        assert_eq!(normalize_dns_hostname("funnels.example:443"), None);
        assert_eq!(normalize_dns_hostname("[::1]"), None);
        assert_eq!(normalize_dns_hostname("localhost"), None);
        assert_eq!(normalize_dns_hostname("127.0.0.1"), None);
        assert_eq!(normalize_dns_hostname("*.funnels.example"), None);
    }
}
