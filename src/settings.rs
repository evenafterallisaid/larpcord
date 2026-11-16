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
    pub theme: crate::theme::Theme,
    #[serde(skip)]
    pub custom_theme: Option<crate::theme::Tokens>,
    pub recent_emoji: Vec<String>,
    pub recent_stickers: Vec<crate::expressions::Sticker>,
    pub recent_gifs: Vec<crate::expressions::Gif>,
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
            theme: crate::theme::Theme::default(),
            custom_theme: None,
            recent_emoji: Vec::new(),
            recent_stickers: Vec::new(),
            recent_gifs: Vec::new(),
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
        settings.sanitize_recents();
        settings.custom_theme = Self::path()
            .map(|p| p.with_file_name("theme.json"))
            .filter(|p| fs::metadata(p).is_ok_and(|m| m.len() <= 16384))
            .and_then(|p| fs::read(p).ok())
            .filter(|bytes| bytes.len() <= 16384)
            .and_then(|bytes| serde_json::from_slice::<crate::theme::Tokens>(&bytes).ok())
            .filter(crate::theme::Tokens::valid);
        settings
    }

    pub fn sanitize_recents(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.recent_stickers
            .retain(|s| s.valid() && s.tags.len() <= 256 && seen.insert(s.id.clone()));
        self.recent_stickers.truncate(32);
        seen.clear();
        self.recent_gifs.retain(|g| {
            (crate::expressions::gif_url(&g.url) || crate::media::cdn_url(&g.url))
                && g.title.len() <= 480
                && seen.insert(g.url.clone())
        });
        self.recent_gifs.truncate(32);
        for gif in &mut self.recent_gifs {
            if gif.preview.as_deref().is_some_and(|url| {
                !crate::expressions::gif_preview(url) && !crate::media::cdn_url(url)
            }) {
                gif.preview = None;
            }
        }
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
