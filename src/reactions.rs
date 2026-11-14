use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Emoji {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub animated: bool,
}
impl Emoji {
    fn same(&self, other: &Self) -> bool {
        match (&self.id, &other.id) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.name == other.name,
            _ => false,
        }
    }
    pub fn url(&self) -> Option<String> {
        let id = self
            .id
            .as_deref()
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))?;
        Some(format!(
            "https://cdn.discordapp.com/emojis/{id}.webp?size=48&quality=lossless"
        ))
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Counts {
    #[serde(default)]
    pub normal: u32,
    #[serde(default)]
    pub burst: u32,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Reaction {
    #[serde(default)]
    pub count: u32,
    pub count_details: Option<Counts>,
    #[serde(default)]
    pub me: bool,
    #[serde(default)]
    pub me_burst: bool,
    pub emoji: Emoji,
    #[serde(default)]
    pub burst_colors: Vec<String>,
}
impl Reaction {
    pub fn counts(&self) -> Counts {
        self.count_details.clone().unwrap_or(Counts {
            normal: self.count,
            burst: 0,
        })
    }
}

pub fn event(kind: &str, data: &Value, own: &str) -> Value {
    json!({"kind":kind,"emoji":data["emoji"],"burst":data["burst"].as_bool().unwrap_or(false)||data["type"].as_u64()==Some(1),"own":data["user_id"].as_str()==Some(own),"colors":data["burst_colors"]})
}

// one gateway session delivers each event once; history requests must not replace newer counts.
pub fn apply(reactions: &mut Vec<Reaction>, event: &Value) {
    let kind = event["kind"].as_str().unwrap_or_default();
    if kind == "MESSAGE_REACTION_REMOVE_ALL" {
        reactions.clear();
        return;
    }
    let Ok(emoji) = serde_json::from_value::<Emoji>(event["emoji"].clone()) else {
        return;
    };
    if emoji.id.is_none() && emoji.name.as_deref().is_none_or(str::is_empty) {
        return;
    }
    if kind == "MESSAGE_REACTION_REMOVE_EMOJI" {
        reactions.retain(|r| !r.emoji.same(&emoji));
        return;
    }
    let add = kind == "MESSAGE_REACTION_ADD";
    if !add && kind != "MESSAGE_REACTION_REMOVE" {
        return;
    }
    let index = match reactions.iter().position(|r| r.emoji.same(&emoji)) {
        Some(index) => index,
        None if add && reactions.len() < 128 => {
            reactions.push(Reaction {
                emoji,
                ..Default::default()
            });
            reactions.len() - 1
        }
        _ => return,
    };
    let reaction = &mut reactions[index];
    let mut counts = reaction.counts();
    let burst = event["burst"].as_bool().unwrap_or(false);
    let count = if burst {
        &mut counts.burst
    } else {
        &mut counts.normal
    };
    *count = if add {
        count.saturating_add(1)
    } else {
        count.saturating_sub(1)
    };
    if event["own"].as_bool() == Some(true) {
        if burst {
            reaction.me_burst = add;
        } else {
            reaction.me = add;
        }
    }
    if burst && let Some(colors) = event["colors"].as_array() {
        reaction.burst_colors = colors
            .iter()
            .filter_map(Value::as_str)
            .take(8)
            .map(str::to_string)
            .collect();
    }
    reaction.count = counts.normal.saturating_add(counts.burst);
    reaction.count_details = Some(counts);
    reactions.retain(|r| r.count > 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_and_super_counts_update_independently_and_remove_cleanly() {
        let mut reactions = vec![];
        let data = json!({"emoji":{"id":null,"name":"❤️"},"user_id":"1"});
        apply(&mut reactions, &event("MESSAGE_REACTION_ADD", &data, "1"));
        apply(
            &mut reactions,
            &event(
                "MESSAGE_REACTION_ADD",
                &json!({"emoji":data["emoji"],"user_id":"2","burst":true,"burst_colors":["#ff0088"]}),
                "1",
            ),
        );
        assert_eq!(reactions[0].count, 2);
        assert!(reactions[0].me);
        assert!(!reactions[0].me_burst);
        apply(
            &mut reactions,
            &event("MESSAGE_REACTION_REMOVE", &data, "1"),
        );
        assert_eq!(reactions[0].counts().normal, 0);
        assert_eq!(reactions[0].counts().burst, 1);
        assert!(!reactions[0].me);
        apply(
            &mut reactions,
            &event("MESSAGE_REACTION_REMOVE_EMOJI", &data, "1"),
        );
        assert!(reactions.is_empty());
    }
}
