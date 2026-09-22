//! Command implementations. Each returns a process exit code.

use chrono::Utc;
use wremind_core::config::Config;
use wremind_core::model::{Reminder, ReminderList};
use wremind_core::paths::Home;
use wremind_core::recurrence::{humanize, next_due, next_upcoming, Schedule};
use wremind_core::store::{load_reminders, load_state_lossy, save_reminders};
use wremind_render::character::{Character, DEFAULT_CHARACTER};

use crate::ui;
use crate::{build_schedule, CharacterOp, ConfigOp};

fn home() -> Home {
    Home::resolve()
}

fn ensure_ready() -> Result<Home, String> {
    let home = home();
    home.ensure().map_err(|e| format!("cannot create {}: {e}", home.root().display()))?;
    Character::ensure_default(&home)
        .map_err(|e| format!("cannot install default character: {e}"))?;
    Ok(home)
}

fn load_list(home: &Home) -> Result<(ReminderList, Option<String>), String> {
    load_reminders(home).map_err(|e| e.to_string())
}

fn persist(home: &Home, list: &ReminderList) -> Result<(), String> {
    save_reminders(home, list).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// add / list / remove / edit / pause / resume
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn add(
    title: &str,
    every: Option<String>,
    at: Option<String>,
    days: Option<String>,
    daily: Option<String>,
    emoji: Option<String>,
    character: Option<String>,
    timezone: Option<String>,
    start: Option<String>,
    end: Option<String>,
) -> i32 {
    let Ok(home) = ensure_ready() else {
        return 1;
    };
    let schedule = match build_schedule(
        every.as_deref(),
        at.as_deref(),
        days.as_deref(),
        daily.as_deref(),
        start.as_deref(),
        end.as_deref(),
    ) {
        Ok(s) => s,
        Err(e) => {
            ui::fail(&e);
            return 1;
        }
    };
    if let Some(tz) = &timezone {
        if tz.parse::<chrono_tz::Tz>().is_err() {
            ui::fail(&format!("unknown timezone '{tz}'"));
            return 1;
        }
    }
    let emoji = emoji.or_else(|| guess_emoji(title));

    let (mut list, _) = match load_list(&home) {
        Ok(v) => v,
        Err(e) => {
            ui::fail(&e);
            return 1;
        }
    };
    let id = list.next_id;
    list.next_id += 1;
    let tz_name = timezone.unwrap_or_else(|| wremind_core::util::system_timezone().to_string());
    let reminder = Reminder {
        id,
        title: title.to_string(),
        emoji,
        schedule: schedule.clone(),
        character: character.clone().filter(|c| !c.is_empty()),
        timezone: tz_name,
        enabled: true,
        completed: false,
        created: Utc::now(),
    };
    list.reminders.push(reminder);
    if let Err(e) = persist(&home, &list) {
        ui::fail(&e);
        return 1;
    }

    ui::success("Reminder created");
    println!();
    let emoji_str = list
        .reminders
        .last()
        .and_then(|r| r.emoji.clone())
        .unwrap_or_default();
    println!("  {} {} {title}", ui::bold("●"), emoji_str);
    println!("  {}", humanize(&schedule));
    match &character {
        Some(c) => println!("  Character: {c}"),
        None => {
            let cfg = Config::load(&home).unwrap_or_default();
            println!("  Character: {}", cfg.character);
        }
    }
    0
}

/// Small keyword->emoji map so reminders feel alive by default.
pub fn guess_emoji(title: &str) -> Option<String> {
    let t = title.to_lowercase();
    let rules: &[(&str, &str)] = &[
        ("water", "💧"),
        ("drink", "💧"),
        ("break", "☕"),
        ("coffee", "☕"),
        ("stretch", "🧘"),
        ("walk", "🚶"),
        ("run", "🏃"),
        ("mom", "❤️"),
        ("dad", "❤️"),
        ("call", "📞"),
        ("vitamin", "💊"),
        ("med", "💊"),
        ("stand", "🧍"),
        ("eye", "👀"),
        ("sleep", "😴"),
        ("bed", "🛏️"),
        ("lunch", "🥪"),
        ("eat", "🍽️"),
        ("football", "⚽"),
        ("soccer", "⚽"),
        ("messi", "⚽"),
    ];
    rules
        .iter()
        .find(|(kw, _)| t.contains(kw))
        .map(|(_, e)| e.to_string())
}

pub fn list() -> i32 {
    let home = home();
    let (list, recovery) = match load_list(&home) {
        Ok(v) => v,
        Err(e) => {
            ui::fail(&e);
            return 1;
        }
    };
    if let Some(note) = recovery {
        ui::warn(&note);
    }
    ui::header("Walking Reminder");
    println!();
    if list.reminders.is_empty() {
        println!("{}", ui::dim("  No reminders yet."));
        println!();
        println!("  Add one:  walking-reminder add \"Drink water\" --every 1h");
        println!();
        return 0;
    }
    let state = load_state_lossy(&home);
    let now = Utc::now();
    let cfg = Config::load(&home).unwrap_or_default();

    // column widths
    let mut rows = Vec::new();
    for r in &list.reminders {
        let status = if r.completed {
            format!("{} done", ui::DOT_ON)
        } else if r.enabled {
            ui::DOT_ON.to_string()
        } else {
            format!("{} paused", ui::DOT_OFF)
        };
        let next = if r.completed {
            String::new()
        } else {
            match next_due(r, &state, now) {
                Some(t) if t <= now => "now".to_string(),
                Some(t) => format!("Next: {}", wremind_core::util::humanize_duration_short((t - now).num_seconds())),
                None => String::new(),
            }
        };
        rows.push(vec![
            r.id.to_string(),
            format!(
                "{} {}",
                r.emoji.clone().unwrap_or_default(),
                r.title
            ),
            humanize(&r.schedule),
            next,
            status,
            r.character.clone().unwrap_or_else(|| cfg.character.clone()),
        ]);
    }

    let widths: Vec<usize> = (0..6)
        .map(|i| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(1))
        .collect();
    println!(
        "  {:<3} {:<idw$} {:<sw$} {:<nw$} {:<stw$} {}",
        ui::dim("ID"),
        ui::dim("Reminder"),
        ui::dim("Schedule"),
        ui::dim("Next"),
        ui::dim("Status"),
        ui::dim("Character"),
        idw = widths[1],
        sw = widths[2],
        nw = widths[3],
        stw = widths[4],
    );
    for r in &rows {
        println!(
            "  {:<3} {:<idw$} {:<sw$} {:<nw$} {:<stw$} {}",
            r[0], r[1], r[2], r[3], r[4], r[5],
            idw = widths[1],
            sw = widths[2],
            nw = widths[3],
            stw = widths[4],
        );
    }
    println!();
    0
}

pub fn remove(id: u64) -> i32 {
    let home = home();
    let (mut list, _) = match load_list(&home) {
        Ok(v) => v,
        Err(e) => {
            ui::fail(&e);
            return 1;
        }
    };
    let before = list.reminders.len();
    list.reminders.retain(|r| r.id != id);
    if list.reminders.len() == before {
        ui::fail(&format!("no reminder with ID {id}"));
        return 1;
    }
    if let Err(e) = persist(&home, &list) {
        ui::fail(&e);
        return 1;
    }
    ui::success(&format!("Removed reminder {id}"));
    0
}

pub fn edit(
    id: u64,
    every: Option<String>,
    at: Option<String>,
    days: Option<String>,
    daily: Option<String>,
    title: Option<String>,
    emoji: Option<String>,
    character: Option<String>,
) -> i32 {
    let home = home();
    let (mut list, _) = match load_list(&home) {
        Ok(v) => v,
        Err(e) => {
            ui::fail(&e);
            return 1;
        }
    };
    let Some(r) = list.get_mut(id) else {
        ui::fail(&format!("no reminder with ID {id}"));
        return 1;
    };
    let mut changed_schedule = false;
    if every.is_some() || at.is_some() || daily.is_some() || days.is_some() {
        let cur_every = match &r.schedule {
            Schedule::Interval(iv) => Some(if iv.seconds >= 3600 && iv.seconds % 3600 == 0 {
                format!("{}h", iv.seconds / 3600)
            } else {
                format!("{}m", iv.seconds / 60)
            }),
            _ => None,
        };
        let cur_at = match &r.schedule {
            Schedule::Daily(d) => Some(d.time.clone()),
            _ => None,
        };
        let cur_days = match &r.schedule {
            Schedule::Daily(d) if !d.days.is_empty() => Some(d.days.join(",")),
            _ => None,
        };
        let schedule = match build_schedule(
            every.as_deref().or(cur_every.as_deref()),
            at.as_deref().or(cur_at.as_deref()),
            days.as_deref().or(cur_days.as_deref()),
            None,
            None,
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                ui::fail(&e);
                return 1;
            }
        };
        r.schedule = schedule;
        changed_schedule = true;
    }
    if let Some(t) = &title {
        r.title = t.clone();
    }
    if emoji.is_some() {
        r.emoji = emoji;
    }
    if character.is_some() {
        r.character = character.filter(|c| !c.is_empty());
    }
    // Reset the anchor when the schedule changed so the new cadence
    // applies from now.
    if changed_schedule {
        let mut state = load_state_lossy(&home);
        state.last_fired.remove(&id);
        let _ = wremind_core::store::save_state(&home, &state);
    }
    if let Err(e) = persist(&home, &list) {
        ui::fail(&e);
        return 1;
    }
    ui::success(&format!("Updated reminder {id}"));
    0
}

