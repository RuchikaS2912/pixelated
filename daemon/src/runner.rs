//! Daemon main loop.
//!
//! Responsibilities (requirement 3):
//! - load reminder configuration (and reload on change)
//! - maintain the scheduler, calculate upcoming reminders
//! - trigger reminders and launch the visual overlay
//! - survive terminal closure (spawned detached, no controlling tty)
//! - recover after sleep/wake (coalesce missed reminders to one)
//! - avoid duplicate reminders (persist last_fired BEFORE spawning)
//! - persist state locally
//! - minimal idle CPU (1s tick)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Utc};

use wremind_core::model::{ReminderList, RunState};
use wremind_core::paths::Home;
use wremind_core::recurrence::{is_due, next_upcoming};
use wremind_core::config::{Config, ConfigError};
use wremind_core::store::{load_reminders, load_state_lossy, save_state};

use crate::ipc::{self, Request, Response, StatusInfo};
use crate::queue::{PendingShow, ShowQueue};
use crate::spawn::RendererPool;

const TICK: Duration = Duration::from_millis(1000);
/// A wall-clock jump larger than this means the machine slept.
const SLEEP_GAP: Duration = Duration::from_secs(5);

pub struct Shared {
    pub reminders: Mutex<ReminderList>,
    pub state: Mutex<RunState>,
    pub config: Mutex<Config>,
    pub queue: Mutex<ShowQueue>,
    pub pool: Mutex<RendererPool>,
    pub stop: AtomicBool,
    /// Tracks meeting-mode edges so the pet can be hidden/restored.
    pub meeting_was_on: AtomicBool,
    pub pet_hidden_by_meeting: AtomicBool,
    /// Auto-detected meeting (pattern name that matched, if any).
    pub meeting_detected: Mutex<Option<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("another daemon is already running (pid {0})")]
    AlreadyRunning(u32),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Store(#[from] wremind_core::store::StoreError),
}

/// Test-friendly core of the daemon loop: one scheduler tick.
/// Returns the shows to enqueue this tick (already persisted to state).
pub fn schedule_tick(
    reminders: &ReminderList,
    state: &mut RunState,
    now: DateTime<Utc>,
    wake_skip: bool,
) -> Vec<(u64, String, Option<String>)> {
    let mut fired = Vec::new();
    for r in reminders.reminders.iter().filter(|r| r.enabled && !r.completed) {
        if is_due(r, state, now) {
            // Persist BEFORE firing: crash/restart cannot duplicate.
            // After a sleep, is_due is still true exactly once per reminder
            // (anchors advance to `now` on fire), satisfying "show only the
            // next valid reminder after waking".
            let _ = wake_skip;
            state.last_fired.insert(r.id, now);
            fired.push((r.id, r.message(), r.character.clone()));
        }
    }
    fired
}

/// Run the daemon until stopped via IPC (or the menu bar Quit).
pub fn run(bin: std::path::PathBuf) -> Result<(), DaemonError> {
    let home = Home::resolve();
    home.ensure()?;
    wremind_render_placeholder();

    let _lock = SingleInstanceLock::acquire(&home)?;
    std::fs::write(home.pid_file(), std::process::id().to_string())?;
    let _ = std::fs::remove_file(home.socket_file());

    let (reminders, recovery_note) = load_reminders(&home)?;
    let state = load_state_lossy(&home);
    let config = Config::load(&home).unwrap_or_else(|e| {
        let msg = match &e {
            ConfigError::Parse(m) => m.clone(),
            ConfigError::Io(io) => io.to_string(),
        };
        log_to_file(&home, &format!("config load failed ({msg}); using defaults"));
        Config::default()
    });
    if let Some(note) = recovery_note {
        log_to_file(&home, &note);
    }
    log_to_file(
        &home,
        &format!(
            "daemon start pid={} reminders={} character={}",
            std::process::id(),
            reminders.reminders.len(),
            config.character
        ),
    );

    let shared = Arc::new(Shared {
        reminders: Mutex::new(reminders),
        state: Mutex::new(state),
        config: Mutex::new(config),
        queue: Mutex::new(ShowQueue::new(1)),
        pool: Mutex::new(RendererPool::new()),
        stop: AtomicBool::new(false),
        meeting_was_on: AtomicBool::new(false),
        pet_hidden_by_meeting: AtomicBool::new(false),
        meeting_detected: Mutex::new(None),
    });
    *shared.queue.lock().unwrap() =
        ShowQueue::new(shared.config.lock().unwrap().max_simultaneous as usize);

    // IPC thread.
    {
        let shared = shared.clone();
        let home = home.clone();
        let sock = home.socket_file();
        std::thread::spawn(move || {
            let s2 = shared.clone();
            let _ = ipc::serve(sock, move |req| handle_request(&s2, req));
        });
    }

    // Scheduler loop: background thread on macOS (main thread hosts the
    // menu bar), blocking on other platforms.
    #[cfg(target_os = "macos")]
    let handle = {
        let shared = shared.clone();
        let home = home.clone();
        let bin = bin.clone();
        std::thread::spawn(move || scheduler_loop(shared, home, bin))
    };

    #[cfg(target_os = "macos")]
    {
        crate::menubar::run_menu_bar(shared.clone(), bin.clone(), home.clone());
        // Quit selected: stop the scheduler and clean up.
        let _ = handle.join();
        log_to_file(&home, "daemon stop");
        let _ = std::fs::remove_file(home.socket_file());
        let _ = std::fs::remove_file(home.pid_file());
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        scheduler_loop(shared, home, bin);
        log_to_file(&home, "daemon stop");
        let _ = std::fs::remove_file(home.socket_file());
        let _ = std::fs::remove_file(home.pid_file());
        Ok(())
    }
}

