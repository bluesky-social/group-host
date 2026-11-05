//! Times as atproto writes them: ISO 8601 in UTC, to the millisecond.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since 1970.
pub fn now_millis() -> i64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH);
    since.expect("after 1970").as_millis() as i64
}

/// `millis` since 1970 as `2026-11-03T21:14:28.123Z`.
pub fn iso(millis: i64) -> String {
    let (days, ms) = (millis.div_euclid(86_400_000), millis.rem_euclid(86_400_000));
    // Howard Hinnant's days_to_civil, for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s, f) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{f:03}Z")
}
