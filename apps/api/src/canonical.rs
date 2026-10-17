use url::Url;

pub(crate) fn public_page_url(
    configured_origin: Option<&str>,
    funnel_slug: &str,
    page_slug: Option<&str>,
) -> Option<String> {
    if !valid_slug(funnel_slug) || page_slug.is_some_and(|slug| !valid_slug(slug)) {
        return None;
    }
    let raw_origin = configured_origin?;
    if raw_origin.trim() != raw_origin {
        return None;
    }
    let mut url = Url::parse(raw_origin).ok()?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return None;
    }

    let (_, authority_and_suffix) = raw_origin.split_once("://")?;
    let authority_end = authority_and_suffix
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_suffix.len());
    let suffix = &authority_and_suffix[authority_end..];
    if !suffix.is_empty() && suffix != "/" {
        return None;
    }

    let mut segments = url.path_segments_mut().ok()?;
    segments.pop_if_empty().push("f").push(funnel_slug);
    if let Some(page_slug) = page_slug {
        segments.push(page_slug);
    }
    drop(segments);
    Some(url.to_string())
}

pub(crate) fn insert_public_canonical(html: &str, canonical_url: &str) -> Option<String> {
    let head_end = html.find("</head>")?;
    let head = &html[..head_end];
    if head.contains("rel=\"canonical\"") || head.contains("rel='canonical'") {
        return Some(html.to_owned());
    }

    let mut output = String::with_capacity(html.len() + canonical_url.len() + 48);
    output.push_str(head);
    output.push_str("<link rel=\"canonical\" href=\"");
    output.push_str(&escape_html_attribute(canonical_url));
    output.push_str("\">");
    output.push_str(&html[head_end..]);
    Some(output)
}

fn valid_slug(slug: &str) -> bool {
    (1..=128).contains(&slug.len())
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

fn escape_html_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            control if control.is_control() => escaped.push_str("&#xfffd;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{insert_public_canonical, public_page_url};

    #[test]
    fn canonical_urls_use_only_a_valid_https_origin_and_public_route_slugs() {
        assert_eq!(
            public_page_url(Some("https://funnels.example/"), "lead-gen", None),
            Some("https://funnels.example/f/lead-gen".to_owned())
        );
        assert_eq!(
            public_page_url(
                Some("https://funnels.example"),
                "lead-gen",
                Some("checkout")
            ),
            Some("https://funnels.example/f/lead-gen/checkout".to_owned())
        );
        assert_eq!(
            public_page_url(Some("https://funnels.example:8443"), "lead-gen", None),
            Some("https://funnels.example:8443/f/lead-gen".to_owned())
        );
    }

    #[test]
    fn canonical_urls_fail_closed_for_non_origin_or_untrusted_configuration() {
        for invalid in [
            "",
            "http://funnels.example",
            "https://user@funnels.example",
            "https://funnels.example/path",
            "https://funnels.example/path/..",
            "https://funnels.example?campaign=x",
            "https://funnels.example#section",
            " https://funnels.example",
        ] {
            assert_eq!(
                public_page_url(Some(invalid), "lead-gen", None),
                None,
                "{invalid}"
            );
        }
        assert_eq!(public_page_url(None, "lead-gen", None), None);
        assert_eq!(
            public_page_url(Some("https://funnels.example"), "lead gen", None),
            None
        );
        assert_eq!(
            public_page_url(
                Some("https://funnels.example"),
                "lead-gen",
                Some("Checkout")
            ),
            None
        );
    }

    #[test]
    fn canonical_link_is_inserted_in_the_head_and_html_escaped() {
        let html = "<!doctype html><html><head><title>Offer</title></head><body></body></html>";
        assert_eq!(
            insert_public_canonical(html, "https://funnels.example/f/lead-gen?a=1&b=2"),
            Some("<!doctype html><html><head><title>Offer</title><link rel=\"canonical\" href=\"https://funnels.example/f/lead-gen?a=1&amp;b=2\"></head><body></body></html>".to_owned())
        );
        assert_eq!(
            insert_public_canonical("<html><body></body></html>", "https://example.test"),
            None
        );
    }
}