pub fn pause(id: Option<u64>, resume: bool) -> i32 {
    let home = home();
    let (mut list, _) = match load_list(&home) {
        Ok(v) => v,
        Err(e) => {
            ui::fail(&e);
            return 1;
        }
    };
    let mut changed = 0;
    // pause (resume=false) -> enabled=false; resume -> enabled=true
    let target = resume;
    for r in list.reminders.iter_mut() {
        if id.is_none() || Some(r.id) == id {
            if r.enabled != target {
                r.enabled = target;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        ui::fail("nothing to change");
        return 1;
    }
    if let Err(e) = persist(&home, &list) {
        ui::fail(&e);
        return 1;
    }
    let what = if resume { "Resumed" } else { "Paused" };
    ui::success(&format!("{what} {changed} reminder(s)"));
    0
}

// ---------------------------------------------------------------------------
// daemon lifecycle
// ---------------------------------------------------------------------------

pub fn daemon() -> i32 {
    let bin = std::env::current_exe().unwrap_or_else(|_| "walking-reminder".into());
    match wremind_daemon::run(bin) {
        Ok(()) => 0,
        Err(e) => {
            ui::fail(&e.to_string());
            1
        }
    }
}

pub fn start() -> i32 {
    let home = home();
    // Already running?
    if let Ok(resp) = wremind_daemon::ipc::send(
        &home.socket_file(),
        &wremind_daemon::ipc::Request::Ping,
    ) {
        if resp.ok {
            ui::success(&format!("Daemon already running (pid {})", resp.info.map(|i| i.pid).unwrap_or(0)));
            return 0;
        }
    }
    let bin = std::env::current_exe().unwrap_or_else(|_| "walking-reminder".into());
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("__daemon");
    // Detach: new session, no tty, no inherited stdio -> survives terminal close.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id();
            drop(child);
            // Wait for the socket to come up.
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let Ok(resp) = wremind_daemon::ipc::send(
                    &home.socket_file(),
                    &wremind_daemon::ipc::Request::Ping,
                ) {
                    if resp.ok {
                        ui::success(&format!("Daemon started (pid {pid})"));
                        return 0;
                    }
                }
            }
            ui::success(&format!("Daemon starting (pid {pid})"));
            0
        }
        Err(e) => {
            ui::fail(&format!("failed to start daemon: {e}"));
            1
        }
    }
}

