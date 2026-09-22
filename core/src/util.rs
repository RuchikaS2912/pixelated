//! Parsing and formatting helpers shared by CLI, daemon and tests.

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

/// Parse a duration like `1h`, `45m`, `30s`, `1h30m`, or a bare number
/// (interpreted as minutes).
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        // Bare number = minutes
        return s
            .parse::<u64>()
            .ok()
            .and_then(|m| m.checked_mul(60))
            .filter(|s| *s > 0);
    }
    let mut total: u64 = 0;
    let mut num = String::new();
    let mut matched_any = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            let v: u64 = num.parse().ok()?;
            num.clear();
            let mult = match c {
                'h' => 3600,
                'm' => 60,
                's' => 1,
                _ => return None,
            };
            total += v * mult;
            matched_any = true;
        }
    }
    if !num.is_empty() {
        return None; // trailing digits without a unit
    }
    if !matched_any || total == 0 {
        return None;
    }
    Some(total)
}

/// Parse "HH:MM", "HH:MM:SS", "7:00pm", "7 pm" into (hour, minute).
pub fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().to_lowercase().replace(' ', "");
    let (time_part, ampm) = if let Some(rest) = s.strip_suffix("am") {
        (rest.to_string(), 'a')
    } else if let Some(rest) = s.strip_suffix("pm") {
        (rest.to_string(), 'p')
    } else if s.ends_with('a') || s.ends_with('p') {
        let (rest, suffix) = s.split_at(s.len() - 1);
        (rest.to_string(), suffix.chars().next().unwrap())
    } else {
        (s.clone(), ' ')
    };
    let mut it = time_part.split(':');
    let h: u32 = it.next()?.parse().ok()?;
    let m: u32 = match it.next() {
        Some(m) => m.parse().ok()?,
        None => 0,
    };
    if it.next().is_some_and(|s| !s.is_empty()) {
        return None;
    }
    if h > 23 || m > 59 {
        return None;
    }
    let h = match ampm {
        'a' => {
            if h == 12 {
                0
            } else {
                h
            }
        }
        'p' => {
            if h == 12 {
                12
            } else {
                h + 12
            }
        }
        _ => h,
    };
    Some((h, m))
}

/// Normalize any accepted time form to "HH:MM" (24h).
pub fn normalize_time(s: &str) -> Option<String> {
    let (h, m) = parse_hhmm(s)?;
    Some(format!("{h:02}:{m:02}"))
}

/// Parse a date "YYYY-MM-DD".
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

/// Parse a datetime: RFC3339, "YYYY-MM-DD HH:MM", or "YYYY-MM-DD".
/// Returns timezone-aware UTC datetime (naive input is interpreted in `tz`).
pub fn parse_datetime(s: &str, tz: Tz) -> Option<DateTime<Utc>> {
    let s = s.trim();
    // RFC3339 with offset
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // "YYYY-MM-DD HH:MM[:SS]" possibly with am/pm
    let (date_str, time_str) = match s.split_once(' ') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let date = parse_date(date_str)?;
    let time = match time_str {
        Some(t) => {
            let (h, m) = parse_hhmm(t)?;
            NaiveTime::from_hms_opt(h, m, 0)?
        }
        None => NaiveTime::from_hms_opt(0, 0, 0)?,
    };
    let naive = date.and_time(time);
    Some(tz.from_local_datetime(&naive).single()?.with_timezone(&Utc))
}

