//! Recurrence engine: computes when reminders are due.
//!
//! Design principles:
//! - Pure, clock-free computation (a `now` is passed in) so everything
//!   is unit-testable and deterministic.
//! - Coalescing: if the machine slept for three hours, an hourly
//!   reminder is due *once* at wake, not three times.
//! - Timezone/DST aware via `chrono-tz`.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::model::{Reminder, RunState};

/// A recurrence definition. Serializable in a human-friendly shape:
///
/// ```json
/// {"type": "interval", "minutes": 60}
/// {"type": "interval", "seconds": 90}
/// {"type": "daily", "time": "09:00"}
/// {"type": "daily", "time": "18:00", "days": ["mon","wed","fri"]}
/// {"type": "once", "at": "2026-09-22T19:00:00-04:00"}
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Schedule {
    Interval(Interval),
    Daily(Daily),
    Once(Once),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    /// Interval length in seconds.
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Daily {
    /// "HH:MM" local time.
    pub time: String,
    /// Weekdays the reminder fires ("mon".."sun"). Empty = every day.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub days: Vec<String>,
    /// Inclusive start date "YYYY-MM-DD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// Inclusive end date "YYYY-MM-DD".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Once {
    /// RFC3339 timestamp.
    pub at: String,
}

impl Schedule {
    pub fn interval(seconds: u64) -> Schedule {
        Schedule::Interval(Interval { seconds })
    }

    /// JSON form; intervals serialize as whole minutes when possible,
    /// matching the documented format (`{"type":"interval","minutes":60}`).
    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Schedule::Interval(i) => {
                if i.seconds >= 60 && i.seconds % 60 == 0 {
                    serde_json::json!({"type": "interval", "minutes": i.seconds / 60})
                } else {
                    serde_json::json!({"type": "interval", "seconds": i.seconds})
                }
            }
            Schedule::Daily(d) => {
                let mut v = serde_json::json!({"type": "daily", "time": d.time});
                if !d.days.is_empty() {
                    v["days"] = serde_json::json!(d.days);
                }
                if let Some(s) = &d.start_date {
                    v["start_date"] = serde_json::json!(s);
                }
                if let Some(e) = &d.end_date {
                    v["end_date"] = serde_json::json!(e);
                }
                v
            }
            Schedule::Once(o) => serde_json::json!({"type": "once", "at": o.at}),
        }
    }
}

impl Serialize for Schedule {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json_value().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Schedule {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "lowercase")]
        enum Raw {
            Interval {
                #[serde(default)]
                minutes: Option<u64>,
                #[serde(default)]
                seconds: Option<u64>,
                #[serde(default)]
                hours: Option<u64>,
            },
            Daily {
                time: String,
                #[serde(default)]
                days: Vec<String>,
                #[serde(default)]
                start_date: Option<String>,
                #[serde(default)]
                end_date: Option<String>,
            },
            Once { at: String },
        }

        match Raw::deserialize(d)? {
            Raw::Interval {
                minutes,
                seconds,
                hours,
            } => {
                let secs = hours.unwrap_or(0) * 3600 + minutes.unwrap_or(0) * 60 + seconds.unwrap_or(0);
                if secs == 0 {
                    return Err(serde::de::Error::custom("interval must be > 0"));
                }
                Ok(Schedule::Interval(Interval { seconds: secs }))
            }
            Raw::Daily {
                time,
                days,
                start_date,
                end_date,
            } => Ok(Schedule::Daily(Daily {
                time,
                days,
                start_date,
                end_date,
            })),
            Raw::Once { at } => Ok(Schedule::Once(Once { at })),
        }
    }
}

