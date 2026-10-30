use super::*;
#[cfg(feature = "browser")]
use crate::routes::api::api_request;
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct V2EditorMetadata {
    pub(super) schema_version: u32,
    pub(super) funnel: EditorFunnel,
    pub(super) base_revision_id: String,
    pub(super) published_revision_id: Option<String>,
    #[serde(default)]
    pub(super) publication_readiness: Option<PublicationReadiness>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct V2RevisionSummary {
    pub(super) revision_id: String,
    pub(super) schema_version: i64,
    pub(super) created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct V2RevisionHistory {
    pub(super) revisions: Vec<V2RevisionSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(super) struct V2MediaAsset {
    pub(super) id: String,
    pub(super) mime: String,
    pub(super) size_bytes: u64,
    pub(super) sha256: String,
    pub(super) created_at: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2MediaLibraryResponse {
    pub(super) assets: Vec<V2MediaAsset>,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
pub(super) struct V2MediaUploadResponse {
    pub(super) asset: V2MediaAsset,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct V2StageCapacity {
    pub(super) orphan_byte_budget: u64,
    pub(super) orphan_bytes_held: u64,
    pub(super) orphan_key_budget: u64,
    pub(super) orphan_keys_held: u64,
    pub(super) worst_case_orphan_bytes: u64,
    pub(super) worst_case_orphan_keys: u64,
}

impl V2StageCapacity {
    pub(super) fn is_valid(&self) -> bool {
        self.orphan_byte_budget > 0
            && self.orphan_bytes_held <= self.orphan_byte_budget
            && self.orphan_key_budget > 0
            && self.orphan_keys_held <= self.orphan_key_budget
            && self.worst_case_orphan_bytes == self.orphan_byte_budget
            && self.worst_case_orphan_keys == self.orphan_key_budget
    }
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
pub(super) struct V2EditResponse {
    pub(super) base_revision_id: String,
    pub(super) document_json: String,
    pub(super) preview_html: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2PreviewResponse {
    preview_html: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
pub(super) struct V2StageResponse {
    pub(super) stage_id: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
pub(super) struct V2SaveResponse {
    pub(super) funnel_id: String,
    pub(super) base_revision_id: String,
    pub(super) revision_id: String,
    pub(super) schema_version: u32,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct V2RequestFailure {
    pub(super) code: Option<String>,
    pub(super) retryable: bool,
    pub(super) message: String,
}

#[cfg(feature = "browser")]
const V2_BINARY_UPLOAD: &str = r#"
const request = await dioxus.recv();
try {
    const bytes = new TextEncoder().encode(request.document_json);
    const response = await fetch(request.path, {
        method: "PUT",
        headers: {
            "Authorization": `Bearer ${request.stage_id}`,
            "Content-Type": "application/octet-stream"
        },
        body: bytes
    });
    dioxus.send({status: response.status, body: await response.text()});
} catch (_) {
    dioxus.send({status: 0, body: ""});
}
"#;

#[cfg(feature = "browser")]
pub(super) const V2_MEDIA_UPLOAD: &str = r#"
const request = await dioxus.recv();
try {
    const input = document.getElementById("v2-media-upload");
    const file = input?.files?.[0];
    if (!file) {
        dioxus.send({status: 0, body: ""});
    } else {
        const response = await fetch("/api/media", {
            method: "POST",
            headers: {"Content-Type": file.type},
            body: file
        });
        dioxus.send({status: response.status, body: await response.text()});
        input.value = "";
    }
} catch (_) {
    dioxus.send({status: 0, body: ""});
}
"#;

#[cfg(feature = "browser")]
pub(super) async fn fetch_v2_preview(
    funnel_id: &str,
    base_revision_id: &str,
    page_id: &str,
    document_json: &str,
) -> Result<String, ()> {
    let body = serde_json::json!({
        "base_revision_id": base_revision_id,
        "page_id": page_id,
        "document_json": document_json,
    })
    .to_string();
    let response = api_request(
        "POST",
        &format!("/api/funnels/{funnel_id}/preview-v2"),
        Some(body),
        None,
    )
    .await
    .map_err(|_| ())?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<V2PreviewResponse>(&response.body)
        .map(|response| response.preview_html)
        .map_err(|_| ())
}

#[cfg(feature = "browser")]
pub(super) async fn fetch_v2_media_assets() -> Result<Vec<V2MediaAsset>, String> {
    let response = api_request("GET", "/api/media", None, None)
        .await
        .map_err(|_| "The media library could not be reached.".to_owned())?;
    if response.status != 200 {
        return Err(error_message(&response.body));
    }
    serde_json::from_str::<V2MediaLibraryResponse>(&response.body)
        .map(|response| response.assets)
        .map_err(|_| "The media library returned an invalid response.".to_owned())
}

#[cfg(feature = "browser")]
pub(super) async fn upload_v2_stage(
    funnel_id: &str,
    stage_id: &str,
    document_json: &str,
) -> Result<crate::routes::api::BrowserResponse, ()> {
    let mut request = document::eval(V2_BINARY_UPLOAD);
    request
        .send(serde_json::json!({
            "path": format!("/api/funnels/{funnel_id}/stages-v2"),
            "stage_id": stage_id,
            "document_json": document_json,
        }))
        .map_err(|_| ())?;
    request.recv().await.map_err(|_| ())
}
