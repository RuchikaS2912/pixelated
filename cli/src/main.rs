//! `dribble` — CLI entry point.
//!
//! The CLI is a thin shell over the core engine (`wremind-core`), the
//! daemon (`wremind-daemon`) and the renderer (`wremind-render`). It is
//! NOT the renderer itself: `test`/daemon triggers spawn a separate
//! short-lived renderer process.

mod commands;
mod ui;

use clap::{Parser, Subcommand};

use wremind_core::recurrence::Schedule;

#[derive(Parser)]
#[command(
    name = "dribble",
    version,
    about = "A tiny companion that walks across your screen to remind you of things",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a reminder
    Add {
        /// Reminder text, e.g. "Drink water"
        title: String,
        /// Interval, e.g. --every 1h, --every 45m, --every 90s
        #[arg(long, conflicts_with_all = &["at", "daily"])]
        every: Option<String>,
        /// Time of day ("19:00", "7:00pm") or full datetime ("2026-09-22 19:00")
        #[arg(long)]
        at: Option<String>,
        /// Weekdays for weekly reminders: mon,wed,fri
        #[arg(long, requires = "at")]
        days: Option<String>,
        /// Daily at a fixed time (alternative to --at)
        #[arg(long, conflicts_with_all = &["at", "every"])]
        daily: Option<String>,
        /// Emoji shown with the reminder (default: guessed from title)
        #[arg(long)]
        emoji: Option<String>,
        /// Character override for this reminder
        #[arg(long)]
        character: Option<String>,
        /// IANA timezone, e.g. America/New_York (default: system)
        #[arg(long)]
        timezone: Option<String>,
        /// Start date (YYYY-MM-DD) for daily/weekly schedules
        #[arg(long)]
        start: Option<String>,
        /// End date (YYYY-MM-DD) for daily/weekly schedules
        #[arg(long)]
        end: Option<String>,
    },

    /// List all reminders
    List,

    /// Remove a reminder by ID
    Remove { id: u64 },

    /// Edit a reminder
    Edit {
        id: u64,
        #[arg(long)]
        every: Option<String>,
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        days: Option<String>,
        #[arg(long)]
        daily: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        emoji: Option<String>,
        #[arg(long)]
        character: Option<String>,
    },

    /// Pause a reminder (or all)
    Pause { id: Option<u64> },
    /// Resume a paused reminder (or all)
    Resume { id: Option<u64> },

    /// Preview: walk the character across the screen now
    Test {
        #[arg(long)]
        message: Option<String>,
        #[arg(long)]
        character: Option<String>,
        /// Direction override: left/right/random
        #[arg(long)]
        direction: Option<String>,
    },

    /// Start the background daemon
    Start,
    /// Stop the background daemon
    Stop,
    /// Show daemon + reminder status
    Status,

    /// Start automatically at login (LaunchAgent / autostart)
    Enable,
    /// Disable automatic startup
    Disable,

    /// Read or change configuration
    Config {
        #[command(subcommand)]
        op: ConfigOp,
    },

    /// Manage characters
    Character {
        #[command(subcommand)]
        op: CharacterOp,
    },

    /// Keep the character on your desktop — drag him anywhere
    Pet {
        #[arg(long)]
        character: Option<String>,
        /// Stop the desktop pet
        #[arg(long)]
        stop: bool,
    },

    /// Health check: installation, daemon, characters, permissions
    Doctor,

    /// Show recent daemon log lines
    Logs {
        #[arg(default_value_t = 30)]
        lines: usize,
    },

    /// Internal: run the daemon (used by `start` and LaunchAgents)
    #[command(hide = true, name = "__daemon")]
    Daemon,
    /// Internal: run the desktop pet (spawned by `dribble pet`)
    #[command(hide = true, name = "__pet")]
    PetProcess {
        #[arg(long)]
        character: Option<String>,
        #[arg(long)]
        no_render: bool,
    },

    /// Internal: render one walk (spawned by daemon / test)
    #[command(hide = true, name = "__render")]
    Render {
        #[arg(long)]
        message: String,
        #[arg(long)]
        character: Option<String>,
        #[arg(long)]
        direction: Option<String>,
        #[arg(long)]
        duration: Option<f64>,
        /// Test/headless mode: validate everything but skip the overlay
        #[arg(long)]
        no_render: bool,
        /// Render one mid-walk frame to a PNG instead of animating
        #[arg(long)]
        screenshot: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConfigOp {
    /// Show all configuration
    Get,
    /// Set a key: character, speed, size, sound, duration, max_simultaneous, direction
    Set { key: String, value: String },
    /// Clear one override, or all if no key given
    Reset { key: Option<String> },
}

#[derive(Subcommand)]
enum CharacterOp {
    /// List installed characters
    List,
    /// Set the active character
    Set { name: String },
    /// Show character details
    Show { name: String },
    /// Import a character directory into ~/.dribble/characters/
    Import { path: std::path::PathBuf },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Add {
            title,
            every,
            at,
            days,
            daily,
            emoji,
            character,
            timezone,
            start,
            end,
        } => commands::add(
            &title, every, at, days, daily, emoji, character, timezone, start, end,
        ),
        Command::List => commands::list(),
        Command::Remove { id } => commands::remove(id),
        Command::Edit {
            id,
            every,
            at,
            days,
            daily,
            title,
            emoji,
            character,
        } => commands::edit(id, every, at, days, daily, title, emoji, character),
        Command::Pause { id } => commands::pause(id, false),
        Command::Resume { id } => commands::pause(id, true),
        Command::Test { message, character, direction } => {
            commands::test(message, character, direction)
        }
        Command::Start => commands::start(),
        Command::Stop => commands::stop(),
        Command::Status => commands::status(),
        Command::Enable => commands::enable(true),
        Command::Disable => commands::enable(false),
        Command::Config { op } => commands::config(op),
        Command::Character { op } => commands::character(op),
        Command::Pet { character, stop } => commands::pet(character, stop),
        Command::Doctor => commands::doctor(),
        Command::Logs { lines } => commands::logs(lines),
        Command::Daemon => commands::daemon(),
        Command::PetProcess { character, no_render } => {
            commands::pet_process(character, no_render)
        }
        Command::Render {
            message,
            character,
            direction,
            duration,
            no_render,
            screenshot,
        } => commands::render(
            &message,
            character,
            direction,
            duration,
            no_render,
            screenshot,
        ),
    };
    std::process::exit(code);
}

