// normal-client settings-proto/2 interop. retain unrecognized records byte for byte.
pub mod emoji_usage;

use crate::expressions::Gif;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::{BTreeMap, HashSet};
#[derive(Clone)]
struct Field {
    tag: u64,
    wire: u8,
    raw: Vec<u8>,
    data: Vec<u8>,
    number: u64,
}
fn varint(bytes: &[u8], i: &mut usize) -> Option<u64> {
    let mut n = 0;
    for shift in (0..70).step_by(7) {
        let b = *bytes.get(*i)?;
        *i += 1;
        if shift == 63 && b > 1 {
            return None;
        }
        n |= u64::from(b & 127) << shift;
        if b & 128 == 0 {
            return Some(n);
        }
    }
    None
}
fn fields(bytes: &[u8]) -> Option<Vec<Field>> {
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if out.len() >= 10000 {
            return None;
        }
        let start = i;
        let key = varint(bytes, &mut i)?;
        let tag = key >> 3;
        let wire = (key & 7) as u8;
        if tag == 0 {
            return None;
        }
        let mut number = 0;
        let data = match wire {
            0 => {
                number = varint(bytes, &mut i)?;
                Vec::new()
            }
            1 | 5 => {
                let end = i.checked_add(if wire == 1 { 8 } else { 4 })?;
                let data = bytes.get(i..end)?.to_vec();
                i = end;
                data
            }
            2 => {
                let len = usize::try_from(varint(bytes, &mut i)?).ok()?;
                let end = i.checked_add(len)?;
                let data = bytes.get(i..end)?.to_vec();
                i = end;
                data
            }
            _ => return None,
        };
        out.push(Field {
            tag,
            wire,
            raw: bytes[start..i].to_vec(),
            data,
            number,
        });
    }
    Some(out)
}
fn unique(fs: &[Field], tag: u64, wire: u8) -> Option<Option<&Field>> {
    let mut iter = fs.iter().filter(|f| f.tag == tag);
    let f = iter.next();
    if iter.next().is_some() || f.is_some_and(|f| f.wire != wire) {
        None
    } else {
        Some(f)
    }
}
fn put(n: u64, out: &mut Vec<u8>) {
    let mut n = n;
    while n >= 128 {
        out.push(n as u8 | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
fn blob(tag: u64, bytes: &[u8], out: &mut Vec<u8>) {
    put(tag << 3 | 2, out);
    put(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}
fn integer(tag: u64, n: u64, out: &mut Vec<u8>) {
    put(tag << 3, out);
    put(n, out);
}
struct Entry {
    url: String,
    preview: String,
    order: u64,
    raw: Vec<u8>,
    width: u32,
    height: u32,
}
fn entry(f: &Field) -> Option<Entry> {
    if f.wire != 2 || f.data.len() > 8192 {
        return None;
    }
    let fs = fields(&f.data)?;
    let url = String::from_utf8(unique(&fs, 1, 2)??.data.clone()).ok()?;
    if url.is_empty() || url.len() > 2048 {
        return None;
    }
    let value = unique(&fs, 2, 2)?
        .map(|v| v.data.as_slice())
        .unwrap_or_default();
    let values = fields(value)?;
    let preview = unique(&values, 2, 2)?
        .map(|f| String::from_utf8(f.data.clone()).ok())
        .transpose_option()?;
    let num = |tag| unique(&values, tag, 0).map(|v| v.map_or(0, |f| f.number));
    Some(Entry {
        url,
        preview: preview.unwrap_or_default(),
        order: num(5)?,
        width: num(3)?.min(4096) as u32,
        height: num(4)?.min(4096) as u32,
        raw: f.raw.clone(),
    })
}
trait OptionTranspose<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> OptionTranspose<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(Some(v)) => Some(Some(v)),
            _ => None,
        }
    }
}
pub struct Favorites {
    pub version: u64,
    entries: Vec<Entry>,
    extra: Vec<Vec<u8>>,
}
impl Favorites {
    pub fn decode(value: &serde_json::Value) -> Result<Self, String> {
        Self::read(value).ok_or_else(||"Discord GIF favorites could not be read safely. Your saved favorites were left untouched.".into())
    }
    fn read(value: &serde_json::Value) -> Option<Self> {
        if value["out_of_date"].as_bool() == Some(true) {
            return None;
        }
        let encoded = value["settings"].as_str()?;
        if encoded.len() > 6 * 1024 * 1024 {
            return None;
        }
        let bytes = STANDARD.decode(encoded).ok()?;
        let root = fields(&bytes)?;
        let version = if let Some(v) = unique(&root, 1, 2)? {
            let vs = fields(&v.data)?;
            unique(&vs, 3, 0)?.map_or(0, |f| f.number)
        } else {
            0
        };
        let data = unique(&root, 2, 2)?
            .map(|f| f.data.as_slice())
            .unwrap_or_default();
        if data.len() > 512 * 1024 {
            return None;
        }
        let mut entries = Vec::new();
        let mut extra = Vec::new();
        let mut seen = HashSet::new();
        for f in fields(data)? {
            if f.tag == 1 {
                let e = entry(&f)?;
                if entries.len() >= 2048 || !seen.insert(e.url.clone()) {
                    return None;
                }
                entries.push(e);
            } else {
                extra.push(f.raw);
            }
        }
        Some(Self {
            version,
            entries,
            extra,
        })
    }
    pub fn contains(&self, url: &str) -> bool {
        self.entries.iter().any(|e| e.url == url)
    }
    pub fn gifs(&self) -> Vec<Gif> {
        let mut refs = self.entries.iter().collect::<Vec<_>>();
        refs.sort_by_key(|e| std::cmp::Reverse(e.order));
        refs.into_iter()
            .filter(|e| crate::expressions::gif_url(&e.url) || crate::media::cdn_url(&e.url))
            .take(100)
            .map(|e| Gif {
                title: "Saved GIF".into(),
                url: e.url.clone(),
                preview: if crate::media::gif_preview(&e.preview)
                    || crate::media::cdn_url(&e.preview)
                {
                    Some(e.preview.clone())
                } else {
                    None
                },
                thumbnail: None,
                width: e.width,
                height: e.height,
            })
            .collect()
    }
    pub fn patch(&self, gif: &Gif, save: bool) -> Result<String, String> {
        let mut body = Vec::new();
        for raw in &self.extra {
            body.extend_from_slice(raw);
        }
        for e in &self.entries {
            if e.url != gif.url {
                body.extend_from_slice(&e.raw);
            }
        }
        if save {
            if self.entries.len() >= 2048 && !self.contains(&gif.url) {
                return Err("Your favorites list is full.".into());
            }
            let order = self
                .entries
                .iter()
                .map(|e| e.order)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Favorites ordering limit reached")?;
            let mut value = Vec::new();
            let source = gif.preview.as_deref().unwrap_or(&gif.url);
            integer(
                1,
                if source
                    .split('?')
                    .next()
                    .is_some_and(|s| s.ends_with(".mp4"))
                {
                    2
                } else {
                    1
                },
                &mut value,
            );
            blob(2, source.as_bytes(), &mut value);
            integer(3, gif.width.max(1) as u64, &mut value);
            integer(4, gif.height.max(1) as u64, &mut value);
            integer(5, order, &mut value);
            let mut e = Vec::new();
            blob(1, gif.url.as_bytes(), &mut e);
            blob(2, &value, &mut e);
            blob(1, &e, &mut body);
        }
        if body.len() > 512 * 1024 {
            return Err("Favorites data is too large to update safely.".into());
        }
        let mut root = Vec::new();
        blob(2, &body, &mut root);
        Ok(STANDARD.encode(root))
    }
    pub fn confirms(&self, old: &Self, url: &str, saved: bool) -> bool {
        let rest = |f: &Self| {
            f.entries
                .iter()
                .filter(|e| e.url != url)
                .map(|e| (e.url.clone(), e.raw.clone()))
                .collect::<BTreeMap<_, _>>()
        };
        self.version > old.version
            && self.contains(url) == saved
            && self.extra == old.extra
            && rest(self) == rest(old)
    }
}
/// Read synced expression rankings without writing another client's settings.
#[derive(Default)]
pub struct Recents {
    pub messages: Vec<String>,
    pub reactions: Vec<String>,
    pub stickers: Vec<String>,
}
pub fn emoji_recents(value: &serde_json::Value) -> Result<Recents, String> {
    let read = || -> Option<Recents> {
        let encoded = value["settings"].as_str()?;
        if encoded.len() > 6 * 1024 * 1024 || value["out_of_date"].as_bool() == Some(true) {
            return None;
        }
        let root = fields(&STANDARD.decode(encoded).ok()?)?;
        let rank = |tag, fixed: bool| -> Option<Vec<String>> {
            let Some(tree) = unique(&root, tag, 2)? else {
                return Some(vec![]);
            };
            if tree.data.len() > 512 * 1024 {
                return None;
            }
            let mut entries = BTreeMap::new();
            for f in fields(&tree.data)?
                .into_iter()
                .filter(|f| f.tag == 1 && f.wire == 2)
                .take(2048)
            {
                let Some(entry) = fields(&f.data) else {
                    continue;
                };
                let Some(Some(key)) = unique(&entry, 1, if fixed { 1 } else { 2 }) else {
                    continue;
                };
                let key = if fixed {
                    u64::from_le_bytes(key.data.as_slice().try_into().ok()?).to_string()
                } else {
                    let Ok(key) = String::from_utf8(key.data.clone()) else {
                        continue;
                    };
                    key
                };
                if key.is_empty()
                    || key.len() > 64
                    || key.chars().any(char::is_control)
                    || (fixed && key == "0")
                {
                    continue;
                }
                let Some(Some(item)) = unique(&entry, 2, 2) else {
                    continue;
                };
                let Some(item) = fields(&item.data) else {
                    continue;
                };
                let score = unique(&item, 4, 0)?.map_or(0, |f| f.number as i32);
                let uses = unique(&item, 1, 0)?.map_or(0, |f| f.number);
                entries.insert(key, (score, uses));
            }
            let mut ranked = entries.into_iter().collect::<Vec<_>>();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            Some(ranked.into_iter().take(64).map(|(k, _)| k).collect())
        };
        Some(Recents {
            messages: rank(6, false)?,
            reactions: rank(13, false)?,
            stickers: rank(4, true)?.into_iter().take(32).collect(),
        })
    };
    read().ok_or_else(|| {
        "Could not load Discord's recent emoji and stickers. Local recents are still available."
            .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(body: &[u8], version: u64) -> serde_json::Value {
        let mut v = Vec::new();
        integer(3, version, &mut v);
        let mut root = Vec::new();
        blob(1, &v, &mut root);
        root.extend_from_slice(body);
        serde_json::json!({"settings":STANDARD.encode(root)})
    }
    #[test]
    fn account_emoji_rankings_keep_message_and_reaction_history_separate() {
        let subtree = |keys: &[(&str, u64)]| {
            let mut out = vec![];
            for (key, score) in keys {
                let mut entry = vec![];
                blob(1, key.as_bytes(), &mut entry);
                let mut value = vec![];
                integer(4, *score, &mut value);
                integer(1, 2, &mut value);
                blob(2, &value, &mut entry);
                blob(1, &entry, &mut out);
            }
            out
        };
        let mut root = vec![];
        blob(6, &subtree(&[("sob", 1), ("123", 9)]), &mut root);
        blob(13, &subtree(&[("heart", 8)]), &mut root);
        blob(99, b"unknown", &mut root);
        let value = serde_json::json!({"settings":STANDARD.encode(&root)});
        let recents = emoji_recents(&value).unwrap();
        let messages = recents.messages;
        let reactions = recents.reactions;
        assert_eq!(messages, vec!["123", "sob"]);
        assert_eq!(reactions, vec!["heart"]);
        assert_eq!(value["settings"].as_str().unwrap(), STANDARD.encode(&root));
    }
    #[test]
    fn sticker_rankings_use_fixed64_ids_and_keep_other_settings_untouched() {
        let mut stickers = vec![];
        for (id, score) in [(1560000000000000001u64, 1), (1560000000000000002, 9)] {
            let mut entry = vec![];
            put((1 << 3) | 1, &mut entry);
            entry.extend_from_slice(&id.to_le_bytes());
            let mut item = vec![];
            integer(4, score, &mut item);
            blob(2, &item, &mut entry);
            blob(1, &entry, &mut stickers);
        }
        let mut root = vec![];
        blob(4, &stickers, &mut root);
        blob(99, b"future setting", &mut root);
        let value = serde_json::json!({"settings":STANDARD.encode(&root)});
        assert_eq!(
            emoji_recents(&value).unwrap().stickers,
            vec!["1560000000000000002", "1560000000000000001"]
        );
        assert_eq!(
            STANDARD
                .decode(value["settings"].as_str().unwrap())
                .unwrap(),
            root
        );
        let malformed = serde_json::json!({"settings":STANDARD.encode([0x22,0xff])});
        assert!(emoji_recents(&malformed).is_err());
    }
    #[test]
    fn favorite_changes_preserve_unknown_entries_and_fail_closed() {
        let mut subtree = Vec::new();
        blob(42, b"future setting", &mut subtree);
        let mut unknown_value = Vec::new();
        blob(42, b"future entry property", &mut unknown_value);
        let mut unknown = Vec::new();
        blob(1, b"future://unsupported-favorite", &mut unknown);
        blob(2, &unknown_value, &mut unknown);
        blob(1, &unknown, &mut subtree);
        let mut body = Vec::new();
        blob(2, &subtree, &mut body);
        let empty = Favorites::decode(&response(&body, 9)).unwrap();
        let gif = Gif {
            title: "test".into(),
            url: "https://tenor.com/view/cat-1".into(),
            preview: Some("https://media.tenor.com/cat.gif".into()),
            thumbnail: None,
            width: 200,
            height: 100,
        };
        let patch = STANDARD.decode(empty.patch(&gif, true).unwrap()).unwrap();
        let saved = Favorites::decode(&response(&patch, 10)).unwrap();
        assert!(saved.confirms(&empty, &gif.url, true));
        assert_eq!(saved.gifs()[0].width, 200);
        let removed = STANDARD.decode(saved.patch(&gif, false).unwrap()).unwrap();
        let removed = Favorites::decode(&response(&removed, 11)).unwrap();
        assert!(removed.confirms(&saved, &gif.url, false));
        assert_eq!(removed.extra, empty.extra);
        let mut duplicate = patch.clone();
        duplicate.extend_from_slice(&patch);
        assert!(Favorites::decode(&response(&duplicate, 12)).is_err());
        assert!(Favorites::decode(&serde_json::json!({"settings":"invalid"})).is_err());
    }
}
