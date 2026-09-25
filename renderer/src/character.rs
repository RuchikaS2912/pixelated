//! Character loading and validation.
//!
//! A character is a directory:
//!
//! ```text
//! characters/footballer/
//! ├── character.json
//! ├── walk_01.png .. walk_NN.png      (right-facing)
//! └── walk_left_01.png .. (optional left-facing; right frames are
//!                          mirrored in code if absent)
//! ```
//!
//! `character.json`:
//! ```json
//! {
//!   "name": "footballer",
//!   "description": "Default football character",
//!   "frameRate": 10,
//!   "width": 120,
//!   "height": 120,
//!   "walkingSpeed": 180
//! }
//! ```
//!
//! Nothing about a character is hard-coded in the engine — drop a new
//! folder (e.g. an authorized `messi/` pack) and it works.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use wremind_core::paths::Home;

pub const DEFAULT_CHARACTER: &str = "footballer";

/// The bundled default character, embedded in the binary so a fresh
/// install works offline with zero extra files.
pub const BUNDLED_FILES: &[(&str, &[u8])] = &[
    ("character.json", include_bytes!("../../characters/default-footballer/character.json")),
    ("walk_01.png", include_bytes!("../../characters/default-footballer/walk_01.png")),
    ("walk_02.png", include_bytes!("../../characters/default-footballer/walk_02.png")),
    ("walk_03.png", include_bytes!("../../characters/default-footballer/walk_03.png")),
    ("walk_04.png", include_bytes!("../../characters/default-footballer/walk_04.png")),
    ("walk_05.png", include_bytes!("../../characters/default-footballer/walk_05.png")),
    ("walk_06.png", include_bytes!("../../characters/default-footballer/walk_06.png")),
    ("walk_left_01.png", include_bytes!("../../characters/default-footballer/walk_left_01.png")),
    ("walk_left_02.png", include_bytes!("../../characters/default-footballer/walk_left_02.png")),
    ("walk_left_03.png", include_bytes!("../../characters/default-footballer/walk_left_03.png")),
    ("walk_left_04.png", include_bytes!("../../characters/default-footballer/walk_left_04.png")),
    ("walk_left_05.png", include_bytes!("../../characters/default-footballer/walk_left_05.png")),
    ("walk_left_06.png", include_bytes!("../../characters/default-footballer/walk_left_06.png")),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_frame_rate")]
    pub frame_rate: u32,
    #[serde(default = "default_size")]
    pub width: u32,
    #[serde(default = "default_size")]
    pub height: u32,
    #[serde(default = "default_speed")]
    pub walking_speed: u32,
    #[serde(default)]
    pub frames: Option<u32>,
}

fn default_frame_rate() -> u32 {
    10
}
fn default_size() -> u32 {
    120
}
fn default_speed() -> u32 {
    110
}

#[derive(Debug, Clone)]
pub struct Character {
    pub def: CharacterDef,
    /// Absolute paths of right-facing frames in order.
    pub frame_paths: Vec<PathBuf>,
    /// Left-facing frames (may be empty; renderer mirrors).
    pub left_frame_paths: Vec<PathBuf>,
    pub dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum CharacterError {
    #[error("character '{0}' not found in {1} (run `dribble character list`)")]
    NotFound(String, String),
    #[error("invalid character.json in {0}: {1}")]
    InvalidManifest(String, String),
    #[error("character '{0}' has no walk_XX.png frames")]
    NoFrames(String),
}

impl Character {
    /// Load a character by name from the user home characters dir.
    pub fn load(home: &Home, name: &str) -> Result<Character, CharacterError> {
        let dir = home.character_dir(name);
        Self::load_dir(&dir, name)
    }

    pub fn load_dir(dir: &Path, name: &str) -> Result<Character, CharacterError> {
        let manifest = dir.join("character.json");
        let bytes = std::fs::read(&manifest).map_err(|_| {
            CharacterError::NotFound(
                name.to_string(),
                dir.parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            )
        })?;
        let def: CharacterDef = serde_json::from_slice(&bytes).map_err(|e| {
            CharacterError::InvalidManifest(dir.display().to_string(), e.to_string())
        })?;

        let frame_paths = collect_frames(dir, "walk_");
        let left_frame_paths = collect_frames(dir, "walk_left_");
        if frame_paths.is_empty() {
            return Err(CharacterError::NoFrames(name.to_string()));
        }
        Ok(Character {
            def,
            frame_paths,
            left_frame_paths,
            dir: dir.to_path_buf(),
        })
    }

    /// List installed characters in the home dir (name, description).
    pub fn list_installed(home: &Home) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(home.characters_dir()) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let desc = std::fs::read(e.path().join("character.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<CharacterDef>(&b).ok())
                    .map(|d| d.description)
                    .unwrap_or_default();
                out.push((name, desc));
            }
        }
        out.sort();
        out
    }

