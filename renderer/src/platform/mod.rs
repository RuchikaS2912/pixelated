//! Platform overlay dispatch. Each supported desktop OS implements
//! `show_walk` with a borderless, transparent, always-on-top,
//! non-focus-stealing window that walks a character across the screen.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(target_os = "macos"))]
pub mod stub;

#[cfg(target_os = "macos")]
pub use macos::{run_pet, show_walk};

#[cfg(not(target_os = "macos"))]
pub use stub::{run_pet, show_walk};
