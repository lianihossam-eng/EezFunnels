use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostAllowlist {
    hosts: HashSet<String>,
}

impl HostAllowlist {
    pub fn parse(hosts: &str) -> Self {
        let hosts = hosts
            .split(',')
            .filter_map(normalize_host)
            .collect::<HashSet<_>>();
        Self { hosts }
    }

    pub fn allows(&self, host: &str) -> bool {
        normalize_host(host).is_some_and(|host| self.hosts.contains(&host))
    }
}

fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty()
        || host
            .chars()
            .any(|character| character.is_ascii_whitespace() || character.is_control())
    {
        return None;
    }
    let host = host.to_ascii_lowercase();
    if host.contains('*') {
        return None;
    }
    Some(host)
}

#[cfg(test)]
mod tests {
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
}
