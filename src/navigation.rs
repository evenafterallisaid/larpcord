use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Folder {
    pub id: Option<String>,
    pub name: Option<String>,
    pub color: Option<u32>,
    #[serde(default)]
    pub guild_ids: Vec<String>,
}
#[derive(Clone, Default)]
pub struct Layout {
    pub folders: Vec<Folder>,
    pub positions: Vec<String>,
}
impl Layout {
    pub fn parse(value: &Value) -> Option<Self> {
        let value = value.get("user_settings").unwrap_or(value);
        if value.get("guild_folders").is_some() || value.get("guild_positions").is_some() {
            let folders = value["guild_folders"]
                .as_array()
                .into_iter()
                .flatten()
                .take(2000)
                .filter_map(|v| {
                    let mut v = v.clone();
                    if let Some(id) = v["id"].as_u64() {
                        v["id"] = Value::String(id.to_string());
                    }
                    serde_json::from_value(v).ok()
                })
                .collect();
            let positions = value["guild_positions"]
                .as_array()
                .into_iter()
                .flatten()
                .take(2500)
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            return Some(Self { folders, positions });
        }
        let proto = value.get("settings").unwrap_or(value);
        if proto
            .get("type")
            .and_then(Value::as_u64)
            .is_some_and(|t| t != 1)
        {
            return None;
        }
        let encoded = value
            .get("user_settings_proto")
            .and_then(Value::as_str)
            .or_else(|| value.get("settings").and_then(Value::as_str))
            .or_else(|| proto.get("proto").and_then(Value::as_str))?;
        if encoded.len() > 2 * 1024 * 1024 {
            return None;
        }
        let bytes = STANDARD.decode(encoded).ok()?;
        let root = fields(&bytes)?;
        let folder_data = root.iter().find(|(id, _)| *id == 14)?.1.bytes()?;
        let mut layout = Self::default();
        for (id, field) in fields(folder_data)? {
            match id {
                1 => {
                    let mut folder = Folder::default();
                    for (id, field) in fields(field.bytes()?)? {
                        match id {
                            1 => folder.guild_ids.extend(ids(field)?),
                            2 => {
                                folder.id = fields(field.bytes()?)?
                                    .first()
                                    .and_then(|(_, f)| f.number())
                                    .filter(|n| *n > 0)
                                    .map(|id| id.to_string())
                            }
                            3 => {
                                folder.name = fields(field.bytes()?)?
                                    .first()
                                    .and_then(|(_, f)| f.bytes())
                                    .and_then(|v| std::str::from_utf8(v).ok())
                                    .map(|s| s.chars().take(200).collect())
                            }
                            4 => {
                                folder.color = fields(field.bytes()?)?
                                    .first()
                                    .and_then(|(_, f)| f.number())
                                    .map(|n| n as u32 & 0xffffff)
                            }
                            _ => {}
                        }
                    }
                    if layout.folders.len() < 2000 {
                        layout.folders.push(folder);
                    }
                }
                2 => {
                    layout.positions.extend(ids(field)?);
                    if layout.positions.len() > 2500 {
                        return None;
                    }
                }
                _ => {}
            }
        }
        Some(layout)
    }
    pub fn order(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.folders
            .iter()
            .flat_map(|f| f.guild_ids.iter())
            .chain(self.positions.iter())
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Wire<'a> {
    Number(u64),
    Bytes(&'a [u8]),
}
impl<'a> Wire<'a> {
    fn bytes(self) -> Option<&'a [u8]> {
        match self {
            Self::Bytes(b) => Some(b),
            _ => None,
        }
    }
    fn number(self) -> Option<u64> {
        match self {
            Self::Number(n) => Some(n),
            _ => None,
        }
    }
}
fn varint(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let mut n = 0u64;
    for shift in (0..70).step_by(7) {
        let b = *bytes.get(*offset)?;
        *offset += 1;
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
fn take<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = offset.checked_add(len)?;
    let part = bytes.get(*offset..end)?;
    *offset = end;
    Some(part)
}
fn fields(bytes: &[u8]) -> Option<Vec<(u64, Wire<'_>)>> {
    let mut offset = 0;
    let mut out = Vec::new();
    while offset < bytes.len() {
        if out.len() > 10000 {
            return None;
        }
        let tag = varint(bytes, &mut offset)?;
        if tag >> 3 == 0 {
            return None;
        }
        let field = match tag & 7 {
            0 => Wire::Number(varint(bytes, &mut offset)?),
            1 => Wire::Number(u64::from_le_bytes(
                take(bytes, &mut offset, 8)?.try_into().ok()?,
            )),
            2 => {
                let n = usize::try_from(varint(bytes, &mut offset)?).ok()?;
                Wire::Bytes(take(bytes, &mut offset, n)?)
            }
            5 => {
                take(bytes, &mut offset, 4)?;
                continue;
            }
            _ => return None,
        };
        out.push((tag >> 3, field));
    }
    Some(out)
}
fn ids(field: Wire<'_>) -> Option<Vec<String>> {
    match field {
        Wire::Number(n) => Some(vec![n.to_string()]),
        Wire::Bytes(b) if b.len() % 8 == 0 && b.len() <= 20000 => Some(
            b.as_chunks::<8>()
                .0
                .iter()
                .map(|c| u64::from_le_bytes(*c).to_string())
                .collect(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_proto_preserves_order_and_rejects_truncation() {
        // root field 14, folders field 1, packed fixed64 guild ids 9 then 2.
        let mut folder = vec![10, 16];
        folder.extend(9u64.to_le_bytes());
        folder.extend(2u64.to_le_bytes());
        let mut group = vec![10, folder.len() as u8];
        group.extend(folder);
        let mut root = vec![114, group.len() as u8];
        root.extend(group);
        let settings = serde_json::json!({"settings":STANDARD.encode(&root)});
        assert_eq!(Layout::parse(&settings).unwrap().order(), vec!["9", "2"]);
        assert_eq!(Layout::parse(&serde_json::json!({"settings":{"type":1,"proto":STANDARD.encode(&root)},"partial":true})).unwrap().order(),vec!["9","2"]);
        assert!(
            Layout::parse(
                &serde_json::json!({"settings":{"type":2,"proto":STANDARD.encode(&root)}})
            )
            .is_none()
        );
        root.pop();
        assert!(Layout::parse(&serde_json::json!({"settings":STANDARD.encode(root)})).is_none());
        assert!(fields(&[255; 20]).is_none());
    }
}