    /// Extract the bundled default character into the home dir when it is
    /// actually wanted: on a fresh install (no characters at all), or when
    /// the user's active character IS the default. Prevents the default
    /// from reappearing after a user deletes it in favor of a custom one.
    pub fn ensure_default(home: &Home) -> std::io::Result<()> {
        let dir = home.character_dir(DEFAULT_CHARACTER);
        if dir.join("character.json").exists() {
            return Ok(());
        }
        let active = wremind_core::Config::load(home)
            .map(|c| c.character)
            .unwrap_or_else(|_| DEFAULT_CHARACTER.to_string());
        let any_installed = !Self::list_installed(home).is_empty();
        if any_installed && active != DEFAULT_CHARACTER {
            return Ok(()); // user has their own character; stay out of the way
        }
        std::fs::create_dir_all(&dir)?;
        for (name, bytes) in BUNDLED_FILES {
            let path = dir.join(name);
            if !path.exists() {
                std::fs::write(path, bytes)?;
            }
        }
        Ok(())
    }

    /// Validate a candidate character directory (used by `character import`).
    pub fn validate_dir(dir: &Path) -> Result<Character, CharacterError> {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "custom".into());
        Self::load_dir(dir, &name)
    }

    pub fn frames_for(&self, direction: Direction) -> &Vec<PathBuf> {
        match direction {
            Direction::Left if !self.left_frame_paths.is_empty() => &self.left_frame_paths,
            _ => &self.frame_paths,
        }
    }
}

fn collect_frames(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut frames: Vec<(u32, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with(prefix) || !name.ends_with(".png") {
                continue;
            }
            let stem = name.trim_end_matches(".png");
            let num_str = stem.trim_start_matches(prefix);
            // Reject e.g. "walk_left_01" when prefix is "walk_"
            if !num_str.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let num = num_str.parse::<u32>().unwrap_or(u32::MAX);
            frames.push((num, e.path()));
        }
    }
    frames.sort_by_key(|(n, _)| *n);
    frames.into_iter().map(|(_, p)| p).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Direction> {
        match s.to_lowercase().as_str() {
            "left" | "l" => Some(Direction::Left),
            "right" | "r" => Some(Direction::Right),
            _ => None,
        }
    }
    pub fn random() -> Direction {
        use rand::Rng;
        if rand::thread_rng().gen() {
            Direction::Right
        } else {
            Direction::Left
        }
    }
    pub fn sign(&self) -> f64 {
        match self {
            Direction::Left => -1.0,
            Direction::Right => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_home() -> (tempfile::TempDir, Home) {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::from_root(dir.path().to_path_buf());
        home.ensure().unwrap();
        (dir, home)
    }

    #[test]
    fn bundled_default_extracts_and_loads() {
        let (_t, home) = tmp_home();
        Character::ensure_default(&home).unwrap();
        let c = Character::load(&home, DEFAULT_CHARACTER).unwrap();
        assert_eq!(c.def.name, "footballer");
        assert_eq!(c.frame_paths.len(), 6);
        assert_eq!(c.left_frame_paths.len(), 6);
        assert_eq!(c.def.frame_rate, 8);
        assert_eq!(c.def.walking_speed, 110);
    }

    #[test]
    fn manifest_camel_case_fields_are_honored() {
        // Regression: walkingSpeed/frameRate must be read from the JSON
        // (camelCase), never silently replaced by Rust defaults.
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("custom");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("character.json"),
            r#"{"name":"t","frameRate":7,"width":80,"height":80,"walkingSpeed":55}"#,
        )
        .unwrap();
        std::fs::write(dir.join("walk_01.png"), b"not really a png but listable").unwrap();
        let c = Character::validate_dir(&dir).unwrap();
        assert_eq!(c.def.frame_rate, 7);
        assert_eq!(c.def.walking_speed, 55);
        assert_eq!(c.def.width, 80);
        assert_eq!(c.def.height, 80);
        assert_eq!(c.frame_paths.len(), 1);
    }

    #[test]
    fn missing_character_is_error() {
        let (_t, home) = tmp_home();
        assert!(matches!(
            Character::load(&home, "nope"),
            Err(CharacterError::NotFound(_, _))
        ));
    }

    #[test]
    fn import_validation_rejects_empty_dir() {
        let t = tempfile::tempdir().unwrap();
        assert!(matches!(
            Character::validate_dir(t.path()),
            Err(CharacterError::NotFound(_, _))
        ));
    }

    #[test]
    fn directions() {
        assert_eq!(Direction::parse("left"), Some(Direction::Left));
        assert_eq!(Direction::parse("RIGHT"), Some(Direction::Right));
        assert_eq!(Direction::parse("up"), None);
    }
}
