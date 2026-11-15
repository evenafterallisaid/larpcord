use crate::model::Member;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone)]
pub enum Row {
    Group { id: String, count: u32 },
    Member(Box<Member>, String, Option<String>),
}
#[derive(Clone, Default)]
pub struct Members {
    pub guild: Option<String>,
    pub channel: Option<String>,
    pub list_id: Option<String>,
    pub total: usize,
    pub online: u32,
    pub received: bool,
    pub rows: BTreeMap<usize, Row>,
    groups: BTreeMap<String, u32>,
    pub bucket: usize,
}

#[derive(Default)]
pub struct Views(std::collections::VecDeque<(Members, std::time::Instant)>);
impl Views {
    pub fn select(
        &mut self,
        current: &mut Members,
        guild: Option<String>,
        channel: Option<String>,
        list_id: Option<String>,
    ) {
        if current.guild == guild && current.channel == channel && current.list_id == list_id {
            return;
        }
        // discord shares one sparse stream across channels with the same visibility list.
        if guild.is_some()
            && current.guild == guild
            && list_id.is_some()
            && current.list_id == list_id
        {
            current.channel = channel;
            current.bucket = 0;
            current.trim();
            return;
        }
        self.0
            .retain(|(_, at)| at.elapsed() < std::time::Duration::from_secs(60));
        let found = self.0.iter().position(|(m, _)| {
            m.guild == guild && m.list_id == list_id && (m.channel == channel || list_id.is_some())
        });
        let mut next = found
            .and_then(|index| self.0.remove(index))
            .map(|(m, _)| m)
            .unwrap_or_else(|| {
                let mut m = Members::default();
                m.reset(guild, channel.clone());
                m.list_id = list_id;
                m
            });
        next.channel = channel.clone();
        std::mem::swap(current, &mut next);
        if next.received && next.guild.is_some() {
            self.0.push_back((next, std::time::Instant::now()));
        }
        while self.0.len() > 2 {
            self.0.pop_front();
        }
    }
}

pub fn list_identity(
    channel: &crate::model::Channel,
    roles: &[crate::model::Role],
) -> Option<String> {
    if matches!(channel.kind, 10..=12) {
        return None;
    }
    if let Some(id) = &channel.member_list_id {
        return Some(id.clone());
    }
    let guild = channel.guild_id.as_ref()?;
    let base = roles
        .iter()
        .find(|role| &role.id == guild)?
        .permissions
        .as_ref()?
        .parse::<u128>()
        .ok()?;
    let overwrites = channel.permission_overwrites.as_ref()?;
    if overwrites.len() > 1000 {
        return None;
    }
    let mut restricted = false;
    let mut entries = Vec::new();
    for overwrite in overwrites {
        let allow = overwrite.allow.parse::<u128>().ok()?;
        let deny = overwrite.deny.parse::<u128>().ok()?;
        restricted |= deny & 1024 != 0;
        if allow & 1024 != 0 {
            entries.push(format!("allow:{}", overwrite.id));
        } else if deny & 1024 != 0 {
            entries.push(format!("deny:{}", overwrite.id));
        }
    }
    if base & 1024 != 0 && !restricted {
        return Some("everyone".into());
    }
    entries.sort();
    Some(list_hash(entries.join(",").as_bytes()).to_string())
}

// murmurhash3 x86/32, seed zero: the wire identity is a hash of visibility overwrites.
fn list_hash(bytes: &[u8]) -> u32 {
    let scramble = |value: u32| {
        value
            .wrapping_mul(0xcc9e2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b873593)
    };
    let mut hash = 0_u32;
    let (chunks, tail) = bytes.as_chunks::<4>();
    for chunk in chunks {
        hash ^= scramble(u32::from_le_bytes(*chunk));
        hash = hash
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe6546b64);
    }
    let mut last = 0;
    for (index, byte) in tail.iter().enumerate() {
        last |= u32::from(*byte) << (index * 8);
    }
    if !tail.is_empty() {
        hash ^= scramble(last);
    }
    hash ^= bytes.len() as u32;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85ebca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2ae35);
    hash ^ (hash >> 16)
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
        if let (Some(expected), Some(received)) = (self.list_id.as_deref(), data["id"].as_str())
            && expected != received
        {
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
    fn shared_lists_survive_channel_switches_and_reject_other_list_deltas() {
        let mut current = Members::demo();
        current.list_id = Some("everyone".into());
        let mut views = Views::default();
        views.select(
            &mut current,
            Some("larp".into()),
            Some("other".into()),
            Some("everyone".into()),
        );
        assert_eq!(current.rows.len(), 4);
        assert_eq!(current.channel.as_deref(), Some("other"));
        current.apply(&serde_json::json!({"guild_id":"larp","id":"restricted","ops":[{"op":"INVALIDATE","range":[0,99]}]}));
        assert_eq!(current.rows.len(), 4);
        current.apply(&serde_json::json!({"guild_id":"larp","id":"everyone","ops":[{"op":"DELETE","index":3}]}));
        assert_eq!(current.rows.len(), 3);
        views.select(
            &mut current,
            Some("larp".into()),
            Some("private".into()),
            Some("restricted".into()),
        );
        assert!(!current.received);
        assert!(current.rows.is_empty());
        views.select(
            &mut current,
            Some("larp".into()),
            Some("third".into()),
            Some("everyone".into()),
        );
        assert_eq!(current.rows.len(), 3);
        assert_eq!(current.channel.as_deref(), Some("third"));
    }

    #[test]
    fn member_list_identity_uses_wire_metadata_or_visibility_overwrites() {
        assert_eq!(list_hash(b""), 0);
        assert_eq!(list_hash(b"foo"), 0xf6a5c420);
        assert_eq!(list_hash(b"hello"), 0x248bfa47);
        let roles: Vec<crate::model::Role> =
            serde_json::from_value(serde_json::json!([{"id":"1","permissions":"1024"}])).unwrap();
        let mut channel: crate::model::Channel = serde_json::from_value(
            serde_json::json!({"id":"2","guild_id":"1","permission_overwrites":[]}),
        )
        .unwrap();
        assert_eq!(list_identity(&channel, &roles).as_deref(), Some("everyone"));
        channel.permission_overwrites = Some(vec![crate::model::MemberOverwrite {
            id: "1".into(),
            allow: "0".into(),
            deny: "1024".into(),
        }]);
        assert_eq!(
            list_identity(&channel, &roles),
            Some(list_hash(b"deny:1").to_string())
        );
        channel.member_list_id = Some("wire-value".into());
        assert_eq!(list_identity(&channel, &[]).as_deref(), Some("wire-value"));
        channel.kind = 11;
        assert!(list_identity(&channel, &roles).is_none());
    }

    #[test]
    fn returning_to_a_recent_channel_keeps_rows_without_mixing_channel_views() {
        let mut current = Members::demo();
        let mut views = Views::default();
        views.select(
            &mut current,
            Some("larp".into()),
            Some("other".into()),
            None,
        );
        assert!(!current.received);
        assert!(current.rows.is_empty());
        views.select(
            &mut current,
            Some("larp".into()),
            Some("campfire".into()),
            None,
        );
        assert!(current.received);
        assert_eq!(current.rows.len(), 4);
        views.select(
            &mut current,
            Some("larp".into()),
            Some("campfire".into()),
            None,
        );
        assert_eq!(current.rows.len(), 4);
        for n in 0..8 {
            current.received = true;
            views.select(&mut current, Some("larp".into()), Some(n.to_string()), None);
        }
        assert_eq!(views.0.len(), 2);
    }
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