fn scheduler_loop(shared: Arc<Shared>, home: Home, bin: std::path::PathBuf) {
    let mut tick_count: u64 = 0;
    // Main loop.
    let mut last_tick_wall = SystemTime::now();
    let mut reminders_mtime = file_mtime(&home.reminders_file());
    let mut config_mtime = file_mtime(&home.config_file());

    loop {
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        let tick_start = Instant::now();
        let now = Utc::now();

        // ---- sleep/wake detection ----
        let wake_skip = if let Ok(elapsed) = last_tick_wall.elapsed() {
            elapsed > TICK + SLEEP_GAP
        } else {
            false
        };
        last_tick_wall = SystemTime::now();

        // ---- hot reload on file change ----
        if file_mtime(&home.reminders_file()) != reminders_mtime {
            reminders_mtime = file_mtime(&home.reminders_file());
            match load_reminders(&home) {
                Ok((list, note)) => {
                    if let Some(n) = note {
                        log_to_file(&home, &n);
                    }
                    log_to_file(
                        &home,
                        &format!("reloaded reminders: {} active", list.reminders.len()),
                    );
                    *shared.reminders.lock().unwrap() = list;
                }
                Err(e) => log_to_file(&home, &format!("reload failed: {e}")),
            }
        }
        if file_mtime(&home.config_file()) != config_mtime {
            config_mtime = file_mtime(&home.config_file());
            if let Ok(cfg) = Config::load(&home) {
                let max_sim = cfg.max_simultaneous as usize;
                *shared.queue.lock().unwrap() = ShowQueue::new(max_sim);
                *shared.config.lock().unwrap() = cfg;
                log_to_file(&home, "reloaded config");
            }
        }

        // ---- meeting mode: auto-detection + edges ----
        {
            // Auto-detect every ~5 s (ticks are ~1 s).
            tick_count += 1;
            if tick_count % 5 == 0 {
                let (auto_on, patterns) = {
                    let cfg = shared.config.lock().unwrap();
                    (cfg.meeting_auto, cfg.meeting_apps.clone())
                };
                let detected = if auto_on { detect_meeting(&patterns) } else { None };
                *shared.meeting_detected.lock().unwrap() = detected;
            }
            let manual = shared.config.lock().unwrap().meeting_mode;
            let detected = shared.meeting_detected.lock().unwrap().clone();
            let meeting_now = manual || detected.is_some();
            let was_on = shared
                .meeting_was_on
                .swap(meeting_now, Ordering::SeqCst);
            if meeting_now && !was_on {
                if crate::runner::pets_running() {
                    crate::runner::kill_pets();
                    shared.pet_hidden_by_meeting.store(true, Ordering::SeqCst);
                }
                match &detected {
                    Some(pat) => log_to_file(
                        &home,
                        &format!("meeting auto-detected ({pat}): walks suppressed, pet hidden"),
                    ),
                    None => {
                        log_to_file(&home, "meeting mode ON: walks suppressed, pet hidden")
                    }
                }
            } else if !meeting_now && was_on {
                if shared.pet_hidden_by_meeting.swap(false, Ordering::SeqCst) {
                    let name = shared.config.lock().unwrap().character.clone();
                    let _ = std::process::Command::new(&bin)
                        .args(["__pet", "--character", &name])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }
                log_to_file(&home, "meeting mode OFF: walks resumed, pet restored");
            }
        }

        // ---- scheduler ----
        {
            let meeting_active = shared.config.lock().unwrap().meeting_mode
                || shared.meeting_detected.lock().unwrap().is_some();
            if meeting_active {
                // Reminders stay pending (anchors not advanced): they
                // coalesce into one walk when meeting mode ends.
                let alive = shared.pool.lock().unwrap().reap();
                let _ = alive;
            } else {
            let reminders = shared.reminders.lock().unwrap().clone();
            let mut state = shared.state.lock().unwrap();
            let fired = schedule_tick(&reminders, &mut state, now, wake_skip);
            if !fired.is_empty() {
                let _ = save_state(&home, &state);
                drop(state);
                let mut queue = shared.queue.lock().unwrap();
                for (id, msg, character) in fired {
                    log_to_file(&home, &format!("reminder {id} triggered: {msg}"));
                    queue.push(PendingShow {
                        message: msg,
                        character,
                        label: format!("{id}"),
                    });
                }
            }
            }
        }

        // ---- renderer pool ----
        {
            let mut pool = shared.pool.lock().unwrap();
            let alive = pool.reap();
            let mut queue = shared.queue.lock().unwrap();
            let free = queue.capacity().saturating_sub(alive);
            for item in queue.take(free) {
                if let Err(e) = pool.spawn(&bin, &item) {
                    log_to_file(&home, &format!("renderer spawn failed: {e}"));
                }
            }
        }

        // Sleep the remainder of the tick.
        let elapsed = tick_start.elapsed();
        if elapsed < TICK {
            std::thread::sleep(TICK - elapsed);
        }
    }

}

