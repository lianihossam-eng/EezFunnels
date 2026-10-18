use std::collections::{BTreeMap, BTreeSet};

use axum::http::{HeaderMap, HeaderValue};

use crate::host::{normalize_dns_hostname, HostAllowlist};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CustomHostMappings {
    by_host: BTreeMap<String, String>,
    by_funnel_slug: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CustomHostMapping {
    pub(crate) hostname: String,
    pub(crate) funnel_slug: String,
}

impl CustomHostMappings {
    pub(crate) fn parse(
        raw: &str,
        platform_hosts: &HostAllowlist,
        public_base_hostname: Option<&str>,
    ) -> Result<Self, ()> {
        let mut mappings = Self::default();
        let mut funnel_slugs = BTreeSet::new();
        let Some(raw) = (!raw.trim().is_empty()).then_some(raw) else {
            return Ok(mappings);
        };

        for entry in raw.split(',') {
            let entry = entry.trim();
            if entry.is_empty() || entry.matches('=').count() != 1 {
                return Err(());
            }
            let (hostname, funnel_slug) = entry.split_once('=').ok_or(())?;
            let hostname = normalize_dns_hostname(hostname.trim()).ok_or(())?;
            let funnel_slug = funnel_slug.trim();
            if !crate::valid_public_slug(funnel_slug)
                || platform_hosts.allows(&hostname)
                || public_base_hostname
                    .and_then(normalize_dns_hostname)
                    .is_some_and(|base| base == hostname)
                || mappings
                    .by_host
                    .insert(hostname.clone(), funnel_slug.to_owned())
                    .is_some()
                || !funnel_slugs.insert(funnel_slug.to_owned())
            {
                return Err(());
            }
            mappings
                .by_funnel_slug
                .insert(funnel_slug.to_owned(), hostname);
        }

        Ok(mappings)
    }

    pub(crate) fn for_headers(
        &self,
        headers: &HeaderMap<HeaderValue>,
    ) -> Option<CustomHostMapping> {
        let mut values = headers.get_all("host").iter();
        let host = values.next()?.to_str().ok()?;
        if values.next().is_some() {
            return None;
        }
        let hostname = normalize_dns_hostname(host)?;
        let funnel_slug = self.by_host.get(&hostname)?;
        Some(CustomHostMapping {
            hostname,
            funnel_slug: funnel_slug.clone(),
        })
    }

    pub(crate) fn mapping_for_funnel(&self, funnel_slug: &str) -> Option<CustomHostMapping> {
        let hostname = self.by_funnel_slug.get(funnel_slug)?;
        Some(CustomHostMapping {
            hostname: hostname.clone(),
            funnel_slug: funnel_slug.to_owned(),
        })
    }

    pub(crate) fn has_funnel(&self, funnel_slug: &str) -> bool {
        self.by_funnel_slug.contains_key(funnel_slug)
    }

    pub(crate) fn origin(mapping: &CustomHostMapping) -> String {
        format!("https://{}", mapping.hostname)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use crate::host::HostAllowlist;

    use super::CustomHostMappings;

    fn mappings(raw: &str) -> Result<CustomHostMappings, ()> {
        CustomHostMappings::parse(
            raw,
            &HostAllowlist::parse("eezfunnel.workers.dev, localhost:8787"),
            Some("funnels.example"),
        )
    }

    #[test]
    fn mapping_hosts_are_normalized_and_resolve_only_one_host_header() {
        let mappings = mappings("  Funnel.Example = lead-gen ").expect("valid mapping");
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("funnel.example"));

        let mapping = mappings.for_headers(&headers).expect("mapping resolves");
        assert_eq!(mapping.hostname, "funnel.example");
        assert_eq!(mapping.funnel_slug, "lead-gen");
        assert_eq!(
            CustomHostMappings::origin(&mapping),
            "https://funnel.example"
        );
    }

    #[test]
    fn invalid_duplicate_and_conflicting_mappings_fail_as_a_whole() {
        for invalid in [
            "*.funnels.example=lead-gen",
            "funnels.example:443=lead-gen",
            "funnels.example=Lead-Gen",
            "funnels.example=lead-gen,funnels.example=other",
            "funnels.example=lead-gen,other.example=lead-gen",
            "eezfunnel.workers.dev=lead-gen",
            "funnels.example=lead-gen,",
            "funnels.example=lead-gen=other",
        ] {
            assert!(mappings(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn base_origin_host_cannot_be_reassigned_as_a_funnel_host() {
        assert!(mappings("FUNNELS.EXAMPLE=lead-gen").is_err());
    }

    #[test]
    fn empty_configuration_has_no_custom_hosts() {
        let mappings = mappings("  ").expect("empty configuration is valid");
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("funnels.example"));
        assert!(mappings.for_headers(&headers).is_none());
        assert!(!mappings.has_funnel("lead-gen"));
    }

    #[test]
    fn duplicate_host_headers_do_not_resolve_custom_mapping() {
        let mappings = mappings("funnel.example=lead-gen").expect("valid mapping");
        let mut headers = HeaderMap::new();
        headers.append("host", HeaderValue::from_static("funnel.example"));
        headers.append("host", HeaderValue::from_static("attacker.example"));
        assert!(mappings.for_headers(&headers).is_none());
    }
}
