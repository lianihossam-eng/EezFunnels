use eez_domain::SubmissionFollowUp;
use eez_storage::submission_follow_up::{
    SubmissionFollowUpHistory, SubmissionFollowUpMutationReceipt, SubmissionFollowUpRepository,
    SubmissionFollowUpStorageError,
};
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum SubmissionFollowUpCommandError {
    Storage(SubmissionFollowUpStorageError),
}

impl SubmissionFollowUpCommandError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Storage(error) => error.code(),
        }
    }
}

impl Display for SubmissionFollowUpCommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for SubmissionFollowUpCommandError {}

impl From<SubmissionFollowUpStorageError> for SubmissionFollowUpCommandError {
    fn from(error: SubmissionFollowUpStorageError) -> Self {
        Self::Storage(error)
    }
}

pub async fn read_submission_follow_up(
    repository: &SubmissionFollowUpRepository<'_>,
    submission_id: &str,
) -> Result<SubmissionFollowUpHistory, SubmissionFollowUpCommandError> {
    Ok(repository.read(submission_id).await?)
}

pub async fn update_submission_follow_up(
    repository: &SubmissionFollowUpRepository<'_>,
    submission_id: &str,
    expected_version: i32,
    idempotency_key: &str,
    follow_up: &SubmissionFollowUp,
) -> Result<SubmissionFollowUpMutationReceipt, SubmissionFollowUpCommandError> {
    Ok(repository
        .update_owner_follow_up(submission_id, expected_version, idempotency_key, follow_up)
        .await?)
}