/// Resolve schedule flags into a `Schedule` (shared by add/edit).
pub fn build_schedule(
    every: Option<&str>,
    at: Option<&str>,
    days: Option<&str>,
    daily: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<Schedule, String> {
    if let Some(every) = every {
        let secs = wremind_core::util::parse_duration(every)
            .ok_or_else(|| format!("invalid --every '{every}' (examples: 1h, 45m, 90s)"))?;
        return Ok(Schedule::interval(secs));
    }
    let time_str = daily.or(at);
    let time_str = match time_str {
        Some(t) => t,
        None => return Err("specify a schedule: --every 1h, --daily 09:00, --at 19:00, or --days mon,wed,fri --at 18:00".into()),
    };
    // Full datetime -> one-time reminder
    if at.is_some() && wremind_core::util::parse_datetime(time_str, crate::tz_now()).is_some() && time_str.contains(char::is_whitespace) {
        return Ok(Schedule::Once(wremind_core::recurrence::Once {
            at: wremind_core::util::parse_datetime(time_str, crate::tz_now())
                .unwrap()
                .to_rfc3339(),
        }));
    }
    let time = wremind_core::util::normalize_time(time_str)
        .ok_or_else(|| format!("invalid time '{time_str}' (use HH:MM or H:MMam/pm)"))?;
    let days = match days {
        Some(d) => wremind_core::util::parse_days(d)
            .ok_or_else(|| format!("invalid --days '{d}' (mon,tue,wed,thu,fri,sat,sun)"))?,
        None => vec![],
    };
    let day_names: Vec<String> = days
        .iter()
        .map(|d| match d {
            chrono::Weekday::Mon => "mon",
            chrono::Weekday::Tue => "tue",
            chrono::Weekday::Wed => "wed",
            chrono::Weekday::Thu => "thu",
            chrono::Weekday::Fri => "fri",
            chrono::Weekday::Sat => "sat",
            chrono::Weekday::Sun => "sun",
        })
        .map(String::from)
        .collect();
    Ok(Schedule::Daily(wremind_core::recurrence::Daily {
        time,
        days: day_names,
        start_date: start.map(String::from),
        end_date: end.map(String::from),
    }))
}

pub fn tz_now() -> chrono_tz::Tz {
    wremind_core::util::system_timezone()
}
