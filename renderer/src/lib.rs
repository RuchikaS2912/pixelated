//! Walking Reminder renderer.
//!
//! The renderer receives a reminder event (message + character) and
//! decides how to display it. It runs as a short-lived process spawned
//! by the daemon (or directly by `walking-reminder test`), so idle
//! memory usage is zero: the overlay only exists for the ~8s walk.

pub mod anim;
pub mod character;
pub mod platform;

use wremind_core::paths::Home;
use wremind_core::Config;

pub use character::{Character, Direction};

/// Everything needed to perform one walk.
#[derive(Debug, Clone)]
pub struct ShowOptions {
    /// Full message including emoji, e.g. "💧 Drink water!".
    pub message: String,
    pub character: Character,
    pub direction: Direction,
    pub duration_override: Option<f64>,
    pub size_override: Option<u32>,
    pub speed_override: Option<u32>,
    pub sound: bool,
}

/// Entry point used by `walking-reminder __render` and `walking-reminder test`.
///
/// `character_name` resolves against the home characters dir; the bundled
/// default is auto-extracted on first run.
pub fn run_show(
    message: &str,
    character_name: Option<&str>,
    direction: Option<Direction>,
    duration_override: Option<f64>,
) -> Result<(), String> {
    run_show_screenshot(message, character_name, direction, duration_override, None)
}

/// Like [`run_show`], but optionally renders one mid-walk frame to a PNG
/// (verification/test tooling — no live window needed).
#[allow(clippy::too_many_arguments)]
pub fn run_show_screenshot(
    message: &str,
    character_name: Option<&str>,
    direction: Option<Direction>,
    duration_override: Option<f64>,
    screenshot: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let home = Home::resolve();
    home.ensure().map_err(|e| e.to_string())?;
    Character::ensure_default(&home).map_err(|e| e.to_string())?;

    let cfg = Config::load(&home).unwrap_or_default();
    let name = character_name
        .map(|s| s.to_string())
        .unwrap_or(cfg.character);
    let character = Character::load(&home, &name).map_err(|e| e.to_string())?;

    let direction = direction
        .or_else(|| Direction::parse(&cfg.direction))
        .unwrap_or_else(Direction::random);

    let opts = ShowOptions {
        message: message.to_string(),
        character,
        direction,
        duration_override: duration_override.or(cfg.duration),
        size_override: cfg.size,
        speed_override: cfg.speed,
        sound: cfg.sound,
    };
    platform::show_walk(opts, screenshot)
}

/// Desktop pet mode: the character idles at a remembered spot on screen
/// and can be dragged anywhere with the mouse.
pub fn run_pet(character_name: Option<&str>, no_render: bool) -> Result<(), String> {
    let home = Home::resolve();
    if no_render {
        // Headless validation for tests/CI.
        home.ensure().map_err(|e| e.to_string())?;
        Character::ensure_default(&home).map_err(|e| e.to_string())?;
        let cfg = Config::load(&home).unwrap_or_default();
        let name = character_name.map(|s| s.to_string()).unwrap_or(cfg.character);
        Character::load(&home, &name).map_err(|e| e.to_string())?;
        return Ok(());
    }
    platform::run_pet(character_name)
}
