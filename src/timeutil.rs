//! Calendar helpers on unix seconds + a fixed local UTC offset. Platform independent.

pub const DAY: i64 = 86_400;

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Days since 1970-01-01 -> (year, month 1-12, day 1-31). (H. Hinnant's algorithm)
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// (year, month, day) -> days since 1970-01-01.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Local day number of a unix timestamp.
pub fn local_day(ts: i64, offset: i64) -> i64 {
    (ts + offset).div_euclid(DAY)
}

/// Unix timestamp of local midnight starting `day`.
pub fn day_start(day: i64, offset: i64) -> i64 {
    day * DAY - offset
}

pub fn date_string(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// "YYYY-MM-DD HH:MM:SS" local time (for the debug log).
pub fn datetime_string(ts: i64, offset: i64) -> String {
    let local = ts + offset;
    let secs = local.rem_euclid(DAY);
    format!(
        "{} {:02}:{:02}:{:02}",
        date_string(local.div_euclid(DAY)),
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// Local [first_day, last_day_exclusive) for "today" | "week" (Mon-Sun) | "month".
pub fn range_days(range: &str, now: i64, offset: i64) -> (i64, i64) {
    let today = local_day(now, offset);
    match range {
        "week" => {
            let monday = today - (today + 3).rem_euclid(7); // 1970-01-01 was a Thursday
            (monday, monday + 7)
        }
        "month" => {
            let (y, m, _) = civil_from_days(today);
            let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            (days_from_civil(y, m, 1), days_from_civil(ny, nm, 1))
        }
        _ => (today, today + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2026, 11, 12), 20_769);
        for d in [-1000, 0, 11_016, 20_769, 30_000] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
    }

    #[test]
    fn ranges() {
        // 2026-11-12 is a Thursday. 10:00 UTC, local offset +1h.
        let now = day_start(20_769, 0) + 10 * 3600;
        assert_eq!(range_days("today", now, 3600), (20_769, 20_770));
        let (mon, sun_excl) = range_days("week", now, 3600);
        assert_eq!(date_string(mon), "2026-11-09");
        assert_eq!(sun_excl - mon, 7);
        let (first, next) = range_days("month", now, 3600);
        assert_eq!(date_string(first), "2026-11-01");
        assert_eq!(date_string(next), "2026-12-01");
        // 23:30 UTC with +1h offset is already the next local day.
        assert_eq!(
            local_day(day_start(20_769, 0) + 23 * 3600 + 1800, 3600),
            20_770
        );
    }
}
