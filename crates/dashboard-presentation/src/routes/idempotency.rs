#[cfg(any(feature = "browser", test))]
use sha2::{Digest, Sha256};
#[cfg(feature = "browser")]
use uuid::Uuid;

#[cfg(feature = "browser")]
pub(super) type IdempotencyKeyState = PreparedIdempotencyKey;
#[cfg(not(feature = "browser"))]
pub(super) type IdempotencyKeyState = String;

#[cfg(feature = "browser")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PreparedIdempotencyKey {
    pub(super) key: String,
    pub(super) slot: String,
}

#[cfg(any(feature = "browser", test))]
pub(super) fn idempotency_storage_slot(method: &str, path: &str, body: Option<&str>) -> String {
    let body = body
        .map(|body| {
            serde_json::from_str::<serde_json::Value>(body)
                .unwrap_or_else(|_| serde_json::Value::String(body.to_owned()))
        })
        .unwrap_or(serde_json::Value::Null);
    let canonical_request = serde_json::json!([method, path, body]).to_string();
    let digest = Sha256::digest(canonical_request.as_bytes());
    let digest_hex = digest
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            output
        });
    format!("eezfunnel:idempotency:v1:{method}:{path}:{digest_hex}")
}

#[cfg(feature = "browser")]
pub(super) fn prepare_idempotency_key(
    method: &str,
    path: &str,
    body: Option<&str>,
    retry_key: Option<PreparedIdempotencyKey>,
) -> Result<PreparedIdempotencyKey, ()> {
    let slot = idempotency_storage_slot(method, path, body);
    let storage = web_sys::window()
        .ok_or(())?
        .session_storage()
        .map_err(|_| ())?
        .ok_or(())?;
    let stored_key = storage
        .get_item(&slot)
        .map_err(|_| ())?
        .and_then(|stored| Uuid::parse_str(&stored).ok().map(|key| key.to_string()));
    let retry_key = retry_key
        .filter(|retry_key| retry_key.slot == slot)
        .and_then(|retry_key| {
            Uuid::parse_str(&retry_key.key)
                .ok()
                .map(|key| key.to_string())
        });
    let key = retry_key
        .or(stored_key)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    storage.set_item(&slot, &key).map_err(|_| ())?;
    Ok(PreparedIdempotencyKey { key, slot })
}

#[cfg(feature = "browser")]
pub(super) fn clear_idempotency_storage(prepared_key: &PreparedIdempotencyKey) -> Result<(), ()> {
    let storage = web_sys::window()
        .ok_or(())?
        .session_storage()
        .map_err(|_| ())?
        .ok_or(())?;
    if storage
        .get_item(&prepared_key.slot)
        .map_err(|_| ())?
        .as_deref()
        == Some(prepared_key.key.as_str())
    {
        storage.remove_item(&prepared_key.slot).map_err(|_| ())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idempotency_storage_slot_uses_canonical_payload_digest_without_exposing_fields() {
        let body = r#"{"name":"Offer","blueprint":"tripwire"}"#;
        let original = idempotency_storage_slot("POST", "/api/funnels", Some(body));
        assert_eq!(
            original,
            idempotency_storage_slot(
                "POST",
                "/api/funnels",
                Some(r#"{ "blueprint": "tripwire", "name": "Offer" }"#),
            )
        );
        assert!(!original.contains("Offer"));
        assert!(!original.contains("tripwire"));
        assert!(!original.contains(body));
        assert_ne!(
            original,
            idempotency_storage_slot(
                "POST",
                "/api/funnels",
                Some(r#"{"name":"Offer 2","blueprint":"tripwire"}"#),
            )
        );
        assert_ne!(
            original,
            idempotency_storage_slot("DELETE", "/api/funnels/funnel-1", None)
        );
    }
}