/// Lenient RFC3339 parse (also accepts trailing 'Z' or explicit offset).
pub fn parse_rfc3339_loose(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Parse weekday names: mon/tue/wed/thu/fri/sat/sun, full names, or 1-7
/// (1=mon). Returns chrono weekdays in the given order.
pub fn parse_days(spec: &str) -> Option<Vec<chrono::Weekday>> {
    if spec.trim().is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for part in spec.split(',') {
        let p = part.trim().to_lowercase();
        let wd = match p.as_str() {
            "mon" | "monday" | "1" => Some(chrono::Weekday::Mon),
            "tue" | "tues" | "tuesday" | "2" => Some(chrono::Weekday::Tue),
            "wed" | "wednesday" | "3" => Some(chrono::Weekday::Wed),
            "thu" | "thurs" | "thursday" | "4" => Some(chrono::Weekday::Thu),
            "fri" | "friday" | "5" => Some(chrono::Weekday::Fri),
            "sat" | "saturday" | "6" => Some(chrono::Weekday::Sat),
            "sun" | "sunday" | "7" => Some(chrono::Weekday::Sun),
            _ => None,
        }?;
        out.push(wd);
    }
    Some(out)
}

/// Parse a list of weekday strings (from JSON) to weekdays.
pub fn parse_day_list(days: &[String]) -> Option<Vec<chrono::Weekday>> {
    parse_days(&days.join(","))
}

/// The system IANA timezone (UTC fallback).
pub fn system_timezone() -> Tz {
    std::env::var("TZ")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(iana_time_zone_fallback)
}

fn iana_time_zone_fallback() -> Tz {
    // Read /etc/localtime symlink target on unix without extra deps.
    #[cfg(target_os = "macos")]
    {
        if let Ok(link) = std::fs::read_link("/etc/localtime") {
            let s = link.to_string_lossy();
            if let Some(idx) = s.find("zoneinfo/") {
                let name = &s[idx + "zoneinfo/".len()..];
                if let Ok(tz) = name.parse::<Tz>() {
                    return tz;
                }
            }
        }
    }
    Tz::UTC
}

/// "42m", "1h 21m", "2d 3h" style short relative duration.
pub fn humanize_duration_short(secs: i64) -> String {
    if secs < 0 {
        return "now".into();
    }
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

/// "1 hour", "45 minutes", "30 seconds", "1 hour 30 minutes".
pub fn humanize_seconds(total: u64) -> String {
    if total < 120 {
        return format!("{total} seconds");
    }
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{h} hour{}", plural(h)));
    }
    if m > 0 {
        parts.push(format!("{m} minute{}", plural(m)));
    }
    if s > 0 || parts.is_empty() {
        parts.push(format!("{s} second{}", plural(s)));
    }
    parts.join(" ")
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// "19:00" -> "7:00 PM"
pub fn humanize_time(hhmm: &str) -> Option<String> {
    let (h, m) = parse_hhmm(hhmm)?;
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    Some(format!("{h12}:{m:02} {ampm}"))
}

/// Generate a slug from a title (for logs/filenames only; IDs are integers).
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("45m"), Some(2700));
        assert_eq!(parse_duration("30s"), Some(30));
        assert_eq!(parse_duration("1h30m"), Some(5400));
        assert_eq!(parse_duration("90"), Some(5400));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("0"), None);
    }

    #[test]
    fn times() {
        assert_eq!(parse_hhmm("19:00"), Some((19, 0)));
        assert_eq!(parse_hhmm("7:00pm"), Some((19, 0)));
        assert_eq!(parse_hhmm("12:30 AM"), Some((0, 30)));
        assert_eq!(parse_hhmm("12:30pm"), Some((12, 30)));
        assert_eq!(parse_hhmm("25:00"), None);
        assert_eq!(normalize_time("7:00pm"), Some("19:00".into()));
    }

    #[test]
    fn humanize_time_pm() {
        assert_eq!(humanize_time("19:00").as_deref(), Some("7:00 PM"));
        assert_eq!(humanize_time("09:05").as_deref(), Some("9:05 AM"));
    }

    #[test]
    fn day_parsing() {
        let days = parse_days("mon,wed,fri").unwrap();
        assert_eq!(
            days,
            vec![
                chrono::Weekday::Mon,
                chrono::Weekday::Wed,
                chrono::Weekday::Fri
            ]
        );
        assert!(parse_days("monday,sunday").is_some());
        assert!(parse_days("funday").is_none());
    }

    #[test]
    fn durations_human() {
        assert_eq!(humanize_seconds(3600), "1 hour");
        assert_eq!(humanize_seconds(7200), "2 hours");
        assert_eq!(humanize_seconds(45 * 60), "45 minutes");
        assert_eq!(humanize_seconds(90), "90 seconds");
        assert_eq!(humanize_seconds(5400), "1 hour 30 minutes");
        assert_eq!(humanize_duration_short(2530), "42m");
        assert_eq!(humanize_duration_short(4860), "1h 21m");
    }

    #[test]
    fn datetime_parsing() {
        let tz: Tz = "America/New_York".parse().unwrap();
        let dt = parse_datetime("2026-09-22 19:00", tz).unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-09-22T23:00:00+00:00");
        let dt2 = parse_datetime("2026-09-22T19:00:00Z", tz).unwrap();
        assert_eq!(dt2.to_rfc3339(), "2026-09-22T19:00:00+00:00");
    }
}
