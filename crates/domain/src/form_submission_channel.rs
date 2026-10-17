use serde::{Deserialize, Serialize};

/// A low-cardinality campaign category reported by the page that hosts a form.
///
/// This value is intentionally not a verified identity or cross-page attribution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormSubmissionChannel {
    Unattributed,
    Search,
    Social,
    Email,
    Paid,
    Partner,
    Other,
}

impl FormSubmissionChannel {
    pub const ALL: [Self; 7] = [
        Self::Unattributed,
        Self::Search,
        Self::Social,
        Self::Email,
        Self::Paid,
        Self::Partner,
        Self::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unattributed => "unattributed",
            Self::Search => "search",
            Self::Social => "social",
            Self::Email => "email",
            Self::Paid => "paid",
            Self::Partner => "partner",
            Self::Other => "other",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unattributed => "Unattributed",
            Self::Search => "Search",
            Self::Social => "Social",
            Self::Email => "Email",
            Self::Paid => "Paid",
            Self::Partner => "Partner",
            Self::Other => "Other",
        }
    }

    pub fn from_wire_value(value: &str) -> Option<Self> {
        match value {
            "unattributed" => Some(Self::Unattributed),
            "search" => Some(Self::Search),
            "social" => Some(Self::Social),
            "email" => Some(Self::Email),
            "paid" => Some(Self::Paid),
            "partner" => Some(Self::Partner),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// Maps the page's `utm_medium` value to a bounded category.
    ///
    /// The original value is never retained. Unknown values collapse into `Other`.
    pub fn from_utm_medium(value: &str) -> Self {
        if value.len() > 64 || !value.is_ascii() {
            return Self::Other;
        }
        match value.trim().to_ascii_lowercase().as_str() {
            "organic" | "search" | "cpc" | "ppc" | "paid_search" | "sem" => Self::Search,
            "social" | "social-media" | "social_media" | "paid_social" => Self::Social,
            "email" | "e-mail" | "newsletter" => Self::Email,
            "paid" | "display" | "cpm" | "paid_display" | "retargeting" => Self::Paid,
            "affiliate" | "partner" => Self::Partner,
            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FormSubmissionChannel;

    #[test]
    fn utm_medium_maps_to_a_closed_category_set() {
        for (medium, expected) in [
            ("organic", FormSubmissionChannel::Search),
            ("CPC", FormSubmissionChannel::Search),
            ("social-media", FormSubmissionChannel::Social),
            ("newsletter", FormSubmissionChannel::Email),
            ("display", FormSubmissionChannel::Paid),
            ("affiliate", FormSubmissionChannel::Partner),
            ("unmapped-medium-fixture", FormSubmissionChannel::Other),
            ("", FormSubmissionChannel::Other),
            (&"x".repeat(65), FormSubmissionChannel::Other),
        ] {
            assert_eq!(FormSubmissionChannel::from_utm_medium(medium), expected);
        }
    }

    #[test]
    fn wire_values_are_exact_and_complete() {
        for channel in FormSubmissionChannel::ALL {
            assert_eq!(
                FormSubmissionChannel::from_wire_value(channel.as_str()),
                Some(channel)
            );
        }
        assert_eq!(FormSubmissionChannel::from_wire_value("SEARCH"), None);
        assert_eq!(FormSubmissionChannel::from_wire_value("raw-utm"), None);
    }
}
