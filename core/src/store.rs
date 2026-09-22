//! Persistence for reminders and runtime state.
//!
//! - Atomic writes (tmp + rename) so a crash never corrupts data.
//! - A `.bak` of the last good version is kept.
//! - On corruption, the backup is restored and the bad file quarantined
//!   (`reminders.json.corrupt-<timestamp>`) instead of crashing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{ReminderList, RunState};
use crate::paths::{write_atomic, Home};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid data in {path}: {message}")]
    Invalid { path: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReminderDoc {
    version: u32,
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    reminders: Vec<crate::model::Reminder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateDoc {
    version: u32,
    #[serde(default)]
    last_fired: std::collections::BTreeMap<u64, DateTime<Utc>>,
}

/// Load reminders with backup recovery. Returns the list (possibly empty)
/// and a human-readable note when recovery happened.
pub fn load_reminders(home: &Home) -> Result<(ReminderList, Option<String>), StoreError> {
    let path = home.reminders_file();
    if !path.exists() {
        return Ok((ReminderList::default(), None));
    }
    match read_reminders(&path) {
        Ok(list) => {
            // Keep backup fresh
            let _ = std::fs::copy(&path, backup_path(&path));
            Ok((list, None))
        }
        Err(primary_err) => {
            let bak = backup_path(&path);
            if bak.exists() {
                match read_reminders(&bak) {
                    Ok(list) => {
                        quarantine(&path);
                        let _ = std::fs::copy(&bak, &path);
                        return Ok((
                            list,
                            Some(format!(
                                "reminders.json was corrupted ({primary_err}); restored from backup"
                            )),
                        ));
                    }
                    Err(_) => {}
                }
            }
            quarantine(&path);
            Ok((
                ReminderList::default(),
                Some(format!(
                    "reminders.json was corrupted ({primary_err}) and no backup was usable; started empty"
                )),
            ))
        }
    }
}

fn read_reminders(path: &std::path::Path) -> Result<ReminderList, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let doc: ReminderDoc =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse error: {e}"))?;
    if doc.version != 1 {
        return Err(format!("unsupported version {}", doc.version));
    }
    let mut next_id = doc.next_id.max(1);
    for r in &doc.reminders {
        next_id = next_id.max(r.id + 1);
    }
    Ok(ReminderList {
        version: 1,
        next_id,
        reminders: doc.reminders,
    })
}

/// Save reminders atomically, keeping a `.bak` of the last good version.
pub fn save_reminders(home: &Home, list: &ReminderList) -> Result<(), StoreError> {
    let path = home.reminders_file();
    let doc = ReminderDoc {
        version: 1,
        next_id: list.next_id,
        reminders: list.reminders.clone(),
    };
    let json = serde_json::to_string_pretty(&doc).map_err(|e| StoreError::Invalid {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    write_atomic(&path, json.as_bytes())?;
    // Backup is written from the same bytes we just verified we can encode.
    let _ = write_atomic(&backup_path(&path), json.as_bytes());
    Ok(())
}

pub fn load_state(home: &Home) -> Result<RunState, StoreError> {
    let path = home.state_file();
    if !path.exists() {
        return Ok(RunState::default());
    }
    let bytes = std::fs::read(&path)?;
    let doc: StateDoc = serde_json::from_slice(&bytes).map_err(|e| StoreError::Invalid {
        path: path.display().to_string(),
        message: format!("state.json corrupted, resetting: {e}"),
    })?;
    Ok(RunState {
        last_fired: doc.last_fired,
    })
}

pub fn save_state(home: &Home, state: &RunState) -> Result<(), StoreError> {
    let doc = StateDoc {
        version: 1,
        last_fired: state.last_fired.clone(),
    };
    let json = serde_json::to_string(&doc).map_err(|e| StoreError::Invalid {
        path: home.state_file().display().to_string(),
        message: e.to_string(),
    })?;
    write_atomic(&home.state_file(), json.as_bytes())?;
    // Corrupted state is never fatal: the daemon resets and continues.
    Ok(())
}

pub fn load_state_lossy(home: &Home) -> RunState {
    load_state(home).unwrap_or_default()
}

fn backup_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".bak");
    std::path::PathBuf::from(s)
}

fn quarantine(path: &std::path::Path) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut s = path.as_os_str().to_os_string();
    s.push(format!(".corrupt-{stamp}"));
    let _ = std::fs::rename(path, std::path::PathBuf::from(s));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Reminder;
    use crate::recurrence::{Interval, Schedule};

    fn tmp_home() -> (tempfile::TempDir, Home) {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::from_root(dir.path().to_path_buf());
        home.ensure().unwrap();
        (dir, home)
    }

    fn sample_reminder(id: u64) -> Reminder {
        Reminder {
            id,
            title: "Drink water".into(),
            emoji: Some("💧".into()),
            schedule: Schedule::Interval(Interval { seconds: 3600 }),
            character: None,
            timezone: "UTC".into(),
            enabled: true,
            completed: false,
            created: Utc::now(),
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let (_dir, home) = tmp_home();
        let mut list = ReminderList::default();
        list.reminders.push(sample_reminder(1));
        list.next_id = 2;
        save_reminders(&home, &list).unwrap();
        let (loaded, note) = load_reminders(&home).unwrap();
        assert!(note.is_none());
        assert_eq!(loaded.reminders.len(), 1);
        assert_eq!(loaded.reminders[0].title, "Drink water");
        assert_eq!(loaded.next_id, 2);
    }

    #[test]
    fn corruption_recovers_from_backup() {
        let (_dir, home) = tmp_home();
        let mut list = ReminderList::default();
        list.reminders.push(sample_reminder(1));
        save_reminders(&home, &list).unwrap();
        // Corrupt the primary file
        std::fs::write(home.reminders_file(), b"{ not json !!!").unwrap();
        let (recovered, note) = load_reminders(&home).unwrap();
        assert!(note.is_some());
        assert_eq!(recovered.reminders.len(), 1);
        assert_eq!(recovered.reminders[0].title, "Drink water");
    }

    #[test]
    fn corruption_without_backup_starts_empty() {
        let (_dir, home) = tmp_home();
        std::fs::write(home.reminders_file(), b"garbage").unwrap();
        let (recovered, note) = load_reminders(&home).unwrap();
        assert!(note.is_some());
        assert!(recovered.reminders.is_empty());
    }

    #[test]
    fn state_roundtrip() {
        let (_dir, home) = tmp_home();
        let st = RunState::default().fired(7, Utc::now());
        save_state(&home, &st).unwrap();
        let back = load_state(&home).unwrap();
        assert!(back.last_fired.contains_key(&7));
    }

    #[test]
    fn state_corruption_resets() {
        let (_dir, home) = tmp_home();
        std::fs::write(home.state_file(), b"nope").unwrap();
        let st = load_state_lossy(&home);
        assert!(st.last_fired.is_empty());
    }
}
