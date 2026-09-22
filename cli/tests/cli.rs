//! End-to-end CLI tests (requirement 25): every major command runs
//! against a temporary WALKING_REMINDER_HOME. The real binary is used
//! (CARGO_BIN_EXE_walking-reminder), including a full daemon
//! start/trigger/stop cycle with rendering suppressed
//! (WALKING_REMINDER_NO_RENDER=1) so CI stays headless.

use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::str::{contains, is_empty};
use predicates::boolean::PredicateBooleanExt;

fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_walking-reminder"));
    // Headless: the daemon spawns renderers as `--no-render`.
    cmd.env("WALKING_REMINDER_NO_RENDER", "1");
    cmd
}

struct TempHome(tempfile::TempDir);

impl TempHome {
    fn new() -> TempHome {
        let dir = tempfile::tempdir().expect("tempdir");
        TempHome(dir)
    }
    fn cmd(&self) -> Command {
        let mut c = bin();
        c.env("WALKING_REMINDER_HOME", self.0.path());
        c
    }
    fn path(&self) -> &std::path::Path {
        self.0.path()
    }
    fn reminders_json(&self) -> String {
        std::fs::read_to_string(self.path().join("reminders.json")).unwrap_or_default()
    }
}

#[test]
fn version_works() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("walking-reminder"));
}

#[test]
fn add_interval_reminder() {
    let h = TempHome::new();
    h.cmd()
        .args(["add", "Drink water", "--every", "1h"])
        .assert()
        .success()
        .stdout(contains("Reminder created"))
        .stdout(contains("Drink water"))
        .stdout(contains("Every 1 hour"));
    let json = h.reminders_json();
    assert!(json.contains("\"minutes\": 60"), "got: {json}");
    assert!(json.contains("\"type\": \"interval\""));
    assert!(json.contains("💧"), "emoji should be auto-guessed");
}

#[test]
fn add_daily_reminder() {
    let h = TempHome::new();
    h.cmd()
        .args(["add", "Take vitamins", "--daily", "09:00"])
        .assert()
        .success()
        .stdout(contains("Daily"));
    let json = h.reminders_json();
    assert!(json.contains("\"type\": \"daily\""));
    assert!(json.contains("\"time\": \"09:00\""));
}

#[test]
fn add_weekly_reminder() {
    let h = TempHome::new();
    h.cmd()
        .args([
            "add",
            "Go running",
            "--days",
            "mon,wed,fri",
            "--at",
            "18:00",
        ])
        .assert()
        .success()
        .stdout(contains("Mon/Wed/Fri"));
    let json = h.reminders_json();
    assert!(json.contains("\"mon\""));
    assert!(json.contains("\"fri\""));
}

#[test]
fn add_one_time_reminder() {
    let h = TempHome::new();
    h.cmd()
        .args(["add", "Call Mom", "--at", "2026-09-22 19:00", "--emoji", "❤️"])
        .assert()
        .success();
    let json = h.reminders_json();
    assert!(json.contains("\"type\": \"once\""), "got: {json}");
}

#[test]
fn add_rejects_bad_schedule() {
    let h = TempHome::new();
    h.cmd()
        .args(["add", "Broken", "--every", "fast"])
        .assert()
        .failure()
        .stderr(contains("invalid --every"));
    h.cmd()
        .args(["add", "Broken"])
        .assert()
        .failure();
}

#[test]
fn list_remove_pause_resume_edit() {
    let h = TempHome::new();
    h.cmd().args(["add", "One", "--every", "1h"]).assert().success();
    h.cmd().args(["add", "Two", "--every", "2h"]).assert().success();

    h.cmd()
        .arg("list")
        .assert()
        .success()
        .stdout(contains("One"))
        .stdout(contains("Two"))
        .stdout(contains("Every 2 hours"));

    // pause one
    h.cmd().args(["pause", "1"]).assert().stdout(contains("Paused"));
    h.cmd().arg("list").assert().stdout(contains("paused"));

    // resume
    h.cmd().args(["resume", "1"]).assert().stdout(contains("Resumed"));

    // edit
    h.cmd()
        .args(["edit", "2", "--title", "Two changed", "--every", "90m"])
        .assert()
        .stdout(contains("Updated"));
    h.cmd()
        .arg("list")
        .assert()
        .stdout(contains("Two changed"))
        .stdout(contains("1 hour 30 minutes"));

    // remove
    h.cmd().args(["remove", "1"]).assert().stdout(contains("Removed"));
    h.cmd().arg("list").assert().stdout(contains("Two changed"));
    h.cmd().args(["remove", "1"]).assert().failure(); // already gone
}

