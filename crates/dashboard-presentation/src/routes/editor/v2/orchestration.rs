#[cfg(feature = "browser")]
use super::api::*;
#[cfg(feature = "browser")]
use super::drafts::*;
#[cfg(feature = "browser")]
use super::state::CanonicalDocumentJson;
#[cfg(feature = "browser")]
use super::state::*;
#[cfg(feature = "browser")]
use super::*;

#[cfg(feature = "browser")]
async fn load_v2_editor(funnel_id: &str) -> Result<V2LoadedEditor, ()> {
    let metadata_response = api_request("GET", &format!("/api/funnels/{funnel_id}"), None, None)
        .await
        .map_err(|_| ())?;
    if metadata_response.status != 200 {
        return Err(());
    }
    let metadata: V2EditorMetadata =
        serde_json::from_str(&metadata_response.body).map_err(|_| ())?;
    if metadata.schema_version != 2 || metadata.funnel.id != funnel_id {
        return Err(());
    }
    let stage_capacity = match api_request("GET", "/api/v2/stage-capacity", None, None).await {
        Ok(response) if response.status == 200 => {
            serde_json::from_str::<V2StageCapacity>(&response.body)
                .ok()
                .filter(V2StageCapacity::is_valid)
        }
        _ => None,
    };
    let document_response = api_request(
        "GET",
        &format!("/api/funnels/{funnel_id}/document"),
        None,
        None,
    )
    .await
    .map_err(|_| ())?;
    if document_response.status != 200 {
        return Err(());
    }
    let saved_document =
        DocumentV2::decode(document_response.body.as_bytes(), funnel_id).map_err(|_| ())?;
    let saved_document_json = saved_document.canonical_json();
    let history_response = api_request(
        "GET",
        &format!("/api/funnels/{funnel_id}/revisions-v2"),
        None,
        None,
    )
    .await
    .map_err(|_| ())?;
    if history_response.status != 200 {
        return Err(());
    }
    let revisions: Vec<V2RevisionSummary> =
        serde_json::from_str::<V2RevisionHistory>(&history_response.body)
            .map_err(|_| ())?
            .revisions
            .into_iter()
            .filter(|revision| revision.schema_version == 2)
            .collect();

    let mut document = saved_document.clone();
    let mut recovered_draft = None;
    let mut recovered_current_draft = false;
    let mut recovery_error = None;
    if let Some(raw_draft) = read_v2_local_draft(funnel_id).await {
        match serde_json::from_str::<V2LocalDraft>(&raw_draft) {
            Ok(draft) if draft.version == 1 && draft.funnel_id == funnel_id => {
                match DocumentV2::decode(draft.document_json.as_bytes(), funnel_id) {
                    Ok(local_document)
                        if draft.base_revision_id == metadata.base_revision_id
                            && local_document.canonical_json() != saved_document_json =>
                    {
                        document = local_document;
                        recovered_current_draft = true;
                    }
                    Ok(local_document)
                        if draft.base_revision_id != metadata.base_revision_id
                            && local_document.canonical_json() != saved_document_json =>
                    {
                        recovered_draft = Some(draft);
                    }
                    Ok(_) => {
                        let _ = remove_v2_local_draft(funnel_id).await;
                    }
                    Err(_) => {
                        recovery_error =
                            Some("The saved browser draft is invalid and was preserved.".to_owned())
                    }
                }
            }
            Ok(_) => {
                recovery_error = Some(
                    "The saved browser draft belongs to a different version or funnel.".to_owned(),
                )
            }
            Err(_) => {
                recovery_error =
                    Some("The saved browser draft could not be read and was preserved.".to_owned())
            }
        }
    }
    let selected_page = document.entry_page_id().as_str().to_owned();
    let selected_section = document
        .pages()
        .iter()
        .find(|page| page.id().as_str() == selected_page)
        .and_then(|page| page.sections().first())
        .map(|section| section.id().as_str().to_owned());
    let preview_html = fetch_v2_preview(
        funnel_id,
        &metadata.base_revision_id,
        &selected_page,
        &document.canonical_json(),
    )
    .await
    .map_err(|_| ())?;
    Ok(V2LoadedEditor {
        metadata,
        document,
        saved_document_json,
        preview_html,
        selected_page,
        selected_section,
        revisions,
        stage_capacity,
        recovered_draft,
        recovered_current_draft,
        recovery_error,
    })
}

