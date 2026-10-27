use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct V2LocalDraft {
    pub(super) version: u8,
    pub(super) funnel_id: String,
    pub(super) base_revision_id: String,
    pub(super) document_json: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2LocalStorageResponse {
    ok: bool,
    value: Option<String>,
}

#[cfg(feature = "browser")]
const V2_DRAFT_STORAGE: &str = r#"
const request = await dioxus.recv();
try {
    if (request.action === "read") {
        dioxus.send({ok: true, value: localStorage.getItem(request.key)});
    } else if (request.action === "write") {
        localStorage.setItem(request.key, JSON.stringify(request.value));
        dioxus.send({ok: true, value: null});
    } else if (request.action === "remove") {
        localStorage.removeItem(request.key);
        dioxus.send({ok: true, value: null});
    } else {
        dioxus.send({ok: false, value: null});
    }
} catch (_) {
    dioxus.send({ok: false, value: null});
}
"#;

#[cfg(feature = "browser")]
pub(super) async fn read_v2_local_draft(funnel_id: &str) -> Option<String> {
    let mut request = document::eval(V2_DRAFT_STORAGE);
    request
        .send(serde_json::json!({
            "action": "read",
            "key": v2_draft_key(funnel_id),
        }))
        .ok()?;
    let response = request.recv::<V2LocalStorageResponse>().await.ok()?;
    (response.ok).then_some(response.value).flatten()
}

#[cfg(feature = "browser")]
pub(super) async fn write_v2_local_draft(draft: &V2LocalDraft) -> bool {
    let Ok(value) = serde_json::to_value(draft) else {
        return false;
    };
    let mut request = document::eval(V2_DRAFT_STORAGE);
    if request
        .send(serde_json::json!({
            "action": "write",
            "key": v2_draft_key(&draft.funnel_id),
            "value": value,
        }))
        .is_err()
    {
        return false;
    }
    request
        .recv::<V2LocalStorageResponse>()
        .await
        .is_ok_and(|response| response.ok)
}

#[cfg(feature = "browser")]
pub(super) async fn remove_v2_local_draft(funnel_id: &str) -> bool {
    let mut request = document::eval(V2_DRAFT_STORAGE);
    if request
        .send(serde_json::json!({
            "action": "remove",
            "key": v2_draft_key(funnel_id),
        }))
        .is_err()
    {
        return false;
    }
    request
        .recv::<V2LocalStorageResponse>()
        .await
        .is_ok_and(|response| response.ok)
}

fn v2_draft_key(funnel_id: &str) -> String {
    format!("eezfunnel:draft:v2:{funnel_id}")
}