#[test]
fn config_get_set_reset() {
    let h = TempHome::new();
    h.cmd()
        .args(["config", "set", "speed", "180"])
        .assert()
        .success();
    h.cmd()
        .args(["config", "set", "character", "footballer"])
        .assert()
        .success();
    h.cmd().arg("config").args(["get"]).assert().stdout(contains("speed            = 180"));
    h.cmd()
        .args(["config", "set", "bogus", "1"])
        .assert()
        .failure()
        .stderr(contains("unknown config key"));
    h.cmd().args(["config", "reset"]).assert().success();
    h.cmd().arg("config").args(["get"]).assert().stdout(contains("character default"));
}

#[test]
fn character_list_set_show() {
    let h = TempHome::new();
    h.cmd().arg("character").arg("list").assert().stdout(contains("footballer"));
    h.cmd()
        .args(["character", "set", "footballer"])
        .assert()
        .success();
    h.cmd()
        .args(["character", "set", "ghost"])
        .assert()
        .failure()
        .stderr(contains("not found"));
    h.cmd()
        .args(["character", "show", "footballer"])
        .assert()
        .stdout(contains("frame rate"))
        .stdout(contains("frames"));
}

#[test]
fn character_import_rejects_invalid() {
    let h = TempHome::new();
    let bad = h.path().join("badchar");
    std::fs::create_dir_all(&bad).unwrap();
    h.cmd()
        .args(["character", "import"])
        .arg(&bad)
        .assert()
        .failure()
        .stderr(contains("invalid character"));
}

#[test]
fn status_stopped_then_started() {
    let h = TempHome::new();
    h.cmd().args(["add", "Water", "--every", "1h"]).assert().success();
    h.cmd()
        .arg("status")
        .assert()
        .stdout(contains("stopped"))
        .stdout(contains("Water"));

    h.cmd().arg("start").assert().stdout(contains("Daemon"));
    h.cmd().arg("status").assert().stdout(contains("running"));
    h.cmd().arg("stop").assert().stdout(contains("stopped").or(contains("Daemon stopped")));
    h.cmd().arg("status").assert().stdout(contains("stopped"));
}

#[test]
fn test_command_spawns_renderer_headless() {
    let h = TempHome::new();
    h.cmd()
        .args(["test", "--message", "hi there", "--character", "footballer"])
        .assert()
        .success()
        .stdout(contains("preview"));
}

#[test]
fn daemon_fires_interval_reminder() {
    let h = TempHome::new();
    h.cmd().args(["add", "Tick", "--every", "2s"]).assert().success();
    h.cmd().arg("start").assert().success();
    std::thread::sleep(std::time::Duration::from_millis(4500));
    h.cmd().arg("stop").assert().success();

    let state =
        std::fs::read_to_string(h.path().join("state.json")).expect("state.json");
    assert!(state.contains("\"last_fired\""), "state: {state}");

    let log = std::fs::read_to_string(h.path().join("logs/walking-reminder.log"))
        .expect("daemon log");
    assert!(
        log.contains("reminder 1 triggered: ⏱ Tick") || log.contains("triggered"),
        "log: {log}"
    );
}

#[test]
fn daemon_single_instance() {
    let h = TempHome::new();
    h.cmd().arg("start").assert().success();
    // Second start is idempotent.
    h.cmd().arg("start").assert().stdout(contains("already running"));
    h.cmd().arg("stop").assert().success();
}

#[test]
fn doctor_passes_on_fresh_install() {
    let h = TempHome::new();
    h.cmd()
        .arg("doctor")
        .assert()
        .stdout(contains("footballer"))
        .stdout(contains("permissions"));
}

#[test]
fn logs_command() {
    let h = TempHome::new();
    // No logs yet: should not fail.
    h.cmd().arg("logs").assert().stdout(is_empty().or(contains("no logs yet")));
}

#[test]
fn reminders_survive_restart() {
    let h = TempHome::new();
    h.cmd().args(["add", "Persist", "--every", "45m"]).assert().success();
    h.cmd().arg("start").assert().success();
    h.cmd().arg("stop").assert().success();
    h.cmd().arg("start").assert().success();
    h.cmd()
        .arg("status")
        .assert()
        .stdout(contains("Reminders:  1"));
    h.cmd().arg("stop").assert().success();
}

#[test]
fn pet_command_and_stop() {
    let h = TempHome::new();
    // Headless (WALKING_REMINDER_NO_RENDER=1 is inherited from bin()):
    // the pet process validates and exits; pid file semantics still work.
    h.cmd().args(["pet"]).assert().success();
    h.cmd().args(["pet", "--stop"]).assert().stdout(contains("pet"));
}

#[test]
fn no_render_flag_validates_headless() {
    let h = TempHome::new();
    let mut c = h.cmd();
    c.env_remove("WALKING_REMINDER_NO_RENDER");
    c.args([
        "__render",
        "--message",
        "x",
        "--no-render",
    ])
    .assert()
    .success();
}