#[cfg(feature = "browser")]
pub(super) async fn load_v2_media_assets(
    mut state: Signal<V2EditorState>,
    funnel_id: String,
    route_epoch: u64,
) {
    if state.try_peek().is_ok() {
        state.with_mut(|state| {
            if state.route_epoch == route_epoch && state.funnel_id == funnel_id {
                state.media_loading = true;
                state.media_error = None;
            }
        });
    }
    let result = fetch_v2_media_assets().await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_media_load(route_epoch, &funnel_id, result));
    }
}

#[cfg(feature = "browser")]
pub(super) async fn upload_v2_media(mut state: Signal<V2EditorState>) {
    let capture = state.with_mut(|state| {
        if state.media_uploading || !state.can_edit() {
            return None;
        }
        state.media_uploading = true;
        state.media_error = None;
        Some((state.route_epoch, state.funnel_id.clone()))
    });
    let Some((route_epoch, funnel_id)) = capture else {
        return;
    };
    let mut request = document::eval(V2_MEDIA_UPLOAD);
    let response = if request.send(serde_json::json!({})).is_ok() {
        request
            .recv::<crate::routes::api::BrowserResponse>()
            .await
            .ok()
    } else {
        None
    };
    if state.try_peek().is_err() {
        return;
    }
    state.with_mut(|state| {
        if state.route_epoch != route_epoch || state.funnel_id != funnel_id {
            return;
        }
        state.media_uploading = false;
        match response {
            Some(response) if response.status == 201 => {
                match serde_json::from_str::<V2MediaUploadResponse>(&response.body) {
                    Ok(result) => {
                        state.media_assets.retain(|asset| asset.id != result.asset.id);
                        state.media_assets.insert(0, result.asset.clone());
                        state.selected_media_asset = Some(result.asset.id);
                        state.media_error = None;
                        state.announcement = "Image uploaded and selected in the media library.".to_owned();
                    }
                    Err(_) => state.media_error = Some("The Worker returned an invalid image record.".to_owned()),
                }
            }
            Some(response) => {
                state.media_error = Some(if response.status == 0 || response.status >= 500 {
                    "The image upload result is uncertain. Refresh the media library before retrying.".to_owned()
                } else {
                    error_message(&response.body)
                });
            }
            None => state.media_error = Some("The image upload result is uncertain. Refresh the media library before retrying.".to_owned()),
        }
    });
}

#[cfg(feature = "browser")]
fn v2_failure(response: &crate::routes::api::BrowserResponse) -> V2RequestFailure {
    let code = serde_json::from_str::<serde_json::Value>(&response.body)
        .ok()
        .and_then(|value| {
            value
                .get("code")
                .and_then(|code| code.as_str())
                .map(str::to_owned)
        });
    V2RequestFailure {
        code,
        retryable: response.status == 0 || response.status >= 500,
        message: if response.status == 0 || response.status >= 500 {
            "The result is uncertain. Retry the same action; the draft remains in this browser."
                .to_owned()
        } else {
            error_message(&response.body)
        },
    }
}

#[cfg(feature = "browser")]
pub(super) async fn load_v2_editor_state(
    mut state: Signal<V2EditorState>,
    funnel_id: String,
    route_epoch: u64,
) {
    let media_state = state;
    let media_funnel_id = funnel_id.clone();
    spawn(async move {
        load_v2_media_assets(media_state, media_funnel_id, route_epoch).await;
    });
    let loaded = load_v2_editor(&funnel_id).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_load(route_epoch, &funnel_id, loaded));
    }
}

