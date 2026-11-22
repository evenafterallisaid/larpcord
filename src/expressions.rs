use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20 && value.bytes().all(|b| b.is_ascii_digit())
}

#[derive(Clone, Default, Deserialize)]
pub struct Emoji {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub animated: bool,
    pub available: Option<bool>,
    #[serde(default)]
    pub roles: Vec<String>,
}

impl Emoji {
    pub fn valid(&self) -> bool {
        id(&self.id)
            && !self.name.is_empty()
            && self.name.len() <= 64
            && self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    }
    pub fn markup(&self) -> String {
        format!(
            "<{}:{}:{}>",
            if self.animated { "a" } else { "" },
            self.name,
            self.id
        )
    }
    pub fn image(&self, motion: bool) -> String {
        if self.animated && motion {
            format!("https://cdn.discordapp.com/emojis/{}.gif#motion", self.id)
        } else {
            format!(
                "https://cdn.discordapp.com/emojis/{}.webp?size=64&quality=lossless",
                self.id
            )
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Sticker {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tags: String,
    pub format_type: u8,
    pub available: Option<bool>,
}
impl Sticker {
    pub fn valid(&self) -> bool {
        id(&self.id)
            && !self.name.is_empty()
            && self.name.len() <= 120
            && (1..=4).contains(&self.format_type)
    }
    pub fn message(&self) -> crate::model::Sticker {
        crate::model::Sticker {
            id: self.id.clone(),
            name: self.name.clone(),
            format_type: self.format_type,
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct StickerPack {
    pub id: String,
    pub name: String,
    pub stickers: Vec<Sticker>,
}

pub fn packs(value: Value) -> Vec<StickerPack> {
    value["sticker_packs"]
        .as_array()
        .into_iter()
        .flatten()
        .take(100)
        .filter_map(|v| serde_json::from_value::<StickerPack>(v.clone()).ok())
        .filter(|p| id(&p.id) && p.name.len() <= 120)
        .map(|mut p| {
            p.stickers.retain(Sticker::valid);
            p.stickers.truncate(100);
            p
        })
        .collect()
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Gif {
    pub title: String,
    pub url: String,
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

fn https_host(value: &str, hosts: &[&str]) -> bool {
    value.len() <= 2048
        && reqwest::Url::parse(value).ok().is_some_and(|u| {
            u.scheme() == "https"
                && u.username().is_empty()
                && u.password().is_none()
                && u.port_or_known_default() == Some(443)
                && u.host_str().is_some_and(|h| hosts.contains(&h))
        })
}
pub fn gif_url(value: &str) -> bool {
    https_host(
        value,
        &[
            "klipy.com",
            "tenor.com",
            "media.tenor.com",
            "static.klipy.com",
            "static1.klipy.com",
            "static2.klipy.com",
        ],
    )
}
pub fn gif_preview(value: &str) -> bool {
    crate::media::gif_preview(value)
}
pub fn gifs(value: Value) -> Vec<Gif> {
    value
        .as_array()
        .or_else(|| value["results"].as_array())
        .into_iter()
        .flatten()
        .take(50)
        .filter_map(|v| {
            let url = v["url"].as_str().filter(|u| gif_url(u))?.to_string();
            let preview = ["gif_src", "preview"]
                .iter()
                .filter_map(|k| v[*k].as_str())
                .find(|u| gif_preview(u))
                .map(str::to_string);
            Some(Gif {
                title: v["title"]
                    .as_str()
                    .unwrap_or("GIF")
                    .chars()
                    .take(120)
                    .collect(),
                url,
                thumbnail: v["preview"]
                    .as_str()
                    .filter(|url| gif_preview(url) && preview.as_deref() != Some(*url))
                    .map(str::to_string),
                preview,
                width: v["width"].as_u64().unwrap_or(1).min(4096) as u32,
                height: v["height"].as_u64().unwrap_or(1).min(4096) as u32,
            })
        })
        .collect()
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    Gifs,
    Stickers,
    #[default]
    Emoji,
}

#[derive(Default)]
pub struct Picker {
    pub account_recent: Vec<String>,
    pub account_stickers: Vec<String>,
    pub session_stickers: Vec<String>,
    pub sticker_details: std::collections::HashMap<String, Sticker>,
    pub sticker_attempts: std::collections::HashMap<String, std::time::Instant>,
    pub reaction_recent: Vec<String>,
    pub session_recent: Vec<String>,
    pub recents_checked: Option<std::time::Instant>,
    pub recents_loading: bool,
    pub recents_error: Option<String>,
    pub favorites: Vec<Gif>,
    pub favorites_loaded: bool,
    pub favorites_loading: bool,
    pub favorites_error: Option<String>,
    pub hovered: Option<String>,
    pub tab: Tab,
    pub scope: Option<String>,
    pub pending_scope: Option<String>,
    pub recent: Vec<String>,
    pub gifs: Vec<Gif>,
    pub query: String,
    pub request: u64,
    pub loading: bool,
    pub error: Option<String>,
    pub packs: Vec<StickerPack>,
    pub packs_loaded: bool,
    pub packs_loading: bool,
    pub packs_error: Option<String>,
}
impl Picker {
    pub fn remember(&mut self, text: String) {
        self.session_recent.retain(|old| old != &text);
        self.session_recent.insert(0, text.clone());
        self.session_recent.truncate(32);
        self.recent.retain(|old| old != &text);
        self.recent.insert(0, text);
        self.recent.truncate(32);
    }
}

pub fn unicode_alias(key: &str) -> Option<&'static str> {
    include_str!("../assets/emoji-aliases.tsv")
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .find(|(name, _)| *name == key.trim_matches(':'))
        .map(|(_, emoji)| emoji)
}
pub fn unicode_catalog() -> &'static Vec<(&'static str, &'static str)> {
    static CATALOG: std::sync::OnceLock<Vec<(&str, &str)>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut seen = std::collections::HashSet::new();
        include_str!("../assets/emoji-aliases.tsv")
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .filter(|(_, emoji)| seen.insert(*emoji))
            .map(|(name, emoji)| (emoji, name))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gif_results_keep_only_provider_links_and_credential_free_image_previews() {
        let page = gifs(serde_json::json!([
            {"url":"https://klipy.com/gifs/cat","preview":"https://static.klipy.com/cat.webp","title":"cat","gif_src":"https://static.klipy.com/full-cat.gif"},
            {"url":"https://klipy.com/gifs/video","preview":"https://evil.invalid/x.gif"},
            {"url":"https://klipy.com.evil.invalid/gifs/x","preview":"https://static.klipy.com/x.gif"},
            {"url":"https://user:password@klipy.com/gifs/x"}
        ]));
        assert_eq!(page.len(), 2);
        assert_eq!(
            page[0].preview.as_deref(),
            Some("https://static.klipy.com/full-cat.gif")
        );
        assert_eq!(
            page[0].thumbnail.as_deref(),
            Some("https://static.klipy.com/cat.webp")
        );
        assert!(page[1].preview.is_none());
        assert!(!gif_preview("https://static.klipy.com/x.mp4"));
    }
    #[test]
    fn emoji_markup_preserves_animation_and_rejects_invalid_tokens() {
        let emoji = Emoji {
            id: "123".into(),
            name: "wave".into(),
            animated: true,
            ..Default::default()
        };
        assert!(emoji.valid());
        assert_eq!(emoji.markup(), "<a:wave:123>");
        assert!(
            !Emoji {
                name: "bad:name".into(),
                ..emoji
            }
            .valid()
        );
        let mut picker = Picker::default();
        for n in 0..80 {
            picker.remember(n.to_string());
        }
        picker.remember("79".into());
        assert_eq!(picker.recent.len(), 32);
        assert_eq!(picker.recent[0], "79");
    }
}
