//! Scheduling correctness tests (requirement: hourly, daily, weekly,
//! custom, disabled, missed/sleep coalescing, DST, clock changes).

use chrono::{Duration, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use wremind_core::model::{Reminder, RunState};
use wremind_core::recurrence::{is_due, next_due, next_upcoming, Daily, Interval, Once, Schedule};

fn mk(schedule: Schedule, created_minutes_ago: i64) -> Reminder {
    Reminder {
        id: 1,
        title: "Test".into(),
        emoji: None,
        schedule,
        character: None,
        timezone: "UTC".into(),
        enabled: true,
        completed: false,
        created: Utc::now() - Duration::minutes(created_minutes_ago),
    }
}

fn tz(s: &str) -> Tz {
    s.parse().unwrap()
}

#[test]
fn hourly_interval() {
    let r = mk(Schedule::Interval(Interval { seconds: 3600 }), 5);
    let now = Utc::now();
    // First fire: created + 1h
    let next = next_due(&r, &RunState::default(), now).unwrap();
    assert_eq!(next, r.created + Duration::hours(1));

    // After firing, next is +1h from last fired.
    let state = RunState::default().fired(1, now);
    let next2 = next_due(&r, &state, now).unwrap();
    assert_eq!(next2, now + Duration::hours(1));
}

#[test]
fn interval_bare_minutes_and_seconds_roundtrip() {
    let json = serde_json::json!({"type": "interval", "minutes": 60});
    let s: Schedule = serde_json::from_value(json).unwrap();
    assert_eq!(s, Schedule::Interval(Interval { seconds: 3600 }));

    let json = serde_json::json!({"type": "interval", "seconds": 90});
    let s: Schedule = serde_json::from_value(json).unwrap();
    assert_eq!(s, Schedule::Interval(Interval { seconds: 90 }));

    let out = serde_json::to_value(Schedule::Interval(Interval { seconds: 3600 })).unwrap();
    assert_eq!(out["minutes"], 60);
    let out = serde_json::to_value(Schedule::Interval(Interval { seconds: 90 })).unwrap();
    assert_eq!(out["seconds"], 90);
}

#[test]
fn daily_at_time() {
    let tz = tz("UTC");
    let r = mk(
        Schedule::Daily(Daily {
            time: "09:00".into(),
            days: vec![],
            start_date: None,
            end_date: None,
        }),
        0,
    );
    // Created now; first occurrence is the next 09:00 strictly after created.
    let now = Utc::now();
    let next = next_due(&r, &RunState::default(), now).unwrap();
    assert_eq!(next.hour(), 9);
    assert_eq!(next.minute(), 0);
    assert!(next > r.created);
    assert!((next - r.created).num_hours() <= 24);

    // Firing at 09:00 → next is tomorrow 09:00.
    let fire_time = tz.with_ymd_and_hms(2026, 9, 22, 9, 0, 0).unwrap().with_timezone(&Utc);
    let state = RunState::default().fired(1, fire_time);
    let next2 = next_due(&r, &state, fire_time).unwrap();
    assert_eq!(next2, tz.with_ymd_and_hms(2026, 9, 23, 9, 0, 0).unwrap().with_timezone(&Utc).with_timezone(&Utc));
}

#[test]
fn weekly_days() {
    // Mon/Wed/Fri 18:00 UTC. 2026-09-21 is a Monday.
    let tz = tz("UTC");
    let created = tz.with_ymd_and_hms(2026, 9, 21, 10, 0, 0).unwrap().with_timezone(&Utc); // Mon 10:00
    let mut r = mk(
        Schedule::Daily(Daily {
            time: "18:00".into(),
            days: vec!["mon".into(), "wed".into(), "fri".into()],
            start_date: None,
            end_date: None,
        }),
        0,
    );
    r.created = created;

    let next = next_due(&r, &RunState::default(), created).unwrap();
    assert_eq!(next, tz.with_ymd_and_hms(2026, 9, 21, 18, 0, 0).unwrap().with_timezone(&Utc).with_timezone(&Utc)); // same Monday

    // Fired Monday evening → next is Wednesday.
    let fired = tz.with_ymd_and_hms(2026, 9, 21, 18, 0, 0).unwrap().with_timezone(&Utc);
    let state = RunState::default().fired(1, fired);
    let next2 = next_due(&r, &state, fired).unwrap();
    assert_eq!(next2, tz.with_ymd_and_hms(2026, 9, 23, 18, 0, 0).unwrap().with_timezone(&Utc).with_timezone(&Utc)); // Wed

    // Fired Wednesday → Friday.
    let fired_wed = tz.with_ymd_and_hms(2026, 9, 23, 18, 0, 0).unwrap().with_timezone(&Utc);
    let state2 = RunState::default().fired(1, fired_wed);
    let next3 = next_due(&r, &state2, fired_wed).unwrap();
    assert_eq!(next3, tz.with_ymd_and_hms(2026, 9, 25, 18, 0, 0).unwrap().with_timezone(&Utc).with_timezone(&Utc)); // Fri

    // Fired Friday → next Monday.
    let fired_fri = tz.with_ymd_and_hms(2026, 9, 25, 18, 0, 0).unwrap().with_timezone(&Utc);
    let state3 = RunState::default().fired(1, fired_fri);
    let next4 = next_due(&r, &state3, fired_fri).unwrap();
    assert_eq!(next4, tz.with_ymd_and_hms(2026, 9, 28, 18, 0, 0).unwrap().with_timezone(&Utc).with_timezone(&Utc)); // Mon
}

#[test]
fn once_reminder_fires_exactly_once() {
    let at = (Utc::now() + Duration::hours(2)).to_rfc3339();
    let r = mk(Schedule::Once(Once { at }), 0);
    let now = Utc::now();
    let next = next_due(&r, &RunState::default(), now).unwrap();
    assert!(next > now);

    // After now passes `at`, it is due.
    let later = now + Duration::hours(3);
    assert!(is_due(&r, &RunState::default(), later));

    // Once fired, never due again.
    let state = RunState::default().fired(1, later);
    assert!(next_due(&r, &state, later).is_none());
    assert!(!is_due(&r, &state, later));
}

#[test]
fn disabled_never_due() {
    let mut r = mk(Schedule::Interval(Interval { seconds: 60 }), 120);
    r.enabled = false;
    assert!(!is_due(&r, &RunState::default(), Utc::now()));
    assert!(next_due(&r, &RunState::default(), Utc::now()).is_some()); // still computable
}

#[test]
fn missed_while_sleeping_coalesces_to_once() {
    // Hourly reminder, last fired 3h ago (laptop slept).
    // It must be due exactly ONCE now — not three times.
    let r = mk(Schedule::Interval(Interval { seconds: 3600 }), 240);
    let now = Utc::now();
    let last = now - Duration::hours(3);
    let state = RunState::default().fired(1, last);

    assert!(is_due(&r, &state, now));
    // Simulate the daemon firing: update state, re-check immediately.
    let state2 = state.fired(1, now);
    assert!(!is_due(&r, &state2, now));
    // Next due is 1h from the (single) wake fire.
    assert_eq!(next_due(&r, &state2, now).unwrap(), now + Duration::hours(1));
}

#[test]
fn clock_moved_backwards_does_not_wait_forever() {
    let r = mk(Schedule::Interval(Interval { seconds: 3600 }), 0);
    let now = Utc::now();
    // last_fired somehow 2h in the future (clock jumped back)
    let state = RunState {
        last_fired: [(1, now + Duration::hours(2))].into_iter().collect(),
    };
    // next_due clamps anchor to now → due in 1h, not 3h.
    let next = next_due(&r, &state, now).unwrap();
    assert_eq!(next, now + Duration::hours(1));
}

#[test]
fn dst_spring_forward_gap_fires_hour_later() {
    // America/New_York 2027-03-14: 02:00 -> 03:00 (02:30 does not exist).
    let tz = tz("America/New_York");
    let created = tz.with_ymd_and_hms(2027, 3, 13, 12, 0, 0).unwrap().with_timezone(&Utc); // day before
    let mut r = mk(
        Schedule::Daily(Daily {
            time: "02:30".into(),
            days: vec![],
            start_date: None,
            end_date: None,
        }),
        0,
    );
    r.created = created;
    r.timezone = "America/New_York".into();

    let next = next_due(&r, &RunState::default(), created).unwrap();
    // Gap: 02:30 EST doesn't exist; policy = fire at 03:30 EDT.
    assert_eq!(next, tz.with_ymd_and_hms(2027, 3, 14, 3, 30, 0).unwrap().with_timezone(&Utc).with_timezone(&Utc));
}

#[test]
fn dst_fall_back_still_fires_once() {
    // America/New_York 2026-11-01: fall back, 01:30 occurs twice.
    let tz = tz("America/New_York");
    let created = tz.with_ymd_and_hms(2026, 10, 31, 12, 0, 0).unwrap().with_timezone(&Utc);
    let mut r = mk(
        Schedule::Daily(Daily {
            time: "01:30".into(),
            days: vec![],
            start_date: None,
            end_date: None,
        }),
        0,
    );
    r.created = created;
    r.timezone = "America/New_York".into();

    let next = next_due(&r, &RunState::default(), created).unwrap();
    // 01:30 occurs twice; policy = earliest occurrence.
    let expected = tz
        .from_local_datetime(&chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap().and_hms_opt(1, 30, 0).unwrap())
        .earliest()
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(next, expected);
}

#[test]
fn timezone_change_uses_reminder_tz() {
    // Reminder fixed to Tokyo time; "now" in UTC.
    let tz = tz("Asia/Tokyo");
    let created = tz.with_ymd_and_hms(2026, 9, 21, 9, 0, 0).unwrap().with_timezone(&Utc); // 09:00 JST = 00:00 UTC
    let mut r = mk(
        Schedule::Daily(Daily {
            time: "10:00".into(),
            days: vec![],
            start_date: None,
            end_date: None,
        }),
        0,
    );
    r.timezone = "Asia/Tokyo".into();
    r.created = created;

    let next = next_due(&r, &RunState::default(), created).unwrap();
    // Created 09:00 JST → same-day 10:00 JST is the first occurrence (= 01:00 UTC).
    assert_eq!(next, tz.with_ymd_and_hms(2026, 9, 21, 10, 0, 0).unwrap().with_timezone(&Utc));
}

#[test]
fn start_and_end_dates() {
    let tz = tz("UTC");
    let created = tz.with_ymd_and_hms(2026, 9, 1, 8, 0, 0).unwrap().with_timezone(&Utc);
    let mut r = mk(
        Schedule::Daily(Daily {
            time: "09:00".into(),
            days: vec![],
            start_date: Some("2026-09-10".into()),
            end_date: Some("2026-09-12".into()),
        }),
        0,
    );
    r.created = created;

    // Before start → first occurrence is 2026-09-10.
    let next = next_due(&r, &RunState::default(), created).unwrap();
    assert_eq!(next, tz.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap().with_timezone(&Utc).with_timezone(&Utc));

    // After end → None.
    let after_end = tz.with_ymd_and_hms(2026, 9, 13, 9, 0, 0).unwrap().with_timezone(&Utc);
    let state = RunState::default().fired(1, after_end);
    assert!(next_due(&r, &state, after_end).is_none());
}

#[test]
fn next_upcoming_picks_soonest() {
    let tz = tz("UTC");
    let base = tz.with_ymd_and_hms(2026, 9, 21, 10, 0, 0).unwrap().with_timezone(&Utc);
    let mut a = mk(Schedule::Interval(Interval { seconds: 1800 }), 0);
    a.id = 1;
    a.title = "A".into();
    a.created = base;
    let mut b = mk(Schedule::Interval(Interval { seconds: 600 }), 0);
    b.id = 2;
    b.title = "B".into();
    b.created = base;

    let (winner, _t) = next_upcoming([&a, &b], &RunState::default(), base).unwrap();
    assert_eq!(winner.title, "B"); // 10 minutes sooner
}

#[test]
fn duplicate_prevention_same_tick() {
    // Firing and persisting before spawning means a second check in the
    // same tick (or a crash-restart) cannot re-fire.
    let r = mk(Schedule::Interval(Interval { seconds: 60 }), 10);
    let now = Utc::now();
    let mut state = RunState::default();
    assert!(is_due(&r, &state, now));
    state = state.fired(1, now); // daemon persists BEFORE spawning renderer
    assert!(!is_due(&r, &state, now));
    assert!(!is_due(&r, &state, now + Duration::seconds(1)));
}