/// Compute the next due time for a reminder, strictly after `anchor`
/// (usually `last_fired` or `created`).
///
/// Returns `None` for fired one-time reminders or schedules past their
/// `end_date`.
pub fn next_due(
    reminder: &Reminder,
    state: &RunState,
    now_utc: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let tz = reminder.tz();
    let now = now_utc.with_timezone(&tz);

    let created = reminder.created.with_timezone(&tz);
    let anchor_utc = state.last_fired.get(&reminder.id).copied();
    let mut anchor = anchor_utc.map(|t| t.with_timezone(&tz)).unwrap_or(created);

    // System clock moved backwards: clamp so we never wait a huge time.
    if anchor > now {
        anchor = now;
    }

    match &reminder.schedule {
        Schedule::Interval(iv) => Some(
            (anchor + Duration::seconds(iv.seconds as i64)).with_timezone(&Utc),
        ),
        Schedule::Daily(d) => next_daily_after(d, tz, anchor).map(|t| t.with_timezone(&Utc)),
        Schedule::Once(o) => {
            if anchor_utc.is_some() || reminder.completed {
                return None; // already fired
            }
            let at = crate::util::parse_rfc3339_loose(&o.at)?;
            Some(at.with_timezone(&Utc))
        }
    }
}

/// Is the reminder due right now?
pub fn is_due(reminder: &Reminder, state: &RunState, now_utc: DateTime<Utc>) -> bool {
    if !reminder.enabled || reminder.completed {
        return false;
    }
    match next_due(reminder, state, now_utc) {
        Some(next) => next <= now_utc,
        None => false,
    }
}

/// The soonest upcoming due time across enabled reminders (strictly future).
pub fn next_upcoming<'a>(
    reminders: impl IntoIterator<Item = &'a Reminder>,
    state: &RunState,
    now_utc: DateTime<Utc>,
) -> Option<(&'a Reminder, DateTime<Utc>)> {
    reminders
        .into_iter()
        .filter(|r| r.enabled && !r.completed)
        .filter_map(|r| next_due(r, state, now_utc).map(|t| (r, t)))
        .filter(|(_, t)| *t > now_utc)
        .min_by_key(|(_, t)| *t)
}

fn next_daily_after(d: &Daily, tz: Tz, after: DateTime<Tz>) -> Option<DateTime<Tz>> {
    let (hh, mm) = crate::util::parse_hhmm(&d.time)?;
    let days = crate::util::parse_day_list(&d.days).unwrap_or_default();
    let start = d.start_date.as_deref().and_then(parse_date);
    let end = d.end_date.as_deref().and_then(parse_date);

    let mut date = after.date_naive();

    for _ in 0..400 {
        if let Some(s) = start {
            if date < s {
                date += Duration::days(1);
                continue;
            }
        }
        if let Some(e) = end {
            if date > e {
                return None;
            }
        }
        if days.is_empty() || days.contains(&date.weekday()) {
            let naive = date.and_hms_opt(hh, mm, 0)?;
            let candidate = match resolve_local(tz, naive) {
                Some(c) => c,
                // DST gap (e.g. 02:30 on spring-forward day): fire an hour later.
                None => match resolve_local(tz, naive + Duration::hours(1)) {
                    Some(c) => c,
                    None => {
                        date += Duration::days(1);
                        continue;
                    }
                },
            };
            if candidate > after {
                return Some(candidate);
            }
        }
        date += Duration::days(1);
    }
    None
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Resolve a naive local datetime in `tz`; on ambiguity (fall-back) pick the
/// earliest occurrence; on gap return None.
fn resolve_local(tz: Tz, naive: chrono::NaiveDateTime) -> Option<DateTime<Tz>> {
    use chrono::LocalResult;
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(earliest, _) => Some(earliest),
        LocalResult::None => None,
    }
}

/// Human-readable schedule text, e.g. "Every 1 hour", "Daily 7:00 PM".
pub fn humanize(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Interval(iv) => format!("Every {}", crate::util::humanize_seconds(iv.seconds)),
        Schedule::Daily(d) => {
            let time = crate::util::humanize_time(&d.time).unwrap_or_else(|| d.time.clone());
            if d.days.is_empty() {
                format!("Daily {time}")
            } else {
                let days = d
                    .days
                    .iter()
                    .map(|s| title_case(s))
                    .collect::<Vec<_>>()
                    .join("/");
                format!("{days} {time}")
            }
        }
        Schedule::Once(o) => match crate::util::parse_rfc3339_loose(&o.at) {
            Some(dt) => dt.format("%b %d %Y %I:%M %p").to_string(),
            None => o.at.clone(),
        },
    }
}

pub(crate) fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
