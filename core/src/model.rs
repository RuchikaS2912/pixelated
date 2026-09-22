//! Reminder data model.
//!
//! The engine is intentionally decoupled from presentation: a `Reminder`
//! describes *what* and *when*, never *how it is displayed*.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// Top-level `reminders.json` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderList {
    pub version: u32,
    pub next_id: u64,
    pub reminders: Vec<Reminder>,
}

impl Default for ReminderList {
    fn default() -> Self {
        ReminderList {
            version: 1,
            next_id: 1,
            reminders: Vec::new(),
        }
    }
}

impl ReminderList {
    pub fn get(&self, id: u64) -> Option<&Reminder> {
        self.reminders.iter().find(|r| r.id == id)
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Reminder> {
        self.reminders.iter_mut().find(|r| r.id == id)
    }
}

/// A single reminder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: u64,
    pub title: String,
    /// Emoji shown in the speech bubble. Optional.
    #[serde(default)]
    pub emoji: Option<String>,
    /// Recurrence definition. See [`crate::recurrence::Schedule`].
    pub schedule: crate::recurrence::Schedule,
    /// Character override for this reminder; `None` uses the global config.
    #[serde(default)]
    pub character: Option<String>,
    /// IANA timezone the schedule is interpreted in.
    #[serde(default = "default_tz")]
    pub timezone: String,
    pub enabled: bool,
    /// Set when a one-time reminder has fired (kept for history).
    #[serde(default)]
    pub completed: bool,
    /// Creation timestamp (UTC, RFC3339).
    pub created: DateTime<Utc>,
}

fn default_tz() -> String {
    crate::util::system_timezone().to_string()
}

impl Reminder {
    pub fn tz(&self) -> Tz {
        self.timezone.parse().unwrap_or(chrono_tz::UTC)
    }

    /// Message displayed by the renderer, e.g. `💧 Drink water!`.
    pub fn message(&self) -> String {
        match &self.emoji {
            Some(e) if !e.is_empty() => format!("{} {}", e, self.title),
            _ => self.title.clone(),
        }
    }
}

/// Persisted per-reminder runtime state (`state.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunState {
    /// reminder id -> last fired time (RFC3339 UTC).
    #[serde(default)]
    pub last_fired: std::collections::BTreeMap<u64, DateTime<Utc>>,
}

impl RunState {
    pub fn fired(&self, id: u64, at: DateTime<Utc>) -> RunState {
        let mut s = self.clone();
        s.last_fired.insert(id, at);
        s
    }
}
