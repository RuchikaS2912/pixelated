//! Filesystem layout: everything lives under `~/.walking-reminder/`
//! (overridable with `WALKING_REMINDER_HOME` for tests and sandboxes).
//!
//! ```text
//! ~/.walking-reminder/
//! ├── config.json
//! ├── reminders.json
//! ├── state.json
//! ├── characters/
//! ├── logs/
//! ├── daemon.sock
//! └── daemon.lock
//! ```

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// Resolve the home directory. `WALKING_REMINDER_HOME` overrides the
    /// default `~/.walking-reminder` (used by tests and power users).
    pub fn resolve() -> Home {
        Home::from_root(
            std::env::var_os("WALKING_REMINDER_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(default_root),
        )
    }

    pub fn from_root(root: PathBuf) -> Home {
        Home { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }
    pub fn reminders_file(&self) -> PathBuf {
        self.root.join("reminders.json")
    }
    pub fn state_file(&self) -> PathBuf {
        self.root.join("state.json")
    }
    pub fn characters_dir(&self) -> PathBuf {
        self.root.join("characters")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn log_file(&self) -> PathBuf {
        self.logs_dir().join("walking-reminder.log")
    }
    pub fn socket_file(&self) -> PathBuf {
        self.root.join("daemon.sock")
    }
    pub fn lock_file(&self) -> PathBuf {
        self.root.join("daemon.lock")
    }
    pub fn pid_file(&self) -> PathBuf {
        self.root.join("daemon.pid")
    }

    pub fn character_dir(&self, name: &str) -> PathBuf {
        self.characters_dir().join(name)
    }

    /// Create the directory skeleton if missing. Idempotent.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.characters_dir())?;
        std::fs::create_dir_all(self.logs_dir())?;
        if !self.config_file().exists() {
            self.save_if_absent_config()?;
        }
        Ok(())
    }

    fn save_if_absent_config(&self) -> std::io::Result<()> {
        let cfg = crate::config::Config::default();
        let json = serde_json::to_string_pretty(&cfg).map_err(io_err)?;
        write_atomic(&self.config_file(), json.as_bytes())
    }
}

pub(crate) fn io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

pub(crate) fn default_root() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".walking-reminder")
}

/// Atomic file write: write to a temp file in the same directory, then rename.
/// Prevents corrupted files if the process dies mid-write.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    std::fs::write(&tmp, bytes)?;
    // fsync for durability before rename
    #[cfg(unix)]
    {
        
        if let Ok(f) = std::fs::File::open(&tmp) {
            let _ = f.sync_all();
        }
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}
