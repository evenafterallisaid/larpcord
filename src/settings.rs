use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub notifications: bool,
    pub notification_preview: bool,
    pub keep_connected: bool,
    pub remember_session: bool,
    pub compact: bool,
    pub animations: bool,
    pub recent_emoji: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: true,
            notification_preview: false,
            keep_connected: true,
            remember_session: true,
            compact: false,
            animations: true,
            recent_emoji: Vec::new(),
        }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("app", "larpcord", "Larpcord")
            .map(|dirs| dirs.config_dir().join("settings.json"))
    }

    pub fn load() -> Self {
        let mut settings: Self = Self::path()
            .and_then(|path| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        settings
            .recent_emoji
            .retain(|s| s.len() <= 128 && !s.chars().any(char::is_control));
        settings.recent_emoji.truncate(32);
        settings
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path()
            .ok_or_else(|| anyhow::anyhow!("Could not locate your settings folder."))?;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid settings path."))?;
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec_pretty(self)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path)?;
        Ok(())
    }
}
