use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn rfc3339_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn parse_rfc3339(ts: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(ts, &Rfc3339).ok()
}

pub fn is_valid_rfc3339(ts: &str) -> bool {
    parse_rfc3339(ts).is_some()
}

pub fn rfc3339_timestamp_secs(ts: &str) -> Option<f64> {
    let parsed = parse_rfc3339(ts)?;
    Some(parsed.unix_timestamp_nanos() as f64 / 1_000_000_000.0)
}

pub fn rfc3339_diff_secs(start: &str, end: &str) -> Option<f64> {
    let start_secs = rfc3339_timestamp_secs(start)?;
    let end_secs = rfc3339_timestamp_secs(end)?;
    Some(end_secs - start_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utc_timestamp() {
        let secs = rfc3339_timestamp_secs("2026-03-17T10:00:30Z").unwrap();
        assert!(secs > 0.0);
    }

    #[test]
    fn parses_offset_timestamp() {
        let diff =
            rfc3339_diff_secs("2026-03-17T10:00:00+01:00", "2026-03-17T10:00:30+01:00").unwrap();
        assert!((diff - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_invalid_timestamp() {
        assert!(parse_rfc3339("2026-03-17 10:00:00").is_none());
        assert!(!is_valid_rfc3339("not-a-timestamp"));
    }
}
