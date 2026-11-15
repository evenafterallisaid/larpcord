use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct Presence {
    pub status: String,
    pub activity: Option<String>,
}

#[derive(Default)]
pub struct Presences(HashMap<String, Presence>);
impl Presences {
    pub fn get(&self, id: &str) -> Option<&Presence> {
        self.0.get(id)
    }
    pub fn update(&mut self, value: &Value) {
        let Some(id) = value["user"]["id"]
            .as_str()
            .or_else(|| value["user_id"].as_str())
        else {
            return;
        };
        let Some(status) = value["status"]
            .as_str()
            .filter(|s| matches!(*s, "online" | "idle" | "dnd" | "offline" | "invisible"))
        else {
            return;
        };
        if self.0.len() >= 2000
            && !self.0.contains_key(id)
            && let Some(old) = self.0.keys().next().cloned()
        {
            self.0.remove(&old);
        }
        let activity = value["activities"]
            .as_array()
            .into_iter()
            .flatten()
            .take(32)
            .find_map(|a| {
                let name = a["name"].as_str().unwrap_or_default();
                match a["type"].as_u64() {
                    Some(4) => a["state"].as_str().map(str::to_string),
                    Some(0) if !name.is_empty() => Some(format!("Playing {name}")),
                    Some(2) if !name.is_empty() => Some(format!("Listening to {name}")),
                    Some(3) if !name.is_empty() => Some(format!("Watching {name}")),
                    _ => None,
                }
            })
            .map(|s| s.chars().take(160).collect());
        self.0.insert(
            id.into(),
            Presence {
                status: status.into(),
                activity,
            },
        );
    }
    pub fn ready(&mut self, value: &Value) {
        self.0.clear();
        self.merge(value);
    }
    pub fn merge(&mut self, value: &Value) {
        for p in value["presences"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                value["merged_presences"]["friends"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            )
            .take(2000)
        {
            self.update(p);
        }
        let available = 2000usize.saturating_sub(self.0.len());
        for p in value["merged_presences"]["guilds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_array)
            .flatten()
            .take(available)
        {
            self.update(p);
        }
    }
    pub fn members(&mut self, value: &Value) {
        for op in value["ops"].as_array().into_iter().flatten().take(200) {
            let entries: Vec<_> = if op["op"] == "SYNC" {
                op["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(200)
                    .collect()
            } else {
                vec![&op["item"]]
            };
            for entry in entries {
                let member = &entry["member"];
                if let Some(fields) = member["presence"].as_object() {
                    let mut p = fields.clone();
                    p.insert("user".into(), member["user"].clone());
                    self.update(&Value::Object(p));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_member_and_live_presence_share_one_bounded_view() {
        let mut p = Presences::default();
        p.ready(&serde_json::json!({"merged_presences":{"friends":[{"user_id":"1","status":"idle","activities":[{"type":4,"state":"tea time"}]}]}}));
        assert_eq!(p.get("1").unwrap().activity.as_deref(), Some("tea time"));
        p.members(&serde_json::json!({"ops":[{"op":"UPDATE","item":{"member":{"user":{"id":"1"},"presence":{"status":"dnd","activities":[]}}}}]}));
        assert_eq!(p.get("1").unwrap().status, "dnd");
        assert!(p.get("1").unwrap().activity.is_none());
        p.merge(&serde_json::json!({"merged_presences":{"friends":[{"user_id":"2","status":"online"}]}}));
        assert_eq!(p.get("1").unwrap().status, "dnd");
        assert_eq!(p.get("2").unwrap().status, "online");
        p.update(&serde_json::json!({"user":{"id":"1"},"status":"invalid"}));
        assert_eq!(p.get("1").unwrap().status, "dnd");
        for n in 0..2500 {
            p.update(&serde_json::json!({"user_id":n.to_string(),"status":"online"}));
        }
        assert_eq!(p.0.len(), 2000);
    }
}
