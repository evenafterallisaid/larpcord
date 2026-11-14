use crate::model::Member;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone)]
pub enum Row {
    Group { id: String, count: u32 },
    Member(Box<Member>, String, Option<String>),
}
#[derive(Default)]
pub struct Members {
    pub guild: Option<String>,
    pub channel: Option<String>,
    pub total: usize,
    pub online: u32,
    pub received: bool,
    pub rows: BTreeMap<usize, Row>,
    groups: BTreeMap<String, u32>,
    pub bucket: usize,
}
impl Members {
    pub fn demo() -> Self {
        let mut members = Self::default();
        members.reset(Some("larp".into()), Some("campfire".into()));
        members.apply(&serde_json::json!({"guild_id":"larp","groups":[{"id":"online","count":3}],"ops":[{"op":"SYNC","range":[0,3],"items":[{"group":{"id":"online","count":3}},{"member":{"user":{"id":"me","username":"you"},"presence":{"status":"online"}}},{"member":{"user":{"id":"moss","username":"moss"},"presence":{"status":"idle","activities":[{"type":0,"name":"a little game"}]}}},{"member":{"user":{"id":"clover","username":"clover"},"presence":{"status":"dnd"}}}]}]}));
        members
    }
    pub fn reset(&mut self, guild: Option<String>, channel: Option<String>) {
        *self = Self {
            guild,
            channel,
            total: 100,
            ..Default::default()
        };
    }
    pub fn apply(&mut self, data: &Value) {
        if data["guild_id"].as_str() != self.guild.as_deref() {
            return;
        }
        self.received = true;
        self.online = data["online_count"]
            .as_u64()
            .unwrap_or(self.online as u64)
            .min(u32::MAX as u64) as u32;
        if let Some(groups) = data["groups"].as_array() {
            self.groups = groups
                .iter()
                .take(2000)
                .filter_map(|group| {
                    Some((
                        group["id"].as_str()?.to_string(),
                        group["count"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32,
                    ))
                })
                .collect();
            for row in self.rows.values_mut() {
                if let Row::Group { id, count } = row
                    && let Some(current) = self.groups.get(id)
                {
                    *count = *current;
                }
            }
            self.total = groups
                .iter()
                .take(2000)
                .map(|g| g["count"].as_u64().unwrap_or(0).min(2_000_000) as usize + 1)
                .fold(0usize, usize::saturating_add)
                .min(2_000_000);
        }
        for op in data["ops"].as_array().into_iter().flatten().take(200) {
            let index = op["index"].as_u64().unwrap_or(0).min(1_999_999) as usize;
            match op["op"].as_str().unwrap_or_default() {
                "SYNC" => {
                    let start = op["range"][0].as_u64().unwrap_or(0).min(1_999_999) as usize;
                    let end = op["range"][1].as_u64().unwrap_or(0).min(1_999_999) as usize;
                    self.rows.retain(|i, _| *i < start || *i > end);
                    for (offset, item) in op["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(200)
                        .enumerate()
                    {
                        if start + offset > end {
                            break;
                        }
                        if let Some(row) = parse(item, &self.groups) {
                            self.rows.insert(start + offset, row);
                        }
                    }
                }
                "INVALIDATE" => {
                    let start = op["range"][0].as_u64().unwrap_or(0).min(1_999_999) as usize;
                    let end = op["range"][1].as_u64().unwrap_or(0).min(1_999_999) as usize;
                    self.rows.retain(|i, _| *i < start || *i > end);
                }
                "UPDATE" => {
                    if let Some(row) = parse(&op["item"], &self.groups) {
                        self.rows.insert(index, row);
                    }
                }
                "INSERT" => {
                    let rest = self.rows.split_off(&index);
                    for (i, row) in rest {
                        self.rows.insert(i.saturating_add(1), row);
                    }
                    if let Some(row) = parse(&op["item"], &self.groups) {
                        self.rows.insert(index, row);
                    }
                }
                "DELETE" => {
                    self.rows.remove(&index);
                    let rest = self.rows.split_off(&index);
                    for (i, row) in rest {
                        self.rows.insert(i.saturating_sub(1), row);
                    }
                }
                _ => {}
            }
        }
        self.trim();
    }
    pub fn trim(&mut self) {
        let bucket = self.bucket;
        self.rows.retain(|i, _| {
            *i < self.total && (*i < 100 || (*i >= bucket && *i < bucket.saturating_add(200)))
        });
        while self.rows.len() > 300 {
            self.rows.pop_last();
        }
    }
    pub fn presence(&mut self, data: &Value) {
        let Some(id) = data["user"]["id"].as_str() else {
            return;
        };
        for row in self.rows.values_mut() {
            if let Row::Member(member, status, activity) = row
                && member.user.as_ref().is_some_and(|u| u.id == id)
            {
                if let Some(s) = data["status"].as_str() {
                    *status = s.to_string();
                }
                *activity = activity_text(data);
            }
        }
    }
}
fn activity_text(v: &Value) -> Option<String> {
    v["activities"]
        .as_array()?
        .iter()
        .find_map(|a| {
            let name = a["name"].as_str().unwrap_or_default();
            match a["type"].as_u64() {
                Some(4) => a["state"].as_str().map(str::to_string),
                Some(0) => Some(format!("Playing {name}")),
                Some(2) => Some(format!("Listening to {name}")),
                _ => None,
            }
        })
        .map(|s| s.chars().take(160).collect())
}
fn parse(v: &Value, groups: &BTreeMap<String, u32>) -> Option<Row> {
    if let Some(g) = v.get("group") {
        return Some(Row::Group {
            id: g["id"].as_str()?.to_string(),
            count: g["count"]
                .as_u64()
                .map(|n| n.min(u32::MAX as u64) as u32)
                .or_else(|| groups.get(g["id"].as_str()?).copied())
                .unwrap_or(0),
        });
    }
    let m = v.get("member")?;
    let member: Member = serde_json::from_value(m.clone()).ok()?;
    member.user.as_ref()?;
    Some(Row::Member(
        Box::new(member),
        m["presence"]["status"]
            .as_str()
            .unwrap_or("offline")
            .to_string(),
        activity_text(&m["presence"]),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_member_updates_shift_rows_and_stay_bounded() {
        let mut m = Members::default();
        m.reset(Some("1".into()), Some("2".into()));
        m.apply(&serde_json::json!({"guild_id":"1","groups":[{"id":"online","count":2}],"ops":[{"op":"SYNC","range":[0,2],"items":[{"group":{"id":"online","count":2}},{"member":{"user":{"id":"7","username":"a"},"presence":{"status":"online"}}},{"member":{"user":{"id":"8","username":"b"}}}]}]}));
        assert_eq!(m.total, 3);
        assert_eq!(m.rows.len(), 3);
        m.apply(&serde_json::json!({"guild_id":"1","ops":[{"op":"DELETE","index":1}]}));
        assert!(
            matches!(m.rows.get(&1),Some(Row::Member(member,_,_)) if member.user.as_ref().unwrap().id=="8")
        );
        m.presence(&serde_json::json!({"user":{"id":"8"},"status":"dnd","activities":[{"type":0,"name":"a game"}]}));
        assert!(
            matches!(m.rows.get(&1),Some(Row::Member(_,status,activity)) if status=="dnd"&&activity.as_deref()==Some("Playing a game"))
        );
    }

    #[test]
    fn role_headers_use_metadata_counts_when_sync_rows_only_contain_ids() {
        let mut members = Members::default();
        members.reset(Some("1".into()), Some("2".into()));
        members.apply(&serde_json::json!({"guild_id":"1","groups":[{"id":"99","count":8}],"ops":[{"op":"SYNC","range":[0,0],"items":[{"group":{"id":"99"}}]}]}));
        assert!(matches!(
            members.rows.get(&0),
            Some(Row::Group { count: 8, .. })
        ));
        members
            .apply(&serde_json::json!({"guild_id":"1","groups":[{"id":"99","count":9}],"ops":[]}));
        assert!(matches!(
            members.rows.get(&0),
            Some(Row::Group { count: 9, .. })
        ));
        members.apply(
            &serde_json::json!({"guild_id":"1","groups":[{"id":"99","count":u64::MAX}],"ops":[]}),
        );
        assert_eq!(members.total, 2_000_000);
        assert!(members.rows.len() <= 300);
    }
}
