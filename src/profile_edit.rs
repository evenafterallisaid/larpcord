use std::{
    io::{Cursor, Read},
    path::Path,
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};

use crate::model::Profile;

#[derive(Clone, Copy)]
pub enum Artwork {
    Avatar,
    Banner,
}

impl Artwork {
    pub fn label(self) -> &'static str {
        match self {
            Self::Avatar => "Avatar",
            Self::Banner => "Banner",
        }
    }
}

pub struct Upload {
    data: String,
    pub image: Arc<gpui::RenderImage>,
}

pub fn prepare_image(path: &Path, kind: Artwork) -> Result<Upload, String> {
    const LIMIT: usize = 8 * 1024 * 1024;
    let metadata = std::fs::metadata(path).map_err(|_| "Could not read the image.")?;
    if !metadata.is_file() || metadata.len() > LIMIT as u64 {
        return Err("Choose an image file of 8 MB or less.".into());
    }
    let mut data = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(LIMIT as u64 + 1).read_to_end(&mut data))
        .map_err(|_| "Could not read the image.")?;
    if data.len() > LIMIT {
        return Err("Choose an image file of 8 MB or less.".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| "Could not identify the image.")?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err("Choose a PNG, JPEG or WebP image.".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(80 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Could not decode the image. Use dimensions up to 4096 × 4096.")?;
    let (width, height) = match kind {
        Artwork::Avatar => (256, 256),
        Artwork::Banner => (600, 240),
    };
    let image = image.resize_to_fill(width, height, image::imageops::FilterType::Lanczos3);
    let mut encoded = Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|_| "Could not prepare the image.")?;
    let data = format!(
        "data:image/png;base64,{}",
        STANDARD.encode(encoded.into_inner())
    );
    let mut pixels = image.into_rgba8();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Upload {
        data,
        image: Arc::new(gpui::RenderImage::new(vec![image::Frame::new(pixels)])),
    })
}

#[derive(Default)]
pub enum ImageEdit {
    #[default]
    Keep,
    Remove,
    Replace(Upload),
}

impl ImageEdit {
    pub fn image(&self) -> Option<Arc<gpui::RenderImage>> {
        match self {
            Self::Replace(upload) => Some(upload.image.clone()),
            _ => None,
        }
    }

    fn patch(&self, current: Option<&str>) -> Option<Value> {
        match self {
            Self::Keep => None,
            Self::Remove => current.map(|_| Value::Null),
            Self::Replace(upload) => Some(json!(upload.data)),
        }
    }
}

#[derive(PartialEq, Eq)]
pub struct Fields {
    pub name: String,
    pub bio: String,
    pub pronouns: String,
    pub color: String,
}

impl Fields {
    pub fn from_profile(p: &Profile) -> Self {
        Self {
            name: p.user.global_name.clone().unwrap_or_default(),
            bio: p.bio().unwrap_or_default().into(),
            pronouns: p.pronouns().unwrap_or_default().into(),
            color: accent(p).map_or_else(String::new, |color| format!("#{color:06X}")),
        }
    }

    pub fn color(&self) -> Result<Option<u32>, String> {
        let color = self.color.trim().trim_start_matches('#');
        if color.is_empty() {
            return Ok(None);
        }
        if color.len() == 6
            && let Ok(color) = u32::from_str_radix(color, 16)
        {
            return Ok(Some(color));
        }
        Err("Use a six-digit hex color such as #7970D8, or leave it blank.".into())
    }

    pub fn patch(
        &self,
        before: &Profile,
        avatar: &ImageEdit,
        banner: &ImageEdit,
    ) -> Result<Map<String, Value>, String> {
        let mut patch = Map::new();
        let baseline = Self::from_profile(before);
        for (key, label, text, old, limit) in [
            (
                "global_name",
                "Display name",
                self.name.trim(),
                baseline.name.as_str(),
                32,
            ),
            (
                "pronouns",
                "Pronouns",
                self.pronouns.trim(),
                baseline.pronouns.as_str(),
                40,
            ),
            (
                "bio",
                "About me",
                self.bio.as_str(),
                baseline.bio.as_str(),
                190,
            ),
        ] {
            if text == old {
                continue;
            }
            let valid = text.chars().count() <= limit
                && text
                    .chars()
                    .all(|c| !c.is_control() || key == "bio" && c == '\n');
            if !valid {
                return Err(format!(
                    "{label} must be {limit} characters or fewer and contain no unsupported characters."
                ));
            }
            patch.insert(
                key.into(),
                if key == "global_name" && text.is_empty() {
                    Value::Null
                } else {
                    json!(text)
                },
            );
        }
        if self.color != baseline.color {
            let color = self.color()?;
            if color != accent(before) {
                patch.insert("accent_color".into(), json!(color));
            }
        }
        if let Some(value) = avatar.patch(before.user.avatar.as_deref()) {
            patch.insert("avatar".into(), value);
        }
        if let Some(value) = banner.patch(before.banner().map(|(hash, _)| hash)) {
            patch.insert("banner".into(), value);
        }
        Ok(patch)
    }
}

fn accent(profile: &Profile) -> Option<u32> {
    profile
        .user_profile
        .as_ref()
        .and_then(|p| p.accent_color)
        .or(profile.user.accent_color)
}

pub fn confirmed(
    patch: &Map<String, Value>,
    profile: &Profile,
    saved: &crate::model::User,
) -> bool {
    patch.iter().all(|(field, value)| match field.as_str() {
        "global_name" => value.as_str() == profile.user.global_name.as_deref(),
        "bio" => value.as_str() == Some(profile.bio().unwrap_or_default()),
        "pronouns" => value.as_str() == Some(profile.pronouns().unwrap_or_default()),
        "accent_color" => value.as_u64() == accent(profile).map(u64::from),
        "avatar" => {
            saved.avatar.as_deref() == profile.user.avatar.as_deref()
                && value.is_null() == saved.avatar.is_none()
        }
        "banner" => {
            saved.banner.as_deref() == profile.banner().map(|(hash, _)| hash)
                && value.is_null() == saved.banner.is_none()
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_only_changed_fields_and_supports_clearing() {
        let p: Profile = serde_json::from_value(json!({
            "user":{"id":"1","global_name":"Example","avatar":"a1"},
            "user_profile":{"bio":"First\nSecond","accent_color":123,"pronouns":"they/them"}
        }))
        .unwrap();
        let mut fields = Fields::from_profile(&p);
        assert!(
            fields
                .patch(&p, &ImageEdit::Keep, &ImageEdit::Keep)
                .unwrap()
                .is_empty()
        );
        fields.name.clear();
        fields.color.clear();
        let patch = fields
            .patch(&p, &ImageEdit::Remove, &ImageEdit::Keep)
            .unwrap();
        assert_eq!(
            Value::Object(patch),
            json!({"global_name":null,"accent_color":null,"avatar":null})
        );
        fields.pronouns = "🦀".repeat(41);
        assert!(
            fields
                .patch(&p, &ImageEdit::Keep, &ImageEdit::Keep)
                .is_err()
        );
    }
}
