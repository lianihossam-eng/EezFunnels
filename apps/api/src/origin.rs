use axum::http::header::ORIGIN;
use axum::http::{HeaderMap, HeaderValue};

use crate::HostAllowlist;

pub(crate) fn matches_request_host(
    headers: &HeaderMap<HeaderValue>,
    request_scheme: Option<&str>,
) -> bool {
    let mut values = headers.get_all(ORIGIN).iter();
    let Some(origin) = values.next() else {
        return true;
    };
    if values.next().is_some() {
        return false;
    }
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    if authority.is_empty()
        || authority.contains(',')
        || authority.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(character, '/' | '?' | '#')
        })
    {
        return false;
    }
    if request_scheme.is_some_and(|request_scheme| !request_scheme.eq_ignore_ascii_case(scheme)) {
        return false;
    }
    HostAllowlist::parse(authority).allows_headers(headers)
}

#[cfg(test)]
mod tests {
    use super::matches_request_host;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn supplied_origins_must_be_single_http_origins_for_the_allowed_host() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("eezfunnel.workers.dev"));
        assert!(matches_request_host(&headers, Some("https")));
        headers.insert(
            "origin",
            HeaderValue::from_static("https://eezfunnel.workers.dev"),
        );
        assert!(matches_request_host(&headers, Some("https")));
        headers.insert(
            "origin",
            HeaderValue::from_static("https://attacker.example"),
        );
        assert!(!matches_request_host(&headers, Some("https")));
        headers.insert(
            "origin",
            HeaderValue::from_static("https://attacker.example,eezfunnel.workers.dev"),
        );
        assert!(!matches_request_host(&headers, Some("https")));
        headers.insert(
            "origin",
            HeaderValue::from_static("https:// eezfunnel.workers.dev"),
        );
        assert!(!matches_request_host(&headers, Some("https")));
        headers.insert(
            "origin",
            HeaderValue::from_static("https://eezfunnel.workers.dev/path"),
        );
        assert!(!matches_request_host(&headers, Some("https")));
        headers.insert("origin", HeaderValue::from_static("null"));
        assert!(!matches_request_host(&headers, Some("https")));
        headers.insert(
            "origin",
            HeaderValue::from_static("http://eezfunnel.workers.dev"),
        );
        assert!(!matches_request_host(&headers, Some("https")));
        headers.append(
            "origin",
            HeaderValue::from_static("https://eezfunnel.workers.dev"),
        );
        assert!(!matches_request_host(&headers, Some("https")));
    }
}
