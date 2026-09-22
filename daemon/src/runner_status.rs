//! Shared daemon status payload (used by both IPC transports).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub pid: u32,
    pub reminders: usize,
    pub enabled_reminders: usize,
    pub character: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_title: Option<String>,
    /// Seconds until next reminder fires (positive = future).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_in_secs: Option<i64>,
}