fn wremind_render_placeholder() {
    // Default character assets are extracted by the CLI/renderer on first
    // use; the daemon itself never renders.
}

fn handle_request(shared: &Arc<Shared>, req: Request) -> Response {
    match req {
        Request::Ping => Response::ok(Some(status_info(shared))),
        Request::Show { message, character } => {
            shared.queue.lock().unwrap().push(PendingShow {
                message,
                character,
                label: "manual".into(),
            });
            Response::ok(None)
        }
        Request::Stop => {
            shared.stop.store(true, Ordering::SeqCst);
            Response::ok(None)
        }
    }
}

pub fn status_info(shared: &Shared) -> StatusInfo {
    let now = Utc::now();
    let reminders = shared.reminders.lock().unwrap();
    let state = shared.state.lock().unwrap();
    let next = next_upcoming(reminders.reminders.iter(), &state, now);
    let meeting = if shared.config.lock().unwrap().meeting_mode {
        Some("on".to_string())
    } else {
        shared.meeting_detected.lock().unwrap().clone()
    };
    StatusInfo {
        pid: std::process::id(),
        meeting,
        reminders: reminders.reminders.len(),
        enabled_reminders: reminders
            .reminders
            .iter()
            .filter(|r| r.enabled)
            .count(),
        character: shared.config.lock().unwrap().character.clone(),
        next_title: next.map(|(r, _)| r.title.clone()),
        next_in_secs: next.map(|(_, t)| (t - now).num_seconds()),
    }
}

/// Scan for meeting-app signals. Returns the first matching pattern.
#[cfg(test)]
fn detect_meeting_testable() {
    use std::process::{Command, Stdio};
    let mut child = Command::new("bash")
        .arg("-c")
        .arg("exec -a DribbleMeetingProbe sleep 4")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("bash");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let hit = detect_meeting(&["DribbleMeetingProbe".to_string()]);
    let _ = child.kill();
    assert_eq!(hit.as_deref(), Some("DribbleMeetingProbe"), "detection failed");
}

#[cfg(test)]
mod detect_tests {
    #[test]
    fn detects_named_process() {
        super::detect_meeting_testable();
    }
}

/// Scan for meeting-app signals. Returns the first matching pattern.
/// Runs pgrep per pattern; cheap enough at a 5 s cadence.
fn detect_meeting(patterns: &[String]) -> Option<String> {
    for pat in patterns {
        #[cfg(unix)]
        {
            let ok = std::process::Command::new("pgrep")
                .args(["-i", "-f"])
                .arg(pat)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped()) // output() keeps explicit stdio!
                .stderr(std::process::Stdio::null())
                .output()
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(false);
            if ok {
                return Some(pat.clone());
            }
        }
        #[cfg(not(unix))]
        {
            let _ = pat;
        }
    }
    None
}

/// True if any desktop pet process is running.
pub fn pets_running() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("pgrep")
            .arg("-f")
            .arg("dribble __pet")
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Kill every desktop pet (discovery by command line).
pub fn kill_pets() {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("pkill")
            .arg("-f")
            .arg("dribble __pet")
            .output();
    }
}

fn file_mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn log_to_file(home: &Home, line: &str) {
    let path = home.log_file();
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let entry = format!("[{stamp}] {line}\n");
    if let Ok(mut existing) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = existing.write_all(entry.as_bytes());
    }
}