#[cfg(feature = "browser")]
pub(super) fn dispatch_v2_edit(mut state: Signal<V2EditorState>, edit: Edit) {
    let operation_id = Uuid::new_v4().simple().to_string();
    let capture = state.with_mut(|state| state.apply_local_edit(edit, operation_id));
    let Some(capture) = capture else {
        return;
    };
    let draft = V2LocalDraft {
        version: 1,
        funnel_id: capture.funnel_id.clone(),
        base_revision_id: capture.base_revision_id.clone(),
        document_json: capture.document_json.clone(),
    };
    spawn(async move {
        let local_write_succeeded = write_v2_local_draft(&draft).await;
        let body = serde_json::json!({
            "base_revision_id": capture.base_revision_id,
            "page_id": capture.page_id,
            "document_json": capture.base_document_json,
            "operation_id": capture.operation_id,
            "edit": capture.edit,
        })
        .to_string();
        let response = api_request(
            "POST",
            &format!("/api/funnels/{}/edit-v2", capture.funnel_id),
            Some(body),
            None,
        )
        .await;
        let result = match response {
            Ok(response) if response.status == 200 => {
                serde_json::from_str::<V2EditResponse>(&response.body)
                    .map_err(|_| V2RequestFailure {
                        code: None,
                        retryable: false,
                        message: "The Worker returned an unreadable edit result. The local draft is preserved.".to_owned(),
                    })
            }
            Ok(response) => Err(v2_failure(&response)),
            Err(()) => Err(V2RequestFailure {
                code: None,
                retryable: true,
                message: "The edit result is uncertain. The local draft is preserved; retry after checking the connection.".to_owned(),
            }),
        };
        if state.try_peek().is_ok() {
            state.with_mut(|state| {
                state.finish_edit(&capture, result);
                if !local_write_succeeded {
                    state.recovery_error =
                        Some("The browser could not persist this draft locally.".to_owned());
                }
            });
        }
    });
}

#[cfg(feature = "browser")]
pub(super) fn schedule_v2_preview(
    mut state: Signal<V2EditorState>,
    request: Option<(u64, String, String, String)>,
) {
    let Some((generation, funnel_id, base_revision_id, document_json)) = request else {
        return;
    };
    let page_id = state.with(|state| state.selected_page.clone());
    spawn(async move {
        let result =
            fetch_v2_preview(&funnel_id, &base_revision_id, &page_id, &document_json).await;
        if state.try_peek().is_ok() {
            state.with_mut(|state| {
                if state.generation == generation && state.base_revision_id == base_revision_id {
                    match result {
                        Ok(preview_html) => {
                            state.preview_html = preview_html;
                            state.status = Some("Draft preview refreshed.".to_owned());
                        }
                        Err(()) => {
                            state.error =
                                Some("The draft preview could not be refreshed.".to_owned())
                        }
                    }
                }
            });
        }
    });
}

