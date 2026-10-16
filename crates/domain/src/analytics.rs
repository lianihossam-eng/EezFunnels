#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormAnalyticsPeriod {
    SevenDays,
    ThirtyDays,
    NinetyDays,
}

impl FormAnalyticsPeriod {
    pub const fn days(self) -> u8 {
        match self {
            Self::SevenDays => 7,
            Self::ThirtyDays => 30,
            Self::NinetyDays => 90,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FormAnalyticsPeriod;

    #[test]
    fn form_analytics_periods_are_bounded_calendar_windows() {
        assert_eq!(FormAnalyticsPeriod::SevenDays.days(), 7);
        assert_eq!(FormAnalyticsPeriod::ThirtyDays.days(), 30);
        assert_eq!(FormAnalyticsPeriod::NinetyDays.days(), 90);
    }
}
