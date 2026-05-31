use chrono::{DateTime, Local, LocalResult, TimeZone, Utc};

const DISPLAY_DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M";

pub fn format_display_datetime(dt: &DateTime<Utc>) -> String {
    local_datetime(dt)
        .map(|local| local.format(DISPLAY_DATETIME_FORMAT).to_string())
        .unwrap_or_else(|| dt.format(DISPLAY_DATETIME_FORMAT).to_string())
}

pub fn local_datetime(dt: &DateTime<Utc>) -> Option<DateTime<Local>> {
    match Local.timestamp_opt(dt.timestamp(), dt.timestamp_subsec_nanos()) {
        LocalResult::Single(local) | LocalResult::Ambiguous(local, _) => Some(local),
        LocalResult::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn format_display_datetime_uses_local_timezone_when_available() {
        let dt = Utc.with_ymd_and_hms(2026, 5, 30, 1, 2, 0).unwrap();
        let expected = local_datetime(&dt)
            .map(|local| local.format(DISPLAY_DATETIME_FORMAT).to_string())
            .unwrap_or_else(|| dt.format(DISPLAY_DATETIME_FORMAT).to_string());

        assert_eq!(format_display_datetime(&dt), expected);
    }
}
