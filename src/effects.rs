use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Default, Deserialize)]
pub struct Effect {
    pub title: Option<String>,
    #[serde(alias = "reducedMotionSrc")]
    pub reduced_motion_src: Option<String>,
    #[serde(alias = "staticFrameSrc")]
    pub static_frame_src: Option<String>,
    #[serde(alias = "thumbnailPreviewSrc")]
    pub thumbnail_preview_src: Option<String>,
    #[serde(default)]
    pub effects: Vec<Layer>,
}
#[derive(Clone, Default, Deserialize)]
pub struct Layer {
    pub src: String,
    pub position: Option<Position>,
}
#[derive(Clone, Default, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Effect {
    pub fn still(&self) -> Option<&str> {
        self.reduced_motion_src
            .as_deref()
            .or(self.static_frame_src.as_deref())
            .or(self.thumbnail_preview_src.as_deref())
            .filter(|u| crate::media::cdn_url(u))
    }
    pub fn valid(&self) -> bool {
        self.still().is_some() || self.effects.iter().any(|l| crate::media::cdn_url(&l.src))
    }
}

// product ids and item ids are both used by profile payloads. retain their relationship.
pub fn catalog(value: &Value) -> Vec<(String, Effect)> {
    fn walk(
        value: &Value,
        parents: &[String],
        depth: usize,
        budget: &mut usize,
        out: &mut Vec<(String, Effect)>,
    ) {
        if depth > 24 || *budget == 0 || out.len() >= 1024 {
            return;
        }
        *budget -= 1;
        match value {
            Value::Object(fields) => {
                let mut ids = parents.to_vec();
                for key in ["id", "sku_id", "skuId"] {
                    if let Some(id) = fields.get(key).and_then(Value::as_str).filter(|s| {
                        !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit())
                    }) && !ids.iter().any(|p| p == id)
                    {
                        ids.push(id.to_string());
                    }
                }
                if (fields.contains_key("effects")
                    || fields.contains_key("reduced_motion_src")
                    || fields.contains_key("reducedMotionSrc"))
                    && let Ok(effect) = serde_json::from_value::<Effect>(value.clone())
                    && effect.valid()
                {
                    for id in &ids {
                        out.push((id.clone(), effect.clone()));
                    }
                }
                for child in fields.values() {
                    walk(child, &ids, depth + 1, budget, out);
                }
            }
            Value::Array(values) => {
                for child in values {
                    walk(child, parents, depth + 1, budget, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, &[], 0, &mut 50_000, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effect_lookup_accepts_product_and_item_ids_but_only_public_cdn_assets() {
        let value = serde_json::json!({"id":"123", "items":[{"id":"456", "type":2,"config":{"reducedMotionSrc":"https://cdn.discordapp.com/assets/profile_effects/still.png", "effects":[{"src":"https://cdn.discordapp.com/assets/profile_effects/layer.png","position":{"x":0,"y":0}}]}}]});
        let result = catalog(&value);
        assert!(
            result
                .iter()
                .any(|(id, e)| id == "123" && e.still().is_some())
        );
        assert!(result.iter().any(|(id, _)| id == "456"));
        assert!(
            catalog(
                &serde_json::json!({"id":"123","effects":[{"src":"https://other.test/tracker"}]})
            )
            .is_empty()
        );
    }
}
