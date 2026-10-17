use super::{FormSubmissionListCommand, FormSubmissionStatusSetCommand};
use eez_domain::FormSubmissionStatus;

#[test]
fn status_set_command_accepts_only_canonical_ids_and_closed_statuses() {
    let submission_id = "11111111-1111-4111-8111-111111111111";
    let command = FormSubmissionStatusSetCommand::from_json(&format!(
        r#"{{"submission_id":"{submission_id}","status":"qualified"}}"#
    ))
    .expect("valid status command");
    assert_eq!(command.submission_id, submission_id);
    assert_eq!(command.status, FormSubmissionStatus::Qualified);

    for parameters in [
        r#"{"submission_id":"not-a-uuid","status":"new"}"#,
        r#"{"submission_id":"11111111-1111-4111-8111-111111111111","status":"pending"}"#,
        r#"{"submission_id":"11111111-1111-4111-8111-111111111111","status":"new","extra":true}"#,
    ] {
        assert!(
            FormSubmissionStatusSetCommand::from_json(parameters).is_err(),
            "{parameters}"
        );
    }
}

#[test]
fn submission_list_command_accepts_a_closed_status_filter() {
    let funnel_id = "11111111-1111-4111-8111-111111111111";
    let command = FormSubmissionListCommand::from_json(&format!(
        r#"{{"status":"contacted","funnel_id":"{funnel_id}"}}"#
    ))
    .expect("valid status filter");
    assert_eq!(command.status, Some(FormSubmissionStatus::Contacted));
    assert_eq!(command.funnel_id.as_deref(), Some(funnel_id));
    assert!(FormSubmissionListCommand::from_json(r#"{"status":"unknown"}"#).is_err());
    assert!(FormSubmissionListCommand::from_json(r#"{"funnel_id":"not-a-uuid"}"#).is_err());
    assert!(FormSubmissionListCommand::from_json(r#"{"funnel_id":""}"#).is_err());
    assert!(FormSubmissionListCommand::from_json(r#"{"unknown":true}"#).is_err());
}
