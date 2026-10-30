use crate::FormSubmissionStatus;

pub const MAX_FOLLOW_UP_TEXT_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmissionFollowUp {
    status: FormSubmissionStatus,
    next_action: Option<String>,
    due_on: Option<String>,
    close_reason: Option<String>,
    corrects_version: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowUpValidationError {
    InvalidText,
    InvalidDate,
    CloseReasonRequiresClosedStatus,
    InvalidCorrectionTarget,
}

impl SubmissionFollowUp {
    pub fn new(
        status: FormSubmissionStatus,
        next_action: Option<String>,
        due_on: Option<String>,
        close_reason: Option<String>,
    ) -> Result<Self, FollowUpValidationError> {
        let next_action = normalize_optional_text(next_action)?;
        let close_reason = normalize_optional_text(close_reason)?;
        let due_on = normalize_due_on(due_on)?;
        if close_reason.is_some() && status != FormSubmissionStatus::Closed {
            return Err(FollowUpValidationError::CloseReasonRequiresClosedStatus);
        }
        Ok(Self {
            status,
            next_action,
            due_on,
            close_reason,
            corrects_version: None,
        })
    }

    pub fn with_correction_target(
        mut self,
        corrects_version: Option<i32>,
    ) -> Result<Self, FollowUpValidationError> {
        if corrects_version.is_some_and(|version| version <= 0) {
            return Err(FollowUpValidationError::InvalidCorrectionTarget);
        }
        self.corrects_version = corrects_version;
        Ok(self)
    }

    pub const fn status(&self) -> FormSubmissionStatus {
        self.status
    }

    pub fn next_action(&self) -> Option<&str> {
        self.next_action.as_deref()
    }

    pub fn due_on(&self) -> Option<&str> {
        self.due_on.as_deref()
    }

    pub fn close_reason(&self) -> Option<&str> {
        self.close_reason.as_deref()
    }

    pub const fn corrects_version(&self) -> Option<i32> {
        self.corrects_version
    }

    pub fn into_parts(
        self,
    ) -> (
        FormSubmissionStatus,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i32>,
    ) {
        (
            self.status,
            self.next_action,
            self.due_on,
            self.close_reason,
            self.corrects_version,
        )
    }
}

fn normalize_optional_text(
    value: Option<String>,
) -> Result<Option<String>, FollowUpValidationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > MAX_FOLLOW_UP_TEXT_BYTES
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(FollowUpValidationError::InvalidText);
    }
    Ok(Some(value.to_owned()))
}

fn normalize_due_on(value: Option<String>) -> Result<Option<String>, FollowUpValidationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() != 10 || value.as_bytes()[4] != b'-' || value.as_bytes()[7] != b'-' {
        return Err(FollowUpValidationError::InvalidDate);
    }
    let year = value[0..4]
        .parse::<u32>()
        .map_err(|_| FollowUpValidationError::InvalidDate)?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|_| FollowUpValidationError::InvalidDate)?;
    let day = value[8..10]
        .parse::<u32>()
        .map_err(|_| FollowUpValidationError::InvalidDate)?;
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return Err(FollowUpValidationError::InvalidDate),
    };
    if day == 0
        || day > days_in_month
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
    {
        return Err(FollowUpValidationError::InvalidDate);
    }
    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{FollowUpValidationError, SubmissionFollowUp};
    use crate::FormSubmissionStatus;

    #[test]
    fn normalizes_optional_text_and_accepts_valid_due_dates() {
        let follow_up = SubmissionFollowUp::new(
            FormSubmissionStatus::Contacted,
            Some("  Send the proposal  ".to_owned()),
            Some("2028-02-29".to_owned()),
            Some("  ".to_owned()),
        )
        .unwrap();
        assert_eq!(follow_up.next_action(), Some("Send the proposal"));
        assert_eq!(follow_up.due_on(), Some("2028-02-29"));
        assert_eq!(follow_up.close_reason(), None);
    }

    #[test]
    fn rejects_invalid_dates_and_close_reasons_on_open_statuses() {
        for due_on in ["2027-02-29", "2028-13-01", "2028-04-31", "28-02-29"] {
            assert_eq!(
                SubmissionFollowUp::new(
                    FormSubmissionStatus::New,
                    None,
                    Some(due_on.to_owned()),
                    None,
                ),
                Err(FollowUpValidationError::InvalidDate),
            );
        }
        assert_eq!(
            SubmissionFollowUp::new(
                FormSubmissionStatus::Qualified,
                None,
                None,
                Some("Not a fit".to_owned()),
            ),
            Err(FollowUpValidationError::CloseReasonRequiresClosedStatus),
        );
    }

    #[test]
    fn rejects_unbounded_or_controlled_text_and_preserves_multiline_notes() {
        let multiline = SubmissionFollowUp::new(
            FormSubmissionStatus::New,
            Some("first\nsecond".to_owned()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(multiline.next_action(), Some("first\nsecond"));
        for text in ["x".repeat(513), "first\u{0007}second".to_owned()] {
            assert_eq!(
                SubmissionFollowUp::new(FormSubmissionStatus::New, Some(text), None, None),
                Err(FollowUpValidationError::InvalidText),
            );
        }
    }

    #[test]
    fn correction_target_must_be_a_positive_event_version() {
        let follow_up =
            SubmissionFollowUp::new(FormSubmissionStatus::Contacted, None, None, None).unwrap();
        assert_eq!(
            follow_up.clone().with_correction_target(Some(0)),
            Err(FollowUpValidationError::InvalidCorrectionTarget),
        );
        assert_eq!(
            follow_up
                .with_correction_target(Some(2))
                .unwrap()
                .corrects_version(),
            Some(2),
        );
    }
}
