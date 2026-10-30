use std::collections::BTreeMap;

use eez_storage::submission_lifecycle::{
    EraseResult, MaintenanceProgress, RecoveryStatus, SubmissionExportRecord,
    SubmissionLifecycleRepository,
};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SubmissionExportV1 {
    format: &'static str,
    version: u8,
    submission: ExportedSubmission,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ExportedSubmission {
    id: String,
    funnel_id: String,
    revision_id: String,
    page_id: String,
    form_id: String,
    channel: String,
    status: String,
    values: BTreeMap<String, Value>,
    submitted_at: i64,
    expires_at: i64,
    request_consent: RequestConsentEvidence,
    marketing_consent: Option<MarketingConsentEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct RequestConsentEvidence {
    acknowledged: bool,
    copy_version: String,
    copy: String,
    recorded_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct MarketingConsentEvidence {
    accepted: bool,
    copy_version: String,
    copy: String,
    recorded_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmissionLifecycleError {
    NotFound,
    Unavailable,
    Conflict,
    InvalidInput,
}

pub async fn export_one(
    repository: &SubmissionLifecycleRepository<'_>,
    submission_id: &str,
) -> Result<SubmissionExportV1, SubmissionLifecycleError> {
    let record = repository
        .retained_export(submission_id)
        .await
        .map_err(map_storage_error)?
        .ok_or(SubmissionLifecycleError::NotFound)?;
    submission_export(record)
}

pub async fn erase_one(
    repository: &SubmissionLifecycleRepository<'_>,
    submission_id: &str,
    recovery_horizon_days: u16,
) -> Result<EraseResult, SubmissionLifecycleError> {
    let Some(journal) = repository
        .begin_erasure(submission_id, recovery_horizon_days)
        .await
        .map_err(map_storage_error)?
    else {
        return Err(SubmissionLifecycleError::NotFound);
    };
    repository
        .ensure_journal(&journal)
        .await
        .map_err(map_storage_error)?;
    repository
        .finish_erasure(&journal)
        .await
        .map_err(map_storage_error)?;
    Ok(EraseResult::Erased)
}

pub async fn begin_recovery(
    repository: &SubmissionLifecycleRepository<'_>,
) -> Result<RecoveryStatus, SubmissionLifecycleError> {
    repository.begin_recovery().await.map_err(map_storage_error)
}

pub async fn prepare_recovery_step(
    repository: &SubmissionLifecycleRepository<'_>,
    generation: &str,
) -> Result<(RecoveryStatus, u32, u64), SubmissionLifecycleError> {
    let (status, progress) = repository
        .prepare_recovery_step(generation)
        .await
        .map_err(map_storage_error)?;
    Ok((status, progress.attempted, progress.pending))
}

pub async fn reconcile_after_restore(
    repository: &SubmissionLifecycleRepository<'_>,
    generation: &str,
) -> Result<(RecoveryStatus, u32), SubmissionLifecycleError> {
    repository
        .reconcile_after_restore(generation)
        .await
        .map_err(map_storage_error)
}

pub async fn maintain_pending(
    repository: &SubmissionLifecycleRepository<'_>,
    budget: u32,
) -> Result<MaintenanceProgress, SubmissionLifecycleError> {
    repository
        .maintain_pending(budget)
        .await
        .map_err(map_storage_error)
}

fn submission_export(
    record: SubmissionExportRecord,
) -> Result<SubmissionExportV1, SubmissionLifecycleError> {
    if record.request_consent_acknowledged != 1
        || !matches!(record.marketing_opt_in, 0 | 1)
        || record.marketing_opt_in_copy_version.is_some() != record.marketing_opt_in_copy.is_some()
        || record.marketing_opt_in_copy_version.is_some()
            != record.marketing_opt_in_recorded_at.is_some()
    {
        return Err(SubmissionLifecycleError::Unavailable);
    }
    let values = serde_json::from_str::<BTreeMap<String, Value>>(&record.field_values_json)
        .map_err(|_| SubmissionLifecycleError::Unavailable)?;
    if values
        .values()
        .any(|value| !matches!(value, Value::Bool(_) | Value::String(_)))
    {
        return Err(SubmissionLifecycleError::Unavailable);
    }
    let marketing_consent = match (
        record.marketing_opt_in_copy_version,
        record.marketing_opt_in_copy,
        record.marketing_opt_in_recorded_at,
    ) {
        (Some(copy_version), Some(copy), Some(recorded_at)) => Some(MarketingConsentEvidence {
            accepted: record.marketing_opt_in == 1,
            copy_version,
            copy,
            recorded_at,
        }),
        (None, None, None) => None,
        _ => return Err(SubmissionLifecycleError::Unavailable),
    };
    Ok(SubmissionExportV1 {
        format: "eezfunnel.submission",
        version: 1,
        submission: ExportedSubmission {
            id: record.submission_id,
            funnel_id: record.funnel_id,
            revision_id: record.revision_id,
            page_id: record.page_id,
            form_id: record.form_id,
            channel: record.submission_channel,
            status: record.triage_status,
            values,
            submitted_at: record.submitted_at,
            expires_at: record.expires_at,
            request_consent: RequestConsentEvidence {
                acknowledged: true,
                copy_version: record.request_consent_copy_version,
                copy: record.request_consent_copy,
                recorded_at: record.request_consent_recorded_at,
            },
            marketing_consent,
        },
    })
}

fn map_storage_error(
    error: eez_storage::submission_lifecycle::LifecycleError,
) -> SubmissionLifecycleError {
    use eez_storage::submission_lifecycle::LifecycleError;
    match error {
        LifecycleError::Conflict => SubmissionLifecycleError::Conflict,
        LifecycleError::InvalidInput => SubmissionLifecycleError::InvalidInput,
        LifecycleError::Database(_) | LifecycleError::Unavailable => {
            SubmissionLifecycleError::Unavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{submission_export, SubmissionLifecycleError};
    use eez_storage::submission_lifecycle::SubmissionExportRecord;

    fn record() -> SubmissionExportRecord {
        SubmissionExportRecord {
            submission_id: "11111111-1111-4111-8111-111111111111".to_owned(),
            funnel_id: "22222222-2222-4222-8222-222222222222".to_owned(),
            revision_id: "33333333-3333-4333-8333-333333333333".to_owned(),
            page_id: "page-home".to_owned(),
            form_id: "form-contact".to_owned(),
            submission_channel: "unattributed".to_owned(),
            triage_status: "new".to_owned(),
            field_values_json: r#"{"email":"person@example.test","name":"Example"}"#.to_owned(),
            request_consent_acknowledged: 1,
            request_consent_copy_version: "v1".to_owned(),
            request_consent_copy: "I agree to be contacted about this request.".to_owned(),
            request_consent_recorded_at: 1_800_000_000,
            marketing_opt_in: 0,
            marketing_opt_in_copy_version: Some("m1".to_owned()),
            marketing_opt_in_copy: Some("Optional marketing updates.".to_owned()),
            marketing_opt_in_recorded_at: Some(1_800_000_000),
            submitted_at: 1_800_000_000,
            expires_at: 1_807_776_000,
        }
    }

    #[test]
    fn export_has_an_allowlisted_versioned_shape_and_keeps_consent_evidence() {
        let exported = submission_export(record()).unwrap();
        let json = serde_json::to_value(exported).unwrap();
        assert_eq!(json["format"], "eezfunnel.submission");
        assert_eq!(json["version"], 1);
        assert_eq!(json["submission"]["values"]["email"], "person@example.test");
        assert_eq!(json["submission"]["request_consent"]["acknowledged"], true);
        assert_eq!(json["submission"]["marketing_consent"]["accepted"], false);
        assert!(json["submission"].get("submission_key").is_none());
        assert!(json["submission"].get("request_fingerprint").is_none());
    }

    #[test]
    fn export_rejects_invalid_values_or_incomplete_consent_evidence() {
        let mut invalid = record();
        invalid.field_values_json = "[]".to_owned();
        assert_eq!(
            submission_export(invalid),
            Err(SubmissionLifecycleError::Unavailable)
        );
        let mut invalid = record();
        invalid.marketing_opt_in_copy = None;
        assert_eq!(
            submission_export(invalid),
            Err(SubmissionLifecycleError::Unavailable)
        );
    }
}
