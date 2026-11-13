use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct NameStyle {
    pub font_id: Option<u8>,
    pub effect_id: Option<u8>,
    #[serde(default)]
    pub colors: Vec<u32>,
}
impl NameStyle {
    pub fn font(&self) -> &'static str {
        match self.font_id {
            Some(1) => "Bangers",
            Some(2) => "BioRhyme",
            Some(3) => "Cherry Bomb One",
            Some(4) => "Chicle",
            Some(6) => "MuseoModerno",
            Some(8) => "Pixelify Sans",
            Some(12) => "Zilla Slab",
            Some(13) => "Playpen Sans",
            Some(14) => "Orbitron",
            Some(15) => "New Rocker",
            Some(16) => "Kalam",
            _ => "Inter",
        }
    }
    pub fn color_at(&self, position: f32, fallback: u32) -> u32 {
        if self.colors.is_empty() {
            return fallback;
        }
        let colors = &self.colors[..self.colors.len().min(8)];
        if colors.len() == 1 || self.effect_id == Some(1) {
            return colors[0] & 0xffffff;
        }
        let t = position.clamp(0., 1.) * (colors.len() - 1) as f32;
        let i = (t.floor() as usize).min(colors.len() - 1);
        let a = colors[i];
        let b = colors[(i + 1).min(colors.len() - 1)];
        let t = t - i as f32;
        [16, 8, 0].into_iter().fold(0, |out, shift| {
            out | (((((a >> shift) & 255) as f32 * (1. - t) + ((b >> shift) & 255) as f32 * t)
                .round() as u32)
                << shift)
        })
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Nameplate {
    pub sku_id: Option<String>,
    pub asset: Option<String>,
    pub label: Option<String>,
    pub palette: Option<String>,
}
impl Nameplate {
    pub fn url(&self) -> Option<String> {
        self.sku_id.as_deref().and_then(crate::media::collectible)
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Collectibles {
    pub nameplate: Option<Nameplate>,
}
#[derive(Clone, Default, Deserialize)]
pub struct Reference {
    pub sku_id: String,
    #[serde(rename = "type")]
    pub kind: u8,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct TypingEmoji {
    pub custom_emoji_id: Option<String>,
    pub unicode_emoji: Option<String>,
    #[serde(default)]
    pub animated: bool,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct TypingStyle {
    #[serde(default)]
    pub emojis: Vec<TypingEmoji>,
    #[serde(default)]
    pub animation: u8,
    #[serde(default)]
    pub typing_suggestion: u8,
}
impl TypingStyle {
    pub fn verb(&self) -> &'static str {
        match self.typing_suggestion {
            1 => "yapping",
            2 => "venting",
            3 => "oversharing",
            4 => "barking",
            5 => "babbling",
            6 => "daydreaming",
            7 => "meowing",
            8 => "purring",
            9 => "chirping",
            10 => "keysmashing",
            11 => "thinking",
            12 => "squeaking",
            _ => "typing",
        }
    }
}

pub fn register_fonts(cx: &gpui::App) {
    let fonts: Vec<std::borrow::Cow<'static, [u8]>> = vec![
        include_bytes!("../assets/fonts/inter.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/bangers.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/biorhyme.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/cherrybombone.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/chicle.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/museomoderno.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/pixelifysans.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/zillaslab.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/playpensans.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/orbitron.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/newrocker.ttf")
            .as_slice()
            .into(),
        include_bytes!("../assets/fonts/kalam.ttf")
            .as_slice()
            .into(),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("could not register bundled fonts: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn styles_keep_unicode_and_unknown_values_safe() {
        let style: NameStyle = serde_json::from_value(
            serde_json::json!({"font_id":250,"effect_id":2,"colors":[0xff0000,0x0000ff]}),
        )
        .unwrap();
        assert_eq!(style.font(), "Inter");
        assert_eq!(style.color_at(0., 0), 0xff0000);
        assert_eq!(style.color_at(1., 0), 0x0000ff);
        assert_eq!(style.color_at(0.5, 0), 0x800080);
        assert!(
            Nameplate {
                asset: Some("../../evil".into()),
                ..Default::default()
            }
            .url()
            .is_none()
        );
        assert_eq!(
            TypingStyle {
                typing_suggestion: 7,
                ..Default::default()
            }
            .verb(),
            "meowing"
        );
    }
}
