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
    #[serde(skip)]
    pub frame: Option<ProfileFrame>,
}
#[derive(Clone, Default, Deserialize)]
pub struct Layer {
    pub src: String,
    pub position: Option<Position>,
    #[serde(default)]
    pub start: f32,
    #[serde(default)]
    pub duration: f32,
    #[serde(rename = "loop", default)]
    pub looping: bool,
    #[serde(alias = "loopDelay", default)]
    pub loop_delay: f32,
    #[serde(alias = "zIndex", default)]
    pub z_index: i32,
}
#[derive(Clone, Default, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Layer {
    pub fn phase(&self, time: f32, asset_duration: f32) -> Option<f32> {
        let start = if self.start.is_finite() {
            self.start.max(0.)
        } else {
            0.
        };
        if time < start {
            return None;
        }
        let duration = if self.duration.is_finite() && self.duration > 0. {
            self.duration
        } else {
            asset_duration.max(100.)
        };
        let delay = if self.loop_delay.is_finite() {
            self.loop_delay.max(0.)
        } else {
            0.
        };
        let elapsed = time - start;
        let phase = if self.looping {
            elapsed % (duration + delay)
        } else {
            elapsed
        };
        (phase <= duration).then_some(phase)
    }
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
        self.frame.is_some()
            || self.still().is_some()
            || self.effects.iter().any(|l| crate::media::cdn_url(&l.src))
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct ProfileFrame {
    #[serde(default)]
    pub sku_id: String,
    pub inner_width: f32,
    #[serde(default)]
    pub overflow_top: f32,
    #[serde(default)]
    pub overflow_bottom: f32,
    #[serde(default)]
    pub overflow_horizontal: f32,
    #[serde(default)]
    pub layers: Vec<FrameLayer>,
}
#[derive(Clone, Default, Deserialize)]
pub struct FrameLayer {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub anchor: String,
    #[serde(default)]
    pub order: String,
    #[serde(default)]
    pub responsive: bool,
}
impl ProfileFrame {
    pub fn valid(&self) -> bool {
        self.inner_width.is_finite()
            && (100. ..=2000.).contains(&self.inner_width)
            && [
                self.overflow_top,
                self.overflow_bottom,
                self.overflow_horizontal,
            ]
            .into_iter()
            .all(|n| n.is_finite() && (0. ..=512.).contains(&n))
            && !self.layers.is_empty()
            && self.layers.len() <= 12
    }
    pub fn url(&self, layer: &FrameLayer) -> Option<String> {
        let base = crate::media::collectible(&self.sku_id)?;
        if layer.id.is_empty()
            || layer.id.len() > 128
            || !layer
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return None;
        }
        Some(format!(
            "{}/{}/static",
            base.trim_end_matches("/static"),
            layer.id
        ))
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
                if fields.contains_key("inner_width")
                    && fields.contains_key("layers")
                    && let Ok(mut frame) = serde_json::from_value::<ProfileFrame>(value.clone())
                    && frame.valid()
                {
                    if frame.sku_id.is_empty() {
                        frame.sku_id = ids.last().cloned().unwrap_or_default();
                    }
                    for id in &ids {
                        out.push((
                            id.clone(),
                            Effect {
                                frame: Some(frame.clone()),
                                ..Effect::default()
                            },
                        ));
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
    fn effect_layers_observe_start_stop_and_loop_gaps() {
        let mut layer = Layer {
            start: 100.,
            duration: 200.,
            loop_delay: 100.,
            ..Default::default()
        };
        assert_eq!(layer.phase(99., 200.), None);
        assert_eq!(layer.phase(150., 200.), Some(50.));
        assert_eq!(layer.phase(350., 200.), None);
        layer.looping = true;
        assert_eq!(layer.phase(350., 200.), None);
        assert_eq!(layer.phase(450., 200.), Some(50.));
    }
    #[test]
    fn modern_profile_frames_keep_their_own_layers_and_reject_invalid_geometry() {
        let value = serde_json::json!({"sku_id":"123","items":[{"type":3,"sku_id":"456","inner_width":300,"overflow_top":40,"overflow_bottom":20,"overflow_horizontal":12,"layers":[{"id":"top","type":"staple","anchor":"top","order":"front"},{"id":"edge","type":"border","order":"back"}]}]});
        let found = catalog(&value);
        let frame = found
            .iter()
            .find(|(id, _)| id == "123")
            .unwrap()
            .1
            .frame
            .as_ref()
            .unwrap();
        assert_eq!(
            frame.url(&frame.layers[0]).unwrap(),
            "https://cdn.discordapp.com/media/v1/collectibles-shop/456/top/static"
        );
        assert_eq!(frame.overflow_top, 40.);
        let invalid =
            serde_json::json!({"sku_id":"123","inner_width":0,"layers":[{"id":"x","type":"rail"}]});
        assert!(catalog(&invalid).is_empty());
    }
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
