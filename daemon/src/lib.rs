//! Walking Reminder daemon: a small background process that
//! loads reminder configuration, maintains the scheduler, triggers
//! reminders, and launches the visual overlay renderer.
//!
//! Idle cost is a 1s poll loop (~0% CPU). The renderer runs as a
//! separate short-lived process, so the daemon itself stays tiny.
//! On macOS the daemon also hosts the menu bar item (see menubar.rs).

#[cfg(target_os = "macos")]
pub mod menubar;

#[cfg(unix)]
pub mod ipc;
#[cfg(not(unix))]
#[path = "ipc_stub.rs"]
pub mod ipc;

pub mod queue;
pub mod runner;
pub mod runner_status;
pub mod spawn;
pub mod startup;

pub use runner::run;
