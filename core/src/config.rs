//! User configuration (`config.json`).

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::paths::{write_atomic, Home};

/// Renderer/behavior configuration. All values optional so character
/// defaults win unless the user overrides them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    /// Active character name (default "footballer").
    #[serde(default = "default_character")]
    pub character: String,
    /// Walking speed override in px/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u32>,
    /// Character size override in px.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    /// Play a soft sound with each reminder (default false).
    #[serde(default)]
    pub sound: bool,
    /// Animation duration in seconds; None = auto (fit screen crossing,
    /// clamped to 6..12s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// Max simultaneous walking animations (default 1; extras queue).
    #[serde(default = "default_max_simul")]
    pub max_simultaneous: u32,
    /// Reminder direction: "random" (default), "left", or "right".
    #[serde(default = "default_direction")]
    pub direction: String,
    /// Log level for the daemon: "error" | "info" | "debug".
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_character() -> String {
    "footballer".into()
}
fn default_max_simul() -> u32 {
    1
}
fn default_direction() -> String {
    "random".into()
}
fn default_log_level() -> String {
    "info".into()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: 1,
            character: default_character(),
            speed: None,
            size: None,
            sound: false,
            duration: None,
            max_simultaneous: default_max_simul(),
            direction: default_direction(),
            log_level: default_log_level(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid config JSON: {0}")]
    Parse(String),
}

impl Config {
    pub fn load(home: &Home) -> Result<Config, ConfigError> {
        let path = home.config_file();
        Self::load_path(&path)
    }

    pub fn load_path(path: &Path) -> Result<Config, ConfigError> {
        let bytes = std::fs::read(path)?;
        let mut cfg: Config = serde_json::from_slice(&bytes).map_err(|e| ConfigError::Parse(e.to_string()))?;
        cfg.normalize();
        Ok(cfg)
    }

    pub fn save(&self, home: &Home) -> Result<(), ConfigError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| ConfigError::Parse(e.to_string()))?;
        write_atomic(&home.config_file(), json.as_bytes())?;
        Ok(())
    }

    pub fn normalize(&mut self) {
        if self.character.trim().is_empty() {
            self.character = default_character();
        }
        if self.max_simultaneous == 0 {
            self.max_simultaneous = 1;
        }
        if self.max_simultaneous > 4 {
            self.max_simultaneous = 4;
        }
        let d = self.direction.to_lowercase();
        if !matches!(d.as_str(), "random" | "left" | "right") {
            self.direction = "random".into();
        }
    }

    /// Apply a `config set <key> <value>` operation. Returns Err with a
    /// friendly message on unknown keys or bad values.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "character" => {
                self.character = value.to_string();
            }
            "speed" => {
                self.speed = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid speed '{value}' (expected px/sec number)"))?,
                );
            }
            "size" => {
                self.size = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid size '{value}' (expected px number)"))?,
                );
            }
            "sound" => {
                self.sound = parse_bool(value).ok_or(format!("invalid sound '{value}' (true/false)"))?;
            }
            "duration" => {
                let v: f64 = value
                    .parse()
                    .map_err(|_| format!("invalid duration '{value}' (seconds)"))?;
                if !(3.0..=60.0).contains(&v) {
                    return Err("duration must be between 3 and 60 seconds".into());
                }
                self.duration = Some(v);
            }
            "max_simultaneous" | "max-simultaneous" => {
                let v: u32 = value.parse().map_err(|_| "expected 1-4".to_string())?;
                self.max_simultaneous = v.clamp(1, 4);
            }
            "direction" => {
                let d = value.to_lowercase();
                if !matches!(d.as_str(), "random" | "left" | "right") {
                    return Err("direction must be random, left or right".into());
                }
                self.direction = d;
            }
            "log_level" | "log-level" => {
                let l = value.to_lowercase();
                if !matches!(l.as_str(), "error" | "info" | "debug") {
                    return Err("log_level must be error, info or debug".into());
                }
                self.log_level = l;
            }
            other => {
                return Err(format!(
                    "unknown config key '{other}'. Keys: character, speed, size, sound, duration, max_simultaneous, direction, log_level"
                ));
            }
        }
        Ok(())
    }
}

pub fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip() {
        let c = Config::default();
        let json = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.character, "footballer");
        assert_eq!(back.max_simultaneous, 1);
    }

    #[test]
    fn set_keys() {
        let mut c = Config::default();
        c.set("speed", "180").unwrap();
        c.set("sound", "false").unwrap();
        assert_eq!(c.speed, Some(180));
        assert!(!c.sound);
        assert!(c.set("bogus", "1").is_err());
        assert!(c.set("speed", "fast").is_err());
    }
}