#[cfg(feature = "browser")]
pub(super) async fn save_v2_revision(mut state: Signal<V2EditorState>) {
    let capture = state.with_mut(V2EditorState::begin_save);
    let Some(capture) = capture else {
        return;
    };
    let digest = sha256_hex(capture.document_json.as_bytes());
    let stage_body = serde_json::json!({
        "base_revision_id": capture.base_revision_id,
        "expected_length": capture.document_json.len(),
        "expected_sha256": digest,
    })
    .to_string();
    let stage_path = format!("/api/funnels/{}/stages-v2", capture.funnel_id);
    if capture.resume_finalization {
        let retry = state.with(|state| {
            let stage_id = state.save_stage_id.clone()?;
            let save_body = state.save_body.clone()?;
            let save_key = state.save_key.clone()?;
            let stage_key = state.stage_key.clone();
            let expected_save_body = serde_json::json!({
                "base_revision_id": capture.base_revision_id,
                "stage_id": stage_id,
                "expected_length": capture.document_json.len(),
                "expected_sha256": digest,
            })
            .to_string();
            (save_body == expected_save_body).then_some((stage_key, save_body, save_key))
        });
        if let Some((stage_key, save_body, save_key)) = retry {
            finalize_v2_save(state, &capture, stage_key, save_body, save_key).await;
        } else {
            fail_v2_save(
                state,
                &capture,
                V2RequestFailure {
                    code: Some("conflict/edit_stale_revision".to_owned()),
                    retryable: false,
                    message: "The pending save could not be resumed safely. Reload the current revision before saving again.".to_owned(),
                },
            );
        }
        return;
    }
    state.with_mut(|state| {
        state.save_phase = V2SavePhase::Staging;
        state.save_stage_id = None;
    });
    let previous_stage_key = state.with(|state| {
        (state.stage_body.as_deref() == Some(stage_body.as_str()))
            .then(|| state.stage_key.clone())
            .flatten()
    });
    let stage_key =
        prepare_idempotency_key("POST", &stage_path, Some(&stage_body), previous_stage_key);
    let stage_key = match stage_key {
        Ok(key) => key,
        Err(()) => {
            state.with_mut(|state| {
                state.saving = false;
                state.error =
                    Some("The browser could not prepare a retry-safe stage request.".to_owned());
                state.status = None;
            });
            return;
        }
    };
    state.with_mut(|state| {
        state.stage_key = Some(stage_key.clone());
        state.stage_body = Some(stage_body.clone());
    });
    let stage_response = api_request(
        "POST",
        &stage_path,
        Some(stage_body),
        Some(stage_key.clone()),
    )
    .await;
    let stage_id = match stage_response {
        Ok(response) if response.status == 201 => {
            match serde_json::from_str::<V2StageResponse>(&response.body) {
                Ok(stage) => stage.stage_id,
                Err(_) => {
                    fail_v2_save(
                        state,
                        &capture,
                        V2RequestFailure {
                            code: None,
                            retryable: true,
                            message:
                                "The stage response was invalid. Your browser draft is preserved."
                                    .to_owned(),
                        },
                    );
                    return;
                }
            }
        }
        Ok(response) => {
            fail_v2_save(state, &capture, v2_failure(&response));
            return;
        }
        Err(()) => {
            fail_v2_save(
                state,
                &capture,
                V2RequestFailure {
                    code: None,
                    retryable: true,
                    message: "The stage request result is uncertain. Retry the same save."
                        .to_owned(),
                },
            );
            return;
        }
    };
    let upload_response =
        upload_v2_stage(&capture.funnel_id, &stage_id, &capture.document_json).await;
    match upload_response {
        Ok(response) if response.status == 204 => {}
        Ok(response) => {
            fail_v2_save(state, &capture, v2_failure(&response));
            return;
        }
        Err(()) => {
            fail_v2_save(
                state,
                &capture,
                V2RequestFailure {
                    code: None,
                    retryable: true,
                    message: "The upload result is uncertain. Retry the same save to confirm it."
                        .to_owned(),
                },
            );
            return;
        }
    }
    let save_body = serde_json::json!({
        "base_revision_id": capture.base_revision_id,
        "stage_id": stage_id,
        "expected_length": capture.document_json.len(),
        "expected_sha256": digest,
    })
    .to_string();
    let save_path = format!("/api/funnels/{}/revisions-v2", capture.funnel_id);
    let previous_save_key = state.with(|state| {
        (state.save_body.as_deref() == Some(save_body.as_str()))
            .then(|| state.save_key.clone())
            .flatten()
    });
    let save_key =
        match prepare_idempotency_key("POST", &save_path, Some(&save_body), previous_save_key) {
            Ok(key) => key,
            Err(()) => {
                fail_v2_save(
                    state,
                    &capture,
                    V2RequestFailure {
                        code: None,
                        retryable: false,
                        message: "The browser could not prepare a retry-safe final save."
                            .to_owned(),
                    },
                );
                return;
            }
        };
    state.with_mut(|state| {
        state.save_key = Some(save_key.clone());
        state.save_body = Some(save_body.clone());
        state.save_stage_id = Some(stage_id.clone());
        state.save_phase = V2SavePhase::Finalizing;
    });
    finalize_v2_save(state, &capture, Some(stage_key), save_body, save_key).await;
}