/// Exclusive, advisory single-instance lock via flock on daemon.lock.
pub struct SingleInstanceLock {
    _file: std::fs::File,
}

impl SingleInstanceLock {
    #[cfg(unix)]
    pub fn acquire(home: &Home) -> Result<SingleInstanceLock, DaemonError> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(home.lock_file())?;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let pid = std::fs::read_to_string(home.pid_file())
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            return Err(DaemonError::AlreadyRunning(pid));
        }
        Ok(SingleInstanceLock { _file: file })
    }

    #[cfg(not(unix))]
    pub fn acquire(home: &Home) -> Result<SingleInstanceLock, DaemonError> {
        // Simple existence check fallback (OS-specific locks later).
        let lock = home.lock_file();
        if lock.exists() {
            let pid = std::fs::read_to_string(home.pid_file())
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            return Err(DaemonError::AlreadyRunning(pid));
        }
        let file = std::fs::File::create(&lock)?;
        Ok(SingleInstanceLock { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use wremind_core::model::Reminder;
    use wremind_core::recurrence::{Daily, Interval, Once, Schedule};

    fn reminder(id: u64, schedule: Schedule, created_ago_mins: i64) -> Reminder {
        Reminder {
            id,
            title: format!("R{id}"),
            emoji: None,
            schedule,
            character: None,
            timezone: "UTC".into(),
            enabled: true,
            completed: false,
            created: Utc::now() - Duration::minutes(created_ago_mins),
        }
    }

    fn list(rs: Vec<Reminder>) -> ReminderList {
        ReminderList { version: 1, next_id: rs.len() as u64 + 1, reminders: rs }
    }

    #[test]
    fn tick_fires_due_interval_once() {
        let r = reminder(
            1,
            Schedule::Interval(Interval { seconds: 60 }),
            120,
        );
        let reminders = list(vec![r]);
        let mut state = RunState::default();
        let fired = schedule_tick(&reminders, &mut state, Utc::now(), false);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].0, 1);
        // Second tick same moment: not due again (duplicate prevention).
        let fired2 = schedule_tick(&reminders, &mut state, Utc::now(), false);
        assert!(fired2.is_empty());
    }

    #[test]
    fn sleep_three_hours_fires_once() {
        let r = reminder(
            1,
            Schedule::Interval(Interval { seconds: 3600 }),
            240,
        );
        let reminders = list(vec![r]);
        let now = Utc::now();
        let last = now - Duration::hours(3);
        let mut state = RunState { last_fired: [(1, last)].into_iter().collect() };
        // Wake tick
        let fired = schedule_tick(&reminders, &mut state, now, true);
        assert_eq!(fired.len(), 1);
        // Immediately after: quiet (no backlog cascade).
        let fired2 = schedule_tick(&reminders, &mut state, now, true);
        assert!(fired2.is_empty());
    }

    #[test]
    fn disabled_and_completed_skipped() {
        let mut r = reminder(1, Schedule::Interval(Interval { seconds: 60 }), 120);
        r.enabled = false;
        let mut state = RunState::default();
        assert!(schedule_tick(&list(vec![r.clone()]), &mut state, Utc::now(), false).is_empty());
        r.enabled = true;
        r.completed = true;
        assert!(schedule_tick(&list(vec![r]), &mut state, Utc::now(), false).is_empty());
    }

    #[test]
    fn once_reminder_completes_after_fire() {
        let r = reminder(
            1,
            Schedule::Once(Once { at: (Utc::now() - Duration::minutes(1)).to_rfc3339() }),
            5,
        );
        let reminders = list(vec![r]);
        let mut state = RunState::default();
        let fired = schedule_tick(&reminders, &mut state, Utc::now(), false);
        assert_eq!(fired.len(), 1);
        // The runner marks one-time reminders completed on fire (done by
        // the CLI-facing layer when persisting); engine-level, the
        // last_fired marker already prevents refiring.
        let fired2 = schedule_tick(&reminders, &mut state, Utc::now(), false);
        assert!(fired2.is_empty());
    }

    #[test]
    fn daily_not_due_before_time() {
        let tomorrow = (Utc::now() + Duration::days(1)).naive_utc().date();
        let r = reminder(
            1,
            Schedule::Daily(Daily {
                time: "23:59".into(),
                days: vec![],
                start_date: Some(tomorrow.format("%Y-%m-%d").to_string()),
                end_date: None,
            }),
            0,
        );
        let mut state = RunState::default();
        assert!(schedule_tick(&list(vec![r]), &mut state, Utc::now(), false).is_empty());
    }
}
