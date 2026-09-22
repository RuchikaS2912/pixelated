//! Walking Reminder core engine.
//!
//! This crate is a pure library: reminders, recurrence computation,
//! persistence and configuration. It knows nothing about the CLI, the
//! daemon, or any visual rendering. The renderer receives reminder
//! events and decides how to display them.

pub mod config;
pub mod model;
pub mod paths;
pub mod recurrence;
pub mod store;
pub mod util;

pub use config::Config;
pub use model::{Reminder, ReminderList};
pub use store::{load_reminders, load_state, save_state, StoreError};
