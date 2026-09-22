//! Overlay stub for platforms whose native overlay is not yet wired up
//! (Windows/Linux are on the roadmap — see docs/ARCHITECTURE.md).
//! The daemon, scheduler and character system are fully portable; only
//! this window layer is OS-specific.

use crate::ShowOptions;

pub fn show_walk(_opts: ShowOptions, _screenshot: Option<std::path::PathBuf>) -> Result<(), String> {
    Err(
        "transparent overlay is not implemented for this platform yet (macOS is supported today)"
            .to_string(),
    )
}

pub fn run_pet(_character_name: Option<&str>) -> Result<(), String> {
    Err("desktop pet is not implemented for this platform yet (macOS is supported today)".into())
}
