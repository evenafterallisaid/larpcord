use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

pub const CHANNEL_LIMIT: usize = 12;
pub const MESSAGE_LIMIT: usize = 200;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub accent_color: Option<u32>,
    #[serde(default)]
    pub bot: bool,
}

impl User {
    pub fn name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct Guild {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Default, Deserialize)]
pub struct Channel {
    pub id: String,
    pub guild_id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: u8,
    #[serde(default)]
    pub position: i32,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub recipients: Vec<User>,
    pub topic: Option<String>,
    pub last_message_id: Option<String>,
}

impl Channel {
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            let names = self.recipients.iter().map(User::name).collect::<Vec<_>>();
            if names.is_empty() {
                "Direct message".into()
            } else {
                names.join(", ")
            }
        })
    }

    pub fn readable(&self) -> bool {
        matches!(self.kind, 0 | 1 | 3 | 5 | 10..=12)
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub url: String,
    pub proxy_url: Option<String>,
    pub content_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Default, Deserialize)]
pub struct EmbedMedia {
    pub url: Option<String>,
    pub proxy_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Default, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Default, Deserialize)]
pub struct EmbedAuthor {
    pub name: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct EmbedFooter {
    pub text: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Embed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub color: Option<u32>,
    pub author: Option<EmbedAuthor>,
    pub footer: Option<EmbedFooter>,
    pub image: Option<EmbedMedia>,
    pub thumbnail: Option<EmbedMedia>,
    #[serde(default)]
    pub fields: Vec<EmbedField>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Member {
    pub nick: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Latest,
    Before,
    After,
}

#[derive(Clone, Default, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub author: User,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(rename = "type", default)]
    pub kind: u8,
    pub member: Option<Member>,
    pub webhook_id: Option<String>,
    pub message_reference: Option<serde_json::Value>,
}

impl Message {
    pub fn name(&self) -> &str {
        self.member
            .as_ref()
            .and_then(|m| m.nick.as_deref())
            .unwrap_or_else(|| self.author.name())
    }

