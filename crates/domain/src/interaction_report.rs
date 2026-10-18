use serde::{Deserialize, Serialize};

/// The fixed UTC calendar windows supported by the interaction report.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionReportPeriod {
    Days7,
    Days30,
    Days90,
}

impl InteractionReportPeriod {
    pub const fn from_days(days: u8) -> Option<Self> {
        match days {
            7 => Some(Self::Days7),
            30 => Some(Self::Days30),
            90 => Some(Self::Days90),
            _ => None,
        }
    }

    pub const fn days(self) -> u8 {
        match self {
            Self::Days7 => 7,
            Self::Days30 => 30,
            Self::Days90 => 90,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InteractionReportPeriod;

    #[test]
    fn interaction_report_period_accepts_only_supported_calendar_windows() {
        assert_eq!(
            InteractionReportPeriod::from_days(7),
            Some(InteractionReportPeriod::Days7)
        );
        assert_eq!(
            InteractionReportPeriod::from_days(30),
            Some(InteractionReportPeriod::Days30)
        );
        assert_eq!(
            InteractionReportPeriod::from_days(90),
            Some(InteractionReportPeriod::Days90)
        );
        for days in [0, 1, 6, 8, 29, 31, 89, 91, u8::MAX] {
            assert_eq!(InteractionReportPeriod::from_days(days), None);
        }
    }
}