#[cfg(feature = "browser")]
async fn finalize_v2_save(
    mut state: Signal<V2EditorState>,
    capture: &V2SaveCapture,
    stage_key: Option<PreparedIdempotencyKey>,
    save_body: String,
    save_key: PreparedIdempotencyKey,
) {
    let save_path = format!("/api/funnels/{}/revisions-v2", capture.funnel_id);
    let save_response =
        api_request("POST", &save_path, Some(save_body), Some(save_key.clone())).await;
    let result = match save_response {
        Ok(response) if response.status == 201 => {
            serde_json::from_str::<V2SaveResponse>(&response.body).map_err(|_| V2RequestFailure {
                code: None,
                retryable: true,
                message: "The save response was invalid. The local draft remains available."
                    .to_owned(),
            })
        }
        Ok(response) => Err(v2_failure(&response)),
        Err(()) => Err(V2RequestFailure {
            code: None,
            retryable: true,
            message: "The final save result is uncertain. Retry the same action to confirm it."
                .to_owned(),
        }),
    };
    let saved =
        state.try_peek().is_ok() && state.with_mut(|state| state.finish_save(capture, result));
    if saved {
        if let Some(stage_key) = stage_key.as_ref() {
            let _ = clear_idempotency_storage(stage_key);
        }
        let _ = clear_idempotency_storage(&save_key);
        let _ = remove_v2_local_draft(&capture.funnel_id).await;
        if state.try_peek().is_ok() {
            state.with_mut(|state| {
                state.stage_key = None;
                state.stage_body = None;
                state.save_key = None;
                state.save_body = None;
                state.save_stage_id = None;
            });
        }
        let revision_id = state.with(|state| state.base_revision_id.clone());
        refresh_v2_readiness(state, capture, revision_id).await;
    }
}

#[cfg(feature = "browser")]
fn fail_v2_save(
    mut state: Signal<V2EditorState>,
    capture: &V2SaveCapture,
    failure: V2RequestFailure,
) {
    if state.try_peek().is_ok() && state.with(|state| state.is_current_save(capture)) {
        state.with_mut(|state| {
            state.finish_save(capture, Err(failure));
        });
    }
}

#[cfg(feature = "browser")]
pub(super) async fn compare_v2_revision(mut state: Signal<V2EditorState>, revision_id: String) {
    let capture = state.with_mut(|state| state.begin_compare(revision_id));
    let Some(capture) = capture else {
        return;
    };
    let response = api_request(
        "GET",
        &format!(
            "/api/funnels/{}/document?revision_id={}",
            capture.funnel_id, capture.revision_id
        ),
        None,
        None,
    )
    .await;
    let result = match response {
        Ok(response) if response.status == 200 => {
            DocumentV2::decode(response.body.as_bytes(), &capture.funnel_id).map_err(|_| ())
        }
        _ => Err(()),
    };
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_compare(&capture, result));
    }
}

#[cfg(feature = "browser")]
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            output
        })
}

