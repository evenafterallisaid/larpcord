use serde::Deserialize;

#[derive(Clone, Default, Deserialize)]
pub struct Component {
    #[serde(rename = "type", default)]
    pub kind: u8,
    pub id: Option<u32>,
    pub content: Option<String>,
    pub label: Option<String>,
    pub custom_id: Option<String>,
    pub url: Option<String>,
    pub style: Option<u8>,
    #[serde(default)]
    pub disabled: bool,
    pub placeholder: Option<String>,
    pub accent_color: Option<u32>,
    pub divider: Option<bool>,
    pub spacing: Option<u8>,
    #[serde(default)]
    pub components: Vec<Component>,
    pub accessory: Option<Box<Component>>,
    pub media: Option<Media>,
    pub file: Option<Media>,
    #[serde(default)]
    pub items: Vec<GalleryItem>,
    #[serde(default)]
    pub spoiler: bool,
    #[serde(default)]
    pub options: Vec<SelectOption>,
    pub min_values: Option<usize>,
    pub max_values: Option<usize>,
    pub emoji: Option<Emoji>,
}
#[derive(Clone, Default, Deserialize)]
pub struct Media {
    pub url: String,
    pub proxy_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}
#[derive(Clone, Default, Deserialize)]
pub struct GalleryItem {
    pub media: Media,
    pub description: Option<String>,
    #[serde(default)]
    pub spoiler: bool,
}
#[derive(Clone, Default, Deserialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
    #[serde(default)]
    pub default: bool,
}
#[derive(Clone, Default, Deserialize)]
pub struct Emoji {
    pub id: Option<String>,
    pub name: Option<String>,
}
