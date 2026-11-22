//! Account emoji usage, stored in fields 6 and 13 of Discord's frecency settings.
use super::{blob, fields, integer, put, unique, varint};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_KEYS: usize = 64;
const MAX_TIMES: usize = 10;
const MAX_SUBTREE: usize = 512 * 1024;
const DAY_MS: u64 = 86_400_000;

#[derive(Default)]
pub struct Batch(BTreeMap<(bool, String), Use>);

#[derive(Default)]
struct Use {
    count: u32,
    times: Vec<u64>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn usage_key(token: &str) -> Option<String> {
    use crate::{content::Part, expressions};
    if let [Part::Emoji { id, .. }] = crate::content::parts(token).as_slice() {
        return Some((*id).to_owned());
    }
    if expressions::id(token) {
        return Some(token.to_owned());
    }
    if let Some((_, id)) = token.rsplit_once(':')
        && expressions::id(id)
    {
        return Some(id.to_owned());
    }
    let glyph = expressions::unicode_alias(token).unwrap_or(token);
    static NAMES: OnceLock<HashMap<&str, &str>> = OnceLock::new();
    NAMES
        .get_or_init(|| expressions::unicode_catalog().iter().copied().collect())
        .get(glyph)
        .map(|name| (*name).to_owned())
}

impl Batch {
    pub fn record(&mut self, token: &str, reaction: bool, now: u64) -> bool {
        let Some(key) = usage_key(token) else {
            return false;
        };
        let key = (reaction, key);
        if self.0.len() >= MAX_KEYS && !self.0.contains_key(&key) {
            return false;
        }
        let used = self.0.entry(key).or_default();
        used.count = used.count.saturating_add(1);
        used.times.push(now);
        keep_newest(&mut used.times);
        true
    }
}

fn keep_newest(times: &mut Vec<u64>) {
    times.sort_unstable();
    times.drain(..times.len().saturating_sub(MAX_TIMES));
}

fn score(count: u32, times: &[u64], now: u64) -> u64 {
    let weights = times
        .iter()
        .map(|time| match now.saturating_sub(*time) / DAY_MS {
            0..=3 => 100,
            4..=15 => 70,
            16..=30 => 50,
            31..=45 => 30,
            46..=80 => 10,
            _ => 0,
        })
        .sum::<u64>();
    (u64::from(count) * weights / times.len().max(1) as u64).min(i32::MAX as u64)
}

fn update_item(raw: &[u8], used: &Use, now: u64) -> Option<Vec<u8>> {
    let item = fields(raw)?;
    let previous = u32::try_from(unique(&item, 1, 0)?.map_or(0, |f| f.number)).ok()?;
    unique(&item, 4, 0)?;
    let count = previous.saturating_add(used.count);
    let mut times = used.times.clone();
    for f in item.iter().filter(|f| f.tag == 2) {
        match f.wire {
            0 => times.push(f.number),
            2 => {
                let mut offset = 0;
                while offset < f.data.len() {
                    times.push(varint(&f.data, &mut offset)?);
                    if times.len() > 4 * MAX_TIMES {
                        keep_newest(&mut times);
                    }
                }
            }
            _ => return None,
        }
    }
    keep_newest(&mut times);
    let mut bytes = Vec::new();
    integer(1, count.into(), &mut bytes);
    let mut packed = Vec::new();
    for at in &times {
        put(*at, &mut packed);
    }
    blob(2, &packed, &mut bytes);
    integer(4, score(count, &times, now), &mut bytes);
    for f in item.iter().filter(|f| !matches!(f.tag, 1 | 2 | 4)) {
        bytes.extend_from_slice(&f.raw);
    }
    Some(bytes)
}

fn read(value: &serde_json::Value) -> Option<(u64, Vec<super::Field>)> {
    if value["out_of_date"].as_bool() == Some(true) {
        return None;
    }
    let encoded = value["settings"].as_str()?;
    if encoded.len() > 6 * 1024 * 1024 {
        return None;
    }
    let root = fields(&STANDARD.decode(encoded).ok()?)?;
    let version = unique(&fields(&unique(&root, 1, 2)??.data)?, 3, 0)??.number;
    Some((version, root))
}

pub fn confirms(value: &serde_json::Value, previous: u64) -> bool {
    read(value).is_some_and(|(version, _)| version > previous)
}

pub fn patch(value: &serde_json::Value, batch: &Batch, now: u64) -> Option<(u64, String)> {
    let (version, root) = read(value)?;
    let mut output = Vec::new();
    for (reaction, tag) in [(false, 6), (true, 13)] {
        let uses: BTreeMap<_, _> = batch
            .0
            .iter()
            .filter(|((kind, _), _)| *kind == reaction)
            .map(|((_, key), used)| (key.as_str(), used))
            .collect();
        if uses.is_empty() {
            continue;
        }
        let current = unique(&root, tag, 2)?.map_or(&[][..], |field| &field.data);
        if current.len() > MAX_SUBTREE {
            return None;
        }
        let old = fields(current)?;
        let mut subtree = Vec::new();
        let mut written = HashSet::new();
        let mut entries = 0;
        for record in &old {
            if record.tag != 1 {
                subtree.extend_from_slice(&record.raw);
                continue;
            }
            entries += 1;
            if record.wire != 2 {
                return None;
            }
            let parts = fields(&record.data)?;
            let key = unique(&parts, 1, 2)??;
            let key = std::str::from_utf8(&key.data).ok()?;
            let canonical = if uses.contains_key(key) {
                None
            } else {
                usage_key(key)
            };
            let key = canonical.as_deref().unwrap_or(key);
            if let Some(used) = uses.get(key) {
                if !written.insert(key.to_owned()) {
                    // Two aliases for the same emoji would make this update ambiguous.
                    return None;
                }
                let item = unique(&parts, 2, 2)?.map_or(&[][..], |field| &field.data);
                let mut entry = Vec::new();
                for part in parts.iter().filter(|part| part.tag != 2) {
                    entry.extend_from_slice(&part.raw);
                }
                blob(2, &update_item(item, used, now)?, &mut entry);
                blob(1, &entry, &mut subtree);
            } else {
                subtree.extend_from_slice(&record.raw);
            }
        }
        for (key, used) in uses {
            if !written.contains(key) {
                if entries >= 2048 {
                    return None;
                }
                let mut entry = Vec::new();
                blob(1, key.as_bytes(), &mut entry);
                blob(2, &update_item(&[], used, now)?, &mut entry);
                blob(1, &entry, &mut subtree);
                entries += 1;
            }
        }
        if subtree.len() > MAX_SUBTREE {
            return None;
        }
        blob(tag, &subtree, &mut output);
    }
    (!output.is_empty()).then(|| (version, STANDARD.encode(output)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(version: u64, root: &[u8]) -> serde_json::Value {
        let mut versions = Vec::new();
        integer(3, version, &mut versions);
        let mut settings = Vec::new();
        blob(1, &versions, &mut settings);
        settings.extend_from_slice(root);
        serde_json::json!({"settings": STANDARD.encode(settings)})
    }

    fn entry(key: &str, count: u64, time: u64, out: &mut Vec<u8>) {
        let mut record = Vec::new();
        let mut item = Vec::new();
        integer(1, count, &mut item);
        integer(2, time, &mut item);
        integer(3, 17, &mut item);
        integer(4, count * 100, &mut item);
        blob(1, key.as_bytes(), &mut record);
        blob(2, &item, &mut record);
        integer(31, 42, &mut record);
        blob(1, &record, out);
    }

    #[test]
    fn batches_usage_into_versioned_emoji_fields_and_keeps_unrelated_bytes() {
        let now = 20 * DAY_MS;
        let mut existing = Vec::new();
        entry("🥀", 5, now - DAY_MS, &mut existing);
        let mut untouched = Vec::new();
        entry("skull", 10, now, &mut untouched);
        existing.extend_from_slice(&untouched);
        let mut settings = Vec::new();
        blob(6, &existing, &mut settings);
        blob(2, b"favorite GIF payload stays untouched", &mut settings);
        let mut batch = Batch::default();
        assert!(batch.record("🥀", false, now - 200));
        assert!(batch.record("wilted_rose", false, now));
        assert!(batch.record("custom:123", true, now));
        let (version, encoded) = patch(&response(8, &settings), &batch, now).unwrap();
        assert_eq!(version, 8);
        let bytes = STANDARD.decode(encoded).unwrap();
        let root = fields(&bytes).unwrap();
        assert_eq!(
            root.iter().map(|field| field.tag).collect::<Vec<_>>(),
            [6, 13]
        );
        let messages = fields(&root[0].data).unwrap();
        assert_eq!(messages[1].raw, untouched);
        let record = fields(&messages[0].data).unwrap();
        assert_eq!(
            unique(&record, 1, 2).unwrap().unwrap().data,
            "🥀".as_bytes()
        );
        assert_eq!(unique(&record, 31, 0).unwrap().unwrap().number, 42);
        let used = fields(&unique(&record, 2, 2).unwrap().unwrap().data).unwrap();
        assert_eq!(unique(&used, 1, 0).unwrap().unwrap().number, 7);
        assert_eq!(unique(&used, 3, 0).unwrap().unwrap().number, 17);
        assert_eq!(unique(&used, 4, 0).unwrap().unwrap().number, 700);
        let reaction = fields(&root[1].data).unwrap();
        let record = fields(&reaction[0].data).unwrap();
        assert_eq!(unique(&record, 1, 2).unwrap().unwrap().data, b"123");
        assert!(confirms(&response(9, &bytes), version));
        assert!(!confirms(&response(8, &bytes), version));
    }

    #[test]
    fn refuses_conflicts_and_ambiguous_emoji_aliases() {
        let now = 20 * DAY_MS;
        let mut existing = Vec::new();
        entry("🥀", 5, now, &mut existing);
        entry("wilted_flower", 2, now, &mut existing);
        let mut settings = Vec::new();
        blob(6, &existing, &mut settings);
        let mut batch = Batch::default();
        assert!(batch.record("🥀", false, now));
        assert!(patch(&response(8, &settings), &batch, now).is_none());
        let mut stale = response(8, &[]);
        stale["out_of_date"] = serde_json::json!(true);
        assert!(patch(&stale, &batch, now).is_none());
        assert!(!confirms(&stale, 7));
    }
}