pub fn stop() -> i32 {
    let home = home();
    match wremind_daemon::ipc::send(
        &home.socket_file(),
        &wremind_daemon::ipc::Request::Stop,
    ) {
        Ok(resp) if resp.ok => {
            // Wait for the daemon to notice the flag (<= 1 tick) and close
            // its socket, so `status` right after `stop` is accurate.
            for _ in 0..30 {
                if !home.socket_file().exists() {
                    break;
                }
                let still_alive = wremind_daemon::ipc::send(
                    &home.socket_file(),
                    &wremind_daemon::ipc::Request::Ping,
                )
                .map(|r| r.ok)
                .unwrap_or(false);
                if !still_alive {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            ui::success("Daemon stopped");
            0
        }
        Ok(resp) => {
            ui::fail(resp.error.as_deref().unwrap_or("stop failed"));
            1
        }
        Err(_) => {
            ui::warn("Daemon is not running");
            0
        }
    }
}

pub fn status() -> i32 {
    let home = home();
    let (list, _) = load_list(&home).unwrap_or_default();
    ui::header("Walking Reminder");
    println!();
    let ping = wremind_daemon::ipc::send(
        &home.socket_file(),
        &wremind_daemon::ipc::Request::Ping,
    )
    .ok()
    .filter(|r| r.ok);
    match ping.and_then(|r| r.info) {
        Some(info) => {
            let next = match (&info.next_title, info.next_in_secs) {
                (Some(t), Some(s)) => format!("{t} in {}", wremind_core::util::humanize_duration_short(s)),
                _ => "—".to_string(),
            };
            println!("Daemon:     {}", ui::green("running"));
            println!("PID:        {}", info.pid);
            println!("Reminders:  {} ({} enabled)", info.reminders, info.enabled_reminders);
            println!("Character:  {}", info.character);
            println!("Next:       {next}");
        }
        None => {
            println!("Daemon:     {}", ui::yellow("stopped"));
            let state = load_state_lossy(&home);
            let now = Utc::now();
            match next_upcoming(list.reminders.iter(), &state, now) {
                Some((r, t)) => println!(
                    "Next:       {} in {} (start the daemon!)",
                    r.title,
                    wremind_core::util::humanize_duration_short((t - now).num_seconds())
                ),
                None => println!("Next:       —"),
            }
            let startup = wremind_daemon::startup::status();
            println!(
                "Autostart:  {}",
                if startup.enabled { ui::green("enabled") } else { ui::dim("disabled") }
            );
        }
    }
    println!();
    0
}

pub fn enable(on: bool) -> i32 {
    let bin = std::env::current_exe().unwrap_or_else(|_| "walking-reminder".into());
    let result = if on {
        wremind_daemon::startup::enable(&bin)
    } else {
        wremind_daemon::startup::disable()
    };
    match result {
        Ok(()) => {
            if on {
                ui::success("Will start automatically at login");
                // Also start it right now if not running.
                start()
            } else {
                ui::success("Automatic startup disabled");
                0
            }
        }
        Err(e) => {
            ui::fail(&e);
            1
        }
    }
}

// ---------------------------------------------------------------------------
// test / render
// ---------------------------------------------------------------------------

pub fn test(message: Option<String>, character: Option<String>, direction: Option<String>) -> i32 {
    let Ok(home) = ensure_ready() else {
        return 1;
    };
    let message = message.unwrap_or_else(|| "🌊 Just walking by to say hi!".to_string());
    let character_name = character.unwrap_or_else(|| {
        Config::load(&home).unwrap_or_default().character
    });

    // Validate the character up front for a good error message.
    if let Err(e) = Character::load(&home, &character_name) {
        ui::fail(&format!("character '{character_name}': {e}"));
        return 1;
    }

    let dir = direction
        .as_deref()
        .and_then(wremind_render::Direction::parse)
        .unwrap_or_else(wremind_render::Direction::random);

    // If the daemon is running, route through it so the queue works.
    if let Ok(resp) = wremind_daemon::ipc::send(
        &home.socket_file(),
        &wremind_daemon::ipc::Request::Show {
            message: message.clone(),
            character: Some(character_name.clone()),
        },
    ) {
        if resp.ok {
            ui::success("Showing character preview...");
            return 0;
        }
    }

    // Otherwise spawn the renderer directly (fire and forget).
    let bin = std::env::current_exe().unwrap_or_else(|_| "walking-reminder".into());
    let mut cmd = std::process::Command::new(bin);
    cmd.args(["__render", "--message", &message, "--character", &character_name]);
    match dir {
        wremind_render::Direction::Left => {
            cmd.args(["--direction", "left"]);
        }
        wremind_render::Direction::Right => {
            cmd.args(["--direction", "right"]);
        }
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    match cmd.spawn() {
        Ok(_child) => {
            ui::success("Showing character preview...");
            0
        }
        Err(e) => {
            ui::fail(&format!("renderer failed to start: {e}"));
            1
        }
    }
}

pub fn render(
    message: &str,
    character: Option<String>,
    direction: Option<String>,
    duration: Option<f64>,
    no_render: bool,
    screenshot: Option<std::path::PathBuf>,
) -> i32 {
    if no_render {
        // Headless validation for tests/CI.
        let home = Home::resolve();
        let _ = home.ensure();
        let _ = Character::ensure_default(&home);
        return 0;
    }
    let dir = direction
        .as_deref()
        .and_then(wremind_render::Direction::parse);
    match wremind_render::run_show_screenshot(
        message,
        character.as_deref(),
        dir,
        duration,
        screenshot,
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("render error: {e}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

pub fn config(op: ConfigOp) -> i32 {
    let home = home();
    let mut cfg = Config::load(&home).unwrap_or_default();
    match op {
        ConfigOp::Get => {
            ui::header("Configuration");
            println!();
            println!("  character        = {}", cfg.character);
            println!(
                "  speed            = {}",
                cfg.speed.map(|v| v.to_string()).unwrap_or_else(|| "character default".into())
            );
            println!(
                "  size             = {}",
                cfg.size.map(|v| v.to_string()).unwrap_or_else(|| "character default".into())
            );
            println!("  sound            = {}", cfg.sound);
            println!(
                "  duration         = {}",
                cfg.duration
                    .map(|v| format!("{v}s"))
                    .unwrap_or_else(|| "auto (6-12s)".into())
            );
            println!("  max_simultaneous = {}", cfg.max_simultaneous);
            println!("  direction        = {}", cfg.direction);
            println!("  log_level        = {}", cfg.log_level);
            println!();
            println!("  {}", ui::dim(&format!("file: {}", home.config_file().display())));
            0
        }
        ConfigOp::Set { key, value } => {
            if let Err(e) = cfg.set(&key, &value) {
                ui::fail(&e);
                return 1;
            }
            if let Err(e) = cfg.save(&home) {
                ui::fail(&e.to_string());
                return 1;
            }
            ui::success(&format!("{key} = {value}"));
            0
        }
        ConfigOp::Reset { key } => {
            match key.as_deref() {
                None => cfg = Config::default(),
                Some("speed") => cfg.speed = None,
                Some("size") => cfg.size = None,
                Some("sound") => cfg.sound = false,
                Some("duration") => cfg.duration = None,
                Some("character") => cfg.character = DEFAULT_CHARACTER.into(),
                Some("max_simultaneous") => cfg.max_simultaneous = 1,
                Some("direction") => cfg.direction = "random".into(),
                Some(k) => {
                    ui::fail(&format!("unknown key '{k}'"));
                    return 1;
                }
            }
            if let Err(e) = cfg.save(&home) {
                ui::fail(&e.to_string());
                return 1;
            }
            ui::success("Configuration reset");
            0
        }
    }
}

// ---------------------------------------------------------------------------
// character
// ---------------------------------------------------------------------------

pub fn character(op: CharacterOp) -> i32 {
    let home = home();
    match op {
        CharacterOp::List => {
            let _ = home.ensure();
            let _ = Character::ensure_default(&home);
            let installed = Character::list_installed(&home);
            let cfg = Config::load(&home).unwrap_or_default();
            ui::header("Available characters");
            println!();
            for (name, desc) in &installed {
                let active = if *name == cfg.character {
                    format!("{} ", ui::green(ui::CHECK))
                } else {
                    "  ".to_string()
                };
                println!("{active}{name}");
                if !desc.is_empty() {
                    println!("    {}", ui::dim(desc));
                }
            }
            println!();
            println!(
                "  {}",
                ui::dim("import your own: walking-reminder character import ./my-character/")
            );
            0
        }
        CharacterOp::Set { name } => {
            let _ = Character::ensure_default(&home);
            if let Err(e) = Character::load(&home, &name) {
                ui::fail(&format!("{e}"));
                return 1;
            }
            let mut cfg = Config::load(&home).unwrap_or_default();
            cfg.character = name.clone();
            if let Err(e) = cfg.save(&home) {
                ui::fail(&e.to_string());
                return 1;
            }
            ui::success(&format!("Active character: {name}"));
            println!(
                "  {}",
                ui::dim("preview: walking-reminder test --character " ) // trailing space intentional
            );
            0
        }
        CharacterOp::Show { name } => {
            match Character::load(&home, &name) {
                Ok(c) => {
                    ui::header(&format!("Character: {name}"));
                    println!();
                    println!("  name         = {}", c.def.name);
                    println!("  description  = {}", c.def.description);
                    println!("  frame rate   = {} fps", c.def.frame_rate);
                    println!("  size         = {}x{} px", c.def.width, c.def.height);
                    println!("  walk speed   = {} px/s", c.def.walking_speed);
                    println!("  frames       = {}", c.frame_paths.len());
                    println!("  left frames  = {}", c.left_frame_paths.len());
                    println!("  directory    = {}", c.dir.display());
                    0
                }
                Err(e) => {
                    ui::fail(&e.to_string());
                    1
                }
            }
        }
        CharacterOp::Import { path } => {
            if !path.is_dir() {
                ui::fail(&format!("{} is not a directory", path.display()));
                return 1;
            }
            let validated = match Character::validate_dir(&path) {
                Ok(c) => c,
                Err(e) => {
                    ui::fail(&format!("invalid character: {e}"));
                    return 1;
                }
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "custom".into());
            let dest = home.character_dir(&name);
            if let Err(e) = copy_dir(&path, &dest) {
                ui::fail(&e);
                return 1;
            }
            ui::success(&format!("Imported character '{name}'"));
            println!("  {} frames, {} fps", validated.frame_paths.len(), validated.def.frame_rate);
            println!("  {}", ui::dim(&format!("try: walking-reminder character set {name}")));
            0
        }
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let to = dst.join(entry.file_name());
        std::fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// desktop pet
// ---------------------------------------------------------------------------

pub fn pet(character: Option<String>, stop: bool) -> i32 {
    let home = home();
    if stop {
        let pid_file = home.root().join("pet.pid");
        match std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            Some(pid) => {
                let _ = std::process::Command::new("kill").arg(pid.to_string()).output();
                let _ = std::fs::remove_file(&pid_file);
                ui::success("Desktop pet stopped");
                return 0;
            }
            None => {
                ui::warn("No desktop pet running");
                return 0;
            }
        }
    }

    // Replace any existing pet.
    if let Some(pid) = std::fs::read_to_string(home.root().join("pet.pid"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).output();
    }

    let Ok(home) = ensure_ready() else {
        return 1;
    };
    let cfg = Config::load(&home).unwrap_or_default();
    let name = character.unwrap_or_else(|| cfg.character.clone());
    if let Err(e) = Character::load(&home, &name) {
        ui::fail(&format!("character '{name}': {e}"));
        return 1;
    }

    let bin = std::env::current_exe().unwrap_or_else(|_| "walking-reminder".into());
    let mut cmd = std::process::Command::new(bin);
    cmd.args(["__pet", "--character", &name]);
    if std::env::var_os("WALKING_REMINDER_NO_RENDER").is_some() {
        cmd.arg("--no-render");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // fully detach from this terminal
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match cmd.spawn() {
        Ok(_child) => {
            ui::success("Desktop pet is on your screen — drag him anywhere");
            println!("{}", ui::dim("  right-click him to dismiss · `walking-reminder pet --stop` also works"));
            0
        }
        Err(e) => {
            ui::fail(&format!("failed to start pet: {e}"));
            1
        }
    }
}

pub fn pet_process(character: Option<String>, no_render: bool) -> i32 {
    match wremind_render::run_pet(character.as_deref(), no_render) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("pet error: {e}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// doctor / logs
// ---------------------------------------------------------------------------

pub fn doctor() -> i32 {
    let home = home();
    ui::header("Walking Reminder — health check");
    println!();
    let mut bad = 0;

    let home_ok = home.ensure().is_ok();
    report(
        home_ok,
        &format!("home directory ({})", home.root().display()),
        "cannot create ~/.walking-reminder",
    );
    bad += !home_ok as i32;

    let chars_ok = Character::ensure_default(&home).is_ok();
    let char_load = Character::load(&home, DEFAULT_CHARACTER).is_ok();
    report(
        chars_ok && char_load,
        "default character (footballer) installed and valid",
        "character assets missing/corrupt",
    );
    bad += !(chars_ok && char_load) as i32;

    let (list, recovery) = load_list(&home).unwrap_or_default();
    report(true, &format!("reminders.json ({} reminders)", list.reminders.len()), "");
    if let Some(note) = recovery {
        ui::warn(&note);
    }

    let cfg_ok = Config::load(&home).is_ok();
    report(cfg_ok, "config.json readable", "config corrupted; run walking-reminder config reset");
    bad += !cfg_ok as i32;

    let running = wremind_daemon::ipc::send(
        &home.socket_file(),
        &wremind_daemon::ipc::Request::Ping,
    )
    .ok()
    .filter(|r| r.ok)
    .and_then(|r| r.info);
    match &running {
        Some(info) => {
            report(true, &format!("daemon running (pid {})", info.pid), "");
        }
        None => {
            ui::warn("daemon not running — `walking-reminder start`");
        }
    }

    let startup = wremind_daemon::startup::status();
    report(
        true,
        &format!(
            "autostart: {} {}",
            if startup.enabled { "enabled" } else { "disabled" },
            ui::dim(&startup.detail)
        ),
        "",
    );

    report(
        true,
        "permissions: none required (overlay uses no Screen Recording / Accessibility)",
        "",
    );

    println!();
    if bad == 0 {
        ui::success("All good");
        0
    } else {
        ui::fail(&format!("{bad} problem(s) found"));
        1
    }
}

fn report(ok: bool, what: &str, hint: &str) {
    if ok {
        println!("  {} {}", ui::green(ui::CHECK), what);
    } else {
        println!("  {} {} {}", ui::yellow("!"), what, ui::dim(hint));
    }
}

pub fn logs(lines: usize) -> i32 {
    let home = home();
    let path = home.log_file();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let all: Vec<&str> = content.lines().collect();
            let start = all.len().saturating_sub(lines);
            for line in &all[start..] {
                println!("{line}");
            }
            0
        }
        Err(_) => {
            println!("{}", ui::dim("no logs yet (the daemon writes here when it runs)"));
            0
        }
    }
}
