//! OS-native autostart integration (requirement 16):
//! - macOS: LaunchAgent in ~/Library/LaunchAgents (no admin needed)
//! - Linux: XDG autostart desktop entry (~/.config/autostart)
//! - Windows: per-user Startup folder shortcut (stub — documented)

use std::path::PathBuf;

pub const LABEL: &str = "com.dribble.daemon";

pub struct StartupStatus {
    pub enabled: bool,
    pub detail: String,
}

#[cfg(target_os = "macos")]
pub fn enable(bin: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let plist = launchagent_path()?;
    let args = plist_args(bin);
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
{}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        args,
        log = crate::startup::log_path().display()
    );
    if let Some(dir) = plist.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&plist, content).map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(&plist, std::fs::Permissions::from_mode(0o644));
    // Best-effort load (skipped in tests via env guard).
    if std::env::var_os("WR_SKIP_LAUNCHCTL").is_none() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap_or_default()])
            .output();
        let _ = std::process::Command::new("launchctl")
            .args(["load", plist.to_str().unwrap_or_default()])
            .output();
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn plist_args(bin: &std::path::Path) -> String {
    format!(
        "        <string>{}</string>\n        <string>__daemon</string>",
        bin.display()
    )
}

#[cfg(target_os = "macos")]
pub fn disable() -> Result<(), String> {
    let plist = launchagent_path()?;
    if std::env::var_os("WR_SKIP_LAUNCHCTL").is_none() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap_or_default()])
            .output();
    }
    let _ = std::fs::remove_file(&plist);
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn status() -> StartupStatus {
    let plist = launchagent_path().unwrap_or_else(|_| PathBuf::from(LABEL));
    let enabled = plist.exists();
    StartupStatus {
        enabled,
        detail: plist.display().to_string(),
    }
}

#[cfg(target_os = "macos")]
pub fn launchagent_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

pub fn log_path() -> PathBuf {
    let home = wremind_core::paths::Home::resolve();
    home.logs_dir().join("daemon.log")
}

#[cfg(target_os = "linux")]
pub fn enable(bin: &std::path::Path) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    let dir = PathBuf::from(home).join(".config/autostart");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("dribble.desktop");
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=Dribble\nExec={} __daemon\nX-GNOME-Autostart-enabled=true\n",
        bin.display()
    );
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
pub fn disable() -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    let _ = std::fs::remove_file(
        PathBuf::from(home)
            .join(".config/autostart/dribble.desktop"),
    );
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn status() -> StartupStatus {
    let home = std::env::var_os("HOME").unwrap_or_default();
    let path = PathBuf::from(home).join(".config/autostart/dribble.desktop");
    StartupStatus { enabled: path.exists(), detail: path.display().to_string() }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn enable(_bin: &std::path::Path) -> Result<(), String> {
    Err("automatic startup is not implemented for this platform yet".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn disable() -> Result<(), String> {
    Err("automatic startup is not implemented for this platform yet".into())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn status() -> StartupStatus {
    StartupStatus { enabled: false, detail: "unsupported platform".into() }
}