#[cfg(feature = "browser")]
pub(super) async fn publish_v2_revision(mut state: Signal<V2EditorState>, publish: bool) {
    let capture = state.with_mut(|state| state.begin_publication(publish));
    let Some(capture) = capture else {
        return;
    };
    let path = capture.action.path(&capture.funnel_id);
    let body = capture.action.body();
    let previous_key = state.with(|state| {
        (state.is_current_publication(&capture)
            && state.publication_body.as_deref() == Some(body.as_str()))
        .then(|| state.publication_key.clone())
        .flatten()
    });
    let key = match prepare_idempotency_key("POST", &path, Some(&body), previous_key) {
        Ok(key) => key,
        Err(()) => {
            state.with_mut(|state| {
                if !state.is_current_publication(&capture) {
                    return;
                }
                state.publication_pending = false;
                state.publication_error =
                    Some("The browser could not prepare this publication request.".to_owned());
            });
            return;
        }
    };
    if !state.with(|state| state.is_current_publication(&capture) && state.publication_pending) {
        let _ = clear_idempotency_storage(&key);
        return;
    }
    state.with_mut(|state| {
        if state.is_current_publication(&capture) {
            state.publication_key = Some(key.clone());
            state.publication_body = Some(body.clone());
        }
    });
    let response = api_request("POST", &path, Some(body), Some(key.clone())).await;
    match response {
        Ok(response) if response.status == 200 => {
            let result = serde_json::from_str::<PublicationResponse>(&response.body)
                .map(|response| response.published_revision_id)
                .map_err(|_| ());
            let succeeded = state.with_mut(|state| state.finish_publication(&capture, result));
            if succeeded {
                let _ = clear_idempotency_storage(&key);
                state.with_mut(|state| {
                    if state.is_current_publication(&capture) {
                        state.publication_key = None;
                        state.publication_body = None;
                    }
                });
            }
        }
        Ok(response) => {
            let failure = v2_failure(&response);
            if failure.code.as_deref() == Some("conflict/publish_stale_revision") {
                let should_refresh =
                    state.with_mut(|state| state.begin_publication_refresh(&capture));
                let _ = clear_idempotency_storage(&key);
                if should_refresh {
                    state.with_mut(|state| {
                        if state.is_current_publication(&capture) {
                            state.publication_key = None;
                            state.publication_body = None;
                        }
                    });
                    let latest = refresh_v2_publication_state(&capture.funnel_id).await;
                    state.with_mut(|state| state.finish_publication_refresh(&capture, latest));
                }
            } else {
                state.with_mut(|state| {
                    if !state.is_current_publication(&capture) {
                        return;
                    }
                    state.publication_pending = false;
                    state.publication_retryable = failure.retryable;
                    if !failure.retryable {
                        state.publication_attempt = None;
                        state.publication_key = None;
                        state.publication_body = None;
                    }
                    state.publication_error = Some(failure.message.clone());
                    state.publication_status = None;
                });
                if !failure.retryable {
                    let _ = clear_idempotency_storage(&key);
                    if failure.code.as_deref() == Some("conflict/publication_readiness_blocked") {
                        if let Some(revision_id) = capture.action.resulting_revision_id() {
                            refresh_v2_readiness_for_revision(
                                state,
                                capture.route_epoch,
                                capture.funnel_id.clone(),
                                revision_id.to_owned(),
                            )
                            .await;
                        }
                    }
                }
            }
        }
        Err(()) => state.with_mut(|state| {
            if !state.is_current_publication(&capture) {
                return;
            }
            state.publication_pending = false;
            state.publication_retryable = true;
            state.publication_error = Some(
                "The publication result is uncertain. Retry the same request to confirm it."
                    .to_owned(),
            );
            state.publication_status = None;
        }),
    }
}

#[cfg(feature = "browser")]
async fn refresh_v2_publication_state(funnel_id: &str) -> Result<Option<String>, ()> {
    let metadata = fetch_v2_editor_metadata(funnel_id).await?;
    Ok(metadata.published_revision_id)
}

#[cfg(feature = "browser")]
async fn refresh_v2_readiness(
    state: Signal<V2EditorState>,
    capture: &V2SaveCapture,
    revision_id: String,
) {
    refresh_v2_readiness_for_revision(
        state,
        capture.route_epoch,
        capture.funnel_id.clone(),
        revision_id,
    )
    .await;
}

#[cfg(feature = "browser")]
async fn refresh_v2_readiness_for_revision(
    mut state: Signal<V2EditorState>,
    route_epoch: u64,
    funnel_id: String,
    revision_id: String,
) {
    let metadata = fetch_v2_editor_metadata(&funnel_id).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| {
            state.finish_readiness_refresh(route_epoch, &funnel_id, &revision_id, metadata)
        });
    }
}

#[cfg(feature = "browser")]
async fn fetch_v2_editor_metadata(funnel_id: &str) -> Result<V2EditorMetadata, ()> {
    let response = api_request("GET", &format!("/api/funnels/{funnel_id}"), None, None)
        .await
        .map_err(|_| ())?;
    if response.status != 200 {
        return Err(());
    }
    let metadata: V2EditorMetadata = serde_json::from_str(&response.body).map_err(|_| ())?;
    if metadata.schema_version != 2 || metadata.funnel.id != funnel_id {
        return Err(());
    }
    Ok(metadata)
}
