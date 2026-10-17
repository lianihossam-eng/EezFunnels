use serde::{Deserialize, Serialize};

/// A small owner-managed follow-up state for a retained form submission.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormSubmissionStatus {
    New,
    Contacted,
    Qualified,
    Closed,
}

impl FormSubmissionStatus {
    pub const ALL: [Self; 4] = [Self::New, Self::Contacted, Self::Qualified, Self::Closed];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Contacted => "contacted",
            Self::Qualified => "qualified",
            Self::Closed => "closed",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Contacted => "Contacted",
            Self::Qualified => "Qualified",
            Self::Closed => "Closed",
        }
    }

    pub fn from_wire_value(value: &str) -> Option<Self> {
        match value {
            "new" => Some(Self::New),
            "contacted" => Some(Self::Contacted),
            "qualified" => Some(Self::Qualified),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FormSubmissionStatus;

    #[test]
    fn wire_values_are_exact_and_cover_the_closed_status_set() {
        for status in FormSubmissionStatus::ALL {
            assert_eq!(
                FormSubmissionStatus::from_wire_value(status.as_str()),
                Some(status)
            );
        }
        assert_eq!(FormSubmissionStatus::from_wire_value("NEW"), None);
        assert_eq!(FormSubmissionStatus::from_wire_value("pending"), None);
        assert_eq!(FormSubmissionStatus::from_wire_value(""), None);
    }
}
