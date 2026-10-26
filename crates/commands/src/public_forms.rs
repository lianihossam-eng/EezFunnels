use std::collections::BTreeMap;

use eez_domain::document_v2::{
    validate_form_submission, BlockV2, DocumentV2, FormId, FormResult, FormV2, PageId, PageV2,
};
use eez_domain::FormSubmissionChannel;
use eez_render::document_v2::{
    referenced_asset_ids, validate_revision_v2_with_assets, AssetDigestRecord,
};
use eez_storage::media::MediaAssetRepository;
use eez_storage::repository::{
    NewPublicFormSubmission, PublicFormSubmissionResult, Repository, RepositoryError,
};

pub struct SubmitPublicFormInput<'a> {
    pub funnel_slug: &'a str,
    pub page_id: PageId,
    pub form_id: FormId,
    pub revision_id: &'a str,
    pub submission_key: &'a str,
    pub submission_channel: FormSubmissionChannel,
    pub fields: &'a BTreeMap<String, String>,
    pub request_consent_accepted: bool,
    pub marketing_opt_in: bool,
    pub retention_seconds: i32,
    pub limit_per_minute: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmitPublicFormOutcome {
    Accepted { success_location: String },
    KeyReuseMismatch,
    RateLimited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmitPublicFormError {
    NotFound,
    StaleRevision,
    InvalidPayload,
    SerializationFailure,
    Storage { code: &'static str },
}

pub async fn submit_public_form(
    repository: &Repository<'_>,
    media_assets: &MediaAssetRepository<'_>,
    input: SubmitPublicFormInput<'_>,
) -> Result<SubmitPublicFormOutcome, SubmitPublicFormError> {
    let revision = match repository
        .get_published_revision_by_slug(input.funnel_slug)
        .await
    {
        Ok(Some(revision)) if revision.schema_version == 2 => revision,
        _ => return Err(SubmitPublicFormError::NotFound),
    };
    if revision.id != input.revision_id {
        return Err(SubmitPublicFormError::StaleRevision);
    }

    let unvalidated_document =
        DocumentV2::decode(revision.data_json.as_bytes(), &revision.funnel_id)
            .map_err(|_| SubmitPublicFormError::NotFound)?;
    let asset_ids = referenced_asset_ids(&unvalidated_document);
    let media_records = match media_assets.get_many_ready(&asset_ids).await {
        Ok(records) if records.len() == asset_ids.len() => records,
        _ => return Err(SubmitPublicFormError::NotFound),
    };
    let assets = media_records
        .into_iter()
        .map(|record| AssetDigestRecord {
            asset_id: record.id,
            sha256: record.sha256,
        })
        .collect::<Vec<_>>();
    let document = validate_revision_v2_with_assets(
        revision.schema_version,
        &revision.funnel_id,
        &revision.id,
        &revision.data_json,
        &revision.render_manifest_json,
        &assets,
    )
    .map_err(|_| SubmitPublicFormError::NotFound)?;
    let Some(page) = document.page(&input.page_id) else {
        return Err(SubmitPublicFormError::NotFound);
    };
    let Some(form) = document.form(&input.form_id) else {
        return Err(SubmitPublicFormError::NotFound);
    };
    let form_is_placed_on_page = page.sections().iter().any(|section| {
        section.blocks().iter().any(
            |block| matches!(block, BlockV2::Form { form_id, .. } if form_id == &input.form_id),
        )
    });
    if !form_is_placed_on_page {
        return Err(SubmitPublicFormError::NotFound);
    }

    let submission = validate_form_submission(
        form,
        input.fields,
        input.request_consent_accepted,
        input.marketing_opt_in,
    )
    .map_err(|_| SubmitPublicFormError::InvalidPayload)?;
    let field_values_json = serde_json::to_string(submission.values())
        .map_err(|_| SubmitPublicFormError::SerializationFailure)?;
    let submission_id = uuid::Uuid::new_v4().to_string();
    let request_consent = form.request_consent();
    let marketing_consent = form.marketing_opt_in();
    let storage_result = repository
        .store_public_form_submission(NewPublicFormSubmission {
            submission_id: &submission_id,
            funnel_id: &revision.funnel_id,
            funnel_slug: input.funnel_slug,
            revision_id: &revision.id,
            page_id: input.page_id.as_str(),
            form_id: input.form_id.as_str(),
            submission_channel: input.submission_channel,
            submission_key: input.submission_key,
            field_values_json: &field_values_json,
            request_consent_copy_version: request_consent.copy_version().as_str(),
            request_consent_copy: request_consent.copy(),
            marketing_opt_in: submission.marketing_opt_in(),
            marketing_opt_in_copy_version: marketing_consent
                .map(|consent| consent.copy_version().as_str()),
            marketing_opt_in_copy: marketing_consent.map(|consent| consent.copy()),
            retention_seconds: input.retention_seconds,
            limit_per_minute: input.limit_per_minute,
        })
        .await
        .map_err(|error: RepositoryError| SubmitPublicFormError::Storage { code: error.code() })?;

    match storage_result {
        PublicFormSubmissionResult::Stored | PublicFormSubmissionResult::Duplicate => {}
        PublicFormSubmissionResult::KeyReuseMismatch => {
            return Ok(SubmitPublicFormOutcome::KeyReuseMismatch)
        }
        PublicFormSubmissionResult::RateLimited => return Ok(SubmitPublicFormOutcome::RateLimited),
        PublicFormSubmissionResult::NotCurrent => return Err(SubmitPublicFormError::StaleRevision),
    }

    let success_location = public_form_success_location(&document, page, form, input.funnel_slug)
        .ok_or(SubmitPublicFormError::NotFound)?;
    Ok(SubmitPublicFormOutcome::Accepted { success_location })
}

fn public_form_success_location(
    document: &DocumentV2,
    current_page: &PageV2,
    form: &FormV2,
    funnel_slug: &str,
) -> Option<String> {
    let target = match form.result() {
        FormResult::Inline { .. } => current_page,
        FormResult::SuccessPage { page_id } => document.page(page_id)?,
    };
    let page_route = if target.id() == document.entry_page_id() {
        format!("/f/{funnel_slug}")
    } else {
        format!("/f/{funnel_slug}/{}", target.slug())
    };
    Some(match form.result() {
        FormResult::Inline { .. } => format!("{page_route}#form-success-{}", form.id().as_str()),
        FormResult::SuccessPage { .. } => page_route,
    })
}

#[cfg(test)]
mod tests {
    use super::public_form_success_location;
    use eez_domain::document_v2::{DocumentV2, FormId};

    #[test]
    fn public_form_success_page_redirects_to_an_internal_page_route() {
        let mut value: serde_json::Value = serde_json::from_str(include_str!(
            "../../domain/tests/fixtures/document_v2_minimal.json"
        ))
        .expect("minimal V2 fixture");
        value["pages"][0]["sections"][0]["blocks"] = serde_json::json!([
            {"id":"block-form", "kind":"form", "form_id":"form-lead"}
        ]);
        value["pages"]
            .as_array_mut()
            .expect("pages")
            .push(serde_json::json!({
                "id":"page-thanks", "slug":"thanks", "title":"Thanks", "sections":[]
            }));
        value["forms"] = serde_json::json!([{
            "id":"form-lead",
            "fields":[{"id":"field-email", "kind":"email", "name":"email", "label":"Email", "required":true}],
            "purpose":"Contact request",
            "request_consent":{"copy_version":"request-v1", "copy":"I agree to be contacted."},
            "result":{"kind":"success_page", "page_id":"page-thanks"}
        }]);
        let data = serde_json::to_vec(&value).expect("document JSON");
        let document = DocumentV2::decode(&data, "funnel-01").expect("valid form document");
        let form_id = FormId::parse("form-lead").expect("form id");
        let form = document.form(&form_id).expect("form");

        assert_eq!(
            public_form_success_location(&document, &document.pages()[0], form, "lead-gen"),
            Some("/f/lead-gen/thanks".to_owned())
        );
    }
}