    pub fn continues(&self, previous: &Self) -> bool {
        if self.author.id != previous.author.id
            || self.name() != previous.name()
            || self.author.avatar != previous.author.avatar
            || self.channel_id != previous.channel_id
            || self.webhook_id != previous.webhook_id
            || self.kind != 0
            || previous.kind != 0
            || self.message_reference.is_some()
            || previous.message_reference.is_some()
        {
            return false;
        }
        let (Ok(a), Ok(b)) = (
            chrono::DateTime::parse_from_rfc3339(&previous.timestamp),
            chrono::DateTime::parse_from_rfc3339(&self.timestamp),
        ) else {
            return false;
        };
        let seconds = (b - a).num_seconds();
        (0..=300).contains(&seconds)
            && a.with_timezone(&chrono::Local).date_naive()
                == b.with_timezone(&chrono::Local).date_naive()
    }
}

#[derive(Default)]
struct History {
    messages: Vec<Message>,
    revisions: HashMap<String, u64>,
    deleted: HashMap<String, u64>,
    patches: HashMap<String, serde_json::Value>,
}

#[derive(Default)]
pub struct Store {
    pub user: User,
    pub guilds: Vec<Guild>,
    pub channels: HashMap<String, Channel>,
    pub unread: HashSet<String>,
    histories: HashMap<String, History>,
    recent: VecDeque<String>,
    revision: u64,
    pinned: Option<String>,
}

impl Store {
    pub fn channel_messages(&self, id: &str) -> &[Message] {
        self.histories
            .get(id)
            .map_or(&[], |h| h.messages.as_slice())
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn has_history(&self, channel: &str) -> bool {
        self.histories.contains_key(channel)
    }

    pub fn pin(&mut self, channel: &str) {
        self.pinned = Some(channel.into());
        self.history(channel);
    }

    pub fn cached_messages(&self) -> usize {
        self.histories.values().map(|h| h.messages.len()).sum()
    }

    pub fn clear_history(&mut self) {
        self.histories.clear();
        self.recent.clear();
    }

    fn history(&mut self, channel: &str) -> &mut History {
        self.recent.retain(|id| id != channel);
        self.recent.push_back(channel.into());
        while self.recent.len() > CHANNEL_LIMIT {
            if let Some(index) = self
                .recent
                .iter()
                .position(|id| Some(id) != self.pinned.as_ref())
                && let Some(old) = self.recent.remove(index)
            {
                self.histories.remove(&old);
            }
        }
        self.histories.entry(channel.into()).or_default()
    }

    pub fn insert(&mut self, message: Message) {
        self.revision += 1;
        let revision = self.revision;
        let history = self.history(&message.channel_id);
        history.deleted.remove(&message.id);
        history.patches.remove(&message.id);
        history.revisions.insert(message.id.clone(), revision);
        match history.messages.iter().position(|m| m.id == message.id) {
            Some(index) => history.messages[index] = message,
            None => history.messages.push(message),
        }
        Self::trim(history, false);
    }

    pub fn update(&mut self, patch: &serde_json::Value) {
        let (Some(channel), Some(id)) = (patch["channel_id"].as_str(), patch["id"].as_str()) else {
            return;
        };
        self.revision += 1;
        let revision = self.revision;
        let Some(history) = self.histories.get_mut(channel) else {
            return;
        };
        history.revisions.insert(id.into(), revision);
        let Some(message) = history.messages.iter_mut().find(|m| m.id == id) else {
            let fields = history
                .patches
                .entry(id.into())
                .or_insert_with(|| serde_json::json!({}));
            for key in ["content", "edited_timestamp", "attachments", "embeds"] {
                if let Some(value) = patch.get(key) {
                    fields[key] = value.clone();
                }
            }
            if history.patches.len() > MESSAGE_LIMIT * 2
                && let Some(oldest) = history
                    .patches
                    .keys()
                    .min_by_key(|id| history.revisions.get(*id))
                    .cloned()
            {
                history.patches.remove(&oldest);
                history.revisions.remove(&oldest);
            }
            return;
        };
        Self::apply_patch(message, patch);
    }

    pub fn apply_patch(message: &mut Message, patch: &serde_json::Value) {
        if let Some(content) = patch["content"].as_str() {
            message.content = content.into();
        }
        if let Some(edited) = patch["edited_timestamp"].as_str() {
            message.edited_timestamp = Some(edited.into());
        }
        if let Some(attachments) = patch
            .get("attachments")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        {
            message.attachments = attachments;
        }
        if let Some(embeds) = patch
            .get("embeds")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        {
            message.embeds = embeds;
        }
    }

    pub fn delete(&mut self, channel: &str, id: &str) {
        self.revision += 1;
        let revision = self.revision;
        let history = self.history(channel);
        history.messages.retain(|m| m.id != id);
        history.revisions.remove(id);
        history.patches.remove(id);
        history.deleted.insert(id.into(), revision);
        // tombstones protect a concurrent history fetch from resurrecting deletions.
        if history.deleted.len() > MESSAGE_LIMIT * 2
            && let Some(oldest) = history
                .deleted
                .iter()
                .min_by_key(|(_, r)| *r)
                .map(|(id, _)| id.clone())
        {
            history.deleted.remove(&oldest);
        }
    }

    pub fn merge_history(
        &mut self,
        channel: &str,
        fetched: Vec<Message>,
        started: u64,
        page: Page,
    ) {
        let history = self.history(channel);
        if page == Page::Latest {
            history.messages.retain(|message| {
                history
                    .revisions
                    .get(&message.id)
                    .is_some_and(|revision| *revision > started)
            });
        }
        for mut message in fetched {
            if history
                .deleted
                .get(&message.id)
                .is_some_and(|r| *r > started)
            {
                continue;
            }
            if let Some(patch) = history.patches.remove(&message.id) {
                Self::apply_patch(&mut message, &patch);
                history.messages.push(message);
                continue;
            }
            if history
                .revisions
                .get(&message.id)
                .is_some_and(|r| *r > started)
            {
                continue;
            }
            match history.messages.iter().position(|m| m.id == message.id) {
                Some(index) => history.messages[index] = message,
                None => history.messages.push(message),
            }
        }
        Self::trim(history, page == Page::Before);
    }

    fn trim(history: &mut History, keep_oldest: bool) {
        history
            .messages
            .sort_by(|a, b| a.id.len().cmp(&b.id.len()).then_with(|| a.id.cmp(&b.id)));
        let excess = history.messages.len().saturating_sub(MESSAGE_LIMIT);
        let discarded = if keep_oldest {
            history.messages.len() - excess..history.messages.len()
        } else {
            0..excess
        };
        for old in history.messages.drain(discarded) {
            history.revisions.remove(&old.id);
        }
        history.revisions.retain(|id, _| {
            history.messages.iter().any(|message| message.id == *id)
                || history.patches.contains_key(id)
        });
    }

    pub fn demo() -> Self {
        let mut store = Self {
            user: User {
                id: "me".into(),
                username: "you".into(),
                ..User::default()
            },
            ..Self::default()
        };
        store.guilds.push(Guild {
            id: "larp".into(),
            name: "The little guild".into(),
        });
        for (id, name, topic, kind) in [
            ("campfire", "campfire", "A quiet corner of the internet.", 0),
            ("build-log", "build-log", "Small steps, less overhead.", 0),
            ("side-quests", "side-quests", "Things for another day.", 0),
            (
                "voice",
                "The tavern",
                "Voice is coming in a later milestone.",
                2,
            ),
        ] {
            store.channels.insert(
                id.into(),
                Channel {
                    id: id.into(),
                    name: Some(name.into()),
                    guild_id: Some("larp".into()),
                    topic: Some(topic.into()),
                    kind,
                    ..Channel::default()
                },
            );
        }
        for (index, (name, text)) in [
            (
                "moss",
                "found a little place for all of us. shoes off, make yourself at home.",
            ),
            ("clover", "this feels nice. what are we building?"),
            (
                "moss",
                "larpcord. a tiny native client with a little more room to breathe.",
            ),
            ("clover", "less background noise, more hanging out. i'm in."),
        ]
        .into_iter()
        .enumerate()
        {
            store.insert(Message {
                id: format!("{}", index + 1),
                channel_id: "campfire".into(),
                author: User {
                    id: name.into(),
                    username: name.into(),
                    ..User::default()
                },
                content: text.into(),
                timestamp: format!("2026-11-12T18:{:02}:00Z", 30 + index),
                ..Message::default()
            });
        }
        let sample: Message = serde_json::from_value(serde_json::json!({
            "id":"5", "channel_id":"campfire", "author":{"id":"clover", "username":"clover"},
            "timestamp":"2026-11-12T18:34:00Z", "content":"same person, a little less clutter. 🙂"
        }))
        .expect("sample message");
        store.insert(sample.clone());
        let mut next = sample.clone();
        next.id = "6".into();
        next.timestamp = "2026-11-12T18:35:00Z".into();
        next.content = "images and bot cards live here too.".into();
        next.attachments.push(Attachment {
            filename: "sample.png".into(),
            url: "https://cdn.discordapp.com/embed/avatars/0.png".into(),
            width: Some(256),
            height: Some(256),
            content_type: Some("image/png".into()),
            ..Attachment::default()
        });
        store.insert(next);
        store.insert(serde_json::from_value(serde_json::json!({
            "id":"7", "channel_id":"campfire", "author":{"id":"helper", "username":"little helper", "bot":true}, "timestamp":"2026-11-12T18:36:00Z",
            "embeds":[{"title":"a native little card", "description":"bot embeds, fields and pictures, without a webview.", "url":"https://github.com/evenafterallisaid/larpcord", "color":5793266,
            "fields":[{"name":"conversation history", "value":"scroll up to browse. jump down to catch up."}, {"name":"profiles", "value":"click a name or avatar to open a profile."}], "footer":{"text":"larpcord preview"}}]
        })).expect("sample embed"));
        store.insert(Message { id: "20".into(), channel_id: "build-log".into(), author: store.user.clone(), content: "Native Rust + GPUI. This preview uses sample messages. Connect your account when you're ready.".into(), ..Message::default() });
        store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: usize, channel: &str) -> Message {
        Message {
            id: id.to_string(),
            channel_id: channel.into(),
            content: "old".into(),
            ..Message::default()
        }
    }

    #[test]
    fn fetch_cannot_overwrite_a_live_edit_or_resurrect_a_deletion() {
        let mut store = Store::default();
        store.insert(message(1, "a"));
        let started = store.revision();
        store.update(&serde_json::json!({"id":"1", "channel_id":"a", "content":"edited"}));
        store.delete("a", "2");
        store.insert(message(3, "a"));
        store.merge_history(
            "a",
            vec![message(1, "a"), message(2, "a")],
            started,
            Page::Latest,
        );
        assert_eq!(store.channel_messages("a").len(), 2);
        assert_eq!(store.channel_messages("a")[0].content, "edited");
        assert_eq!(store.channel_messages("a")[1].id, "3");
    }

    #[test]
    fn cache_is_bounded_and_snowflakes_sort_numerically() {
        let mut store = Store::default();
        for channel in 0..20 {
            for id in 0..300 {
                store.insert(message(id, &channel.to_string()));
            }
        }
        assert_eq!(store.histories.len(), CHANNEL_LIMIT);
        assert_eq!(store.cached_messages(), CHANNEL_LIMIT * MESSAGE_LIMIT);
        assert!(store.channel_messages("0").is_empty());
        assert_eq!(store.channel_messages("19")[0].id, "100");
        assert_eq!(store.channel_messages("19").last().unwrap().id, "299");
    }

    #[test]
    fn edit_before_first_history_response_is_preserved() {
        let mut store = Store::default();
        store.pin("a");
        let started = store.revision();
        store.update(&serde_json::json!({"id":"1", "channel_id":"a", "content":"new"}));
        store.update(&serde_json::json!({"id":"1", "channel_id":"a", "edited_timestamp":"later"}));
        store.merge_history("a", vec![message(1, "a")], started, Page::Latest);
        assert_eq!(store.channel_messages("a")[0].content, "new");
        assert_eq!(
            store.channel_messages("a")[0].edited_timestamp.as_deref(),
            Some("later")
        );
        for id in 0..30 {
            store.insert(message(id, &format!("other{id}")));
        }
        assert_eq!(store.channel_messages("a").len(), 1);
    }

    #[test]
    fn grouping_respects_identity_time_and_reply_boundaries() {
        let a = Message {
            author: User {
                id: "1".into(),
                username: "a".into(),
                ..User::default()
            },
            timestamp: "2026-11-12T10:00:00Z".into(),
            ..Message::default()
        };
        let mut b = a.clone();
        b.timestamp = "2026-11-12T10:04:59Z".into();
        assert!(b.continues(&a));
        b.timestamp = "2026-11-12T10:05:01Z".into();
        assert!(!b.continues(&a));
        b.timestamp = a.timestamp.clone();
        b.author.id = "2".into();
        assert!(!b.continues(&a));
        b.author = a.author.clone();
        b.message_reference = Some(serde_json::json!({"message_id":"5"}));
        assert!(!b.continues(&a));
    }

    #[test]
    fn sliding_history_can_return_to_latest_without_gaps_or_lost_edits() {
        let mut store = Store::default();
        store.merge_history(
            "a",
            (400..500).map(|id| message(id, "a")).collect(),
            0,
            Page::Latest,
        );
        for start in [300, 200, 100, 0] {
            store.merge_history(
                "a",
                (start..start + 100).map(|id| message(id, "a")).collect(),
                store.revision(),
                Page::Before,
            );
        }
        assert_eq!(store.channel_messages("a").last().unwrap().id, "199");
        for start in [200, 300, 400] {
            let revision = store.revision();
            store.update(&serde_json::json!({"id":start.to_string(), "channel_id":"a", "content":"edited", "embeds": [{"title":"new embed"}]}));
            store.merge_history(
                "a",
                (start..start + 100).map(|id| message(id, "a")).collect(),
                revision,
                Page::After,
            );
            let edited = store
                .channel_messages("a")
                .iter()
                .find(|m| m.id == start.to_string())
                .unwrap();
            assert_eq!(edited.content, "edited");
            assert_eq!(edited.embeds[0].title.as_deref(), Some("new embed"));
            assert_eq!(store.channel_messages("a").len(), MESSAGE_LIMIT);
            assert!(store.channel_messages("a").windows(2).all(|p| p[1].id.parse::<u64>().unwrap() == p[0].id.parse::<u64>().unwrap()+1));
        }
        assert_eq!(store.channel_messages("a").last().unwrap().id, "499");
    }

    #[test]
    fn paging_back_moves_the_bounded_window_into_older_history() {
        let mut store = Store::default();
        for id in 100..300 {
            store.insert(message(id, "a"));
        }
        store.merge_history(
            "a",
            (0..100).map(|id| message(id, "a")).collect(),
            store.revision(),
            Page::Before,
        );
        assert_eq!(store.channel_messages("a")[0].id, "0");
        assert_eq!(store.channel_messages("a").last().unwrap().id, "199");
        assert_eq!(store.channel_messages("a").len(), MESSAGE_LIMIT);
    }
}
