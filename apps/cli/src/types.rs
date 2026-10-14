use serde::{Deserialize, Serialize};
use url::{Host, Url};

pub type Result<T> = std::result::Result<T, String>;
const MAX_ENDPOINT_BYTES: usize = 2 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct Endpoint(String);
impl TryFrom<String> for Endpoint {
    type Error = String;
    fn try_from(value: String) -> Result<Self> {
        if value.len() > MAX_ENDPOINT_BYTES {
            return Err("endpoint cannot exceed 2 KiB".into());
        }
        if value.chars().any(char::is_control) || value.contains('\\') || !value.contains("://") {
            return Err(
                "endpoint must be an HTTPS origin (HTTP is allowed only on loopback)".into(),
            );
        }
        let authority = value
            .split_once("://")
            .map(|(_, authority)| authority)
            .ok_or("invalid endpoint URL")?;
        if authority
            .split_once('/')
            .is_some_and(|(_, path)| !path.is_empty())
        {
            return Err("endpoint cannot contain a path".into());
        }
        let url = Url::parse(&value).map_err(|_| "invalid endpoint URL")?;
        let local = match url.host() {
            Some(Host::Ipv4(ip)) => ip.is_loopback(),
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            Some(Host::Domain(host)) => host == "localhost",
            None => false,
        };
        if !(url.scheme() == "https" || url.scheme() == "http" && local)
            || url.host().is_none()
            || url.port() == Some(0)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || value.contains('@')
        {
            return Err("endpoint must be an HTTPS origin without credentials, path, query, or fragment (HTTP is allowed only on loopback)".into());
        }
        let normalized = url.as_str().trim_end_matches('/');
        if normalized.len() > MAX_ENDPOINT_BYTES {
            return Err("normalized endpoint cannot exceed 2 KiB".into());
        }
        Ok(Self(normalized.to_owned()))
    }
}
impl From<Endpoint> for String {
    fn from(value: Endpoint) -> Self {
        value.0
    }
}
impl Endpoint {
    pub fn route(&self, path: &str) -> String {
        format!("{}{path}", self.0)
    }
}

macro_rules! route_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl TryFrom<String> for $name {
            type Error = String;
            fn try_from(value: String) -> Result<Self> {
                if !(1..=64).contains(&value.len())
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                {
                    return Err(
                        "IDs must contain 1–64 ASCII letters, digits, underscores, or hyphens"
                            .into(),
                    );
                }
                Ok(Self(value))
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
route_id!(FunnelId);
route_id!(RevisionId);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct FunnelName(String);
impl TryFrom<String> for FunnelName {
    type Error = String;
    fn try_from(value: String) -> Result<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > 120 || value.chars().any(char::is_control) {
            return Err("name must contain 1–120 UTF-8 bytes and no control characters".into());
        }
        Ok(Self(trimmed.to_owned()))
    }
}
impl From<FunnelName> for String {
    fn from(value: FunnelName) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct OperationId(String);
impl TryFrom<String> for OperationId {
    type Error = String;
    fn try_from(value: String) -> Result<Self> {
        let id =
            uuid::Uuid::parse_str(&value).map_err(|_| "operation ID must be a canonical UUID")?;
        if id.to_string() != value || id.get_version_num() != 4 {
            return Err("operation ID must be a canonical version 4 UUID".into());
        }
        Ok(Self(value))
    }
}
impl From<OperationId> for String {
    fn from(value: OperationId) -> Self {
        value.0
    }
}
impl OperationId {
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_reject_normalization_tricks() {
        for value in [
            "https://example.com/..",
            "https://example.com/a/..",
            "http://127.0.0.1.example.com",
            "https://example.com:0",
            "https://example.com\n",
            "https://example.com/@",
        ] {
            assert!(Endpoint::try_from(value.to_owned()).is_err(), "{value}");
        }
        for value in [
            "https://example.com",
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
            "http://localhost:8787",
        ] {
            assert!(Endpoint::try_from(value.to_owned()).is_ok(), "{value}");
        }
    }
}
