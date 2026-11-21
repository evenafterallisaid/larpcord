use serde::{Deserialize, Serialize};

// semantic tokens are the stable boundary for future theme and extension authors.
#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Charcoal,
    Onyx,
    Midnight,
    Rose,
    Custom,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(default)]
pub struct Tokens {
    pub background: u32,
    pub panel: u32,
    pub sidebar: u32,
    pub surface: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
}
impl Default for Tokens {
    fn default() -> Self {
        Self {
            background: 0x1e1f26,
            panel: 0x14151a,
            sidebar: 0x191a21,
            surface: 0x2b2d38,
            border: 0x333542,
            text: 0xebedf4,
            muted: 0xa5a8ba,
            accent: 0xb9a8f9,
        }
    }
}
impl Tokens {
    pub fn valid(&self) -> bool {
        [
            self.background,
            self.panel,
            self.sidebar,
            self.surface,
            self.border,
            self.text,
            self.muted,
            self.accent,
        ]
        .into_iter()
        .all(|v| v <= 0xffffff)
    }
    pub fn resolve(&self, color: u32) -> u32 {
        match color {
            0x1b1c21 => self.background,
            0x111216 => self.panel,
            0x15161b => self.sidebar,
            0x282a32 => self.surface,
            0x2d2f38 => self.border,
            0xf2f3f5 | 0xe2e4e9 => self.text,
            0x9a9da7 => self.muted,
            0xc1b5ed => self.accent,
            _ => color,
        }
    }
}
impl Theme {
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::Charcoal, "Charcoal"),
        (Self::Onyx, "Onyx"),
        (Self::Midnight, "Midnight"),
        (Self::Rose, "Rose"),
        (Self::Custom, "Custom"),
    ];
    pub fn tokens(self, custom: Option<Tokens>) -> Tokens {
        let mut t = Tokens::default();
        match self {
            Self::Charcoal => {}
            Self::Onyx => {
                t.background = 0x08090b;
                t.panel = 0x000000;
                t.sidebar = 0x08090b;
                t.surface = 0x1c1d22;
                t.border = 0x27282e;
            }
            Self::Midnight => {
                t.background = 0x151c2b;
                t.panel = 0x0d1220;
                t.sidebar = 0x101727;
                t.surface = 0x25324c;
                t.border = 0x2f3c57;
                t.accent = 0x9bbcff;
            }
            Self::Rose => {
                t.background = 0x261c26;
                t.panel = 0x160f19;
                t.sidebar = 0x1d1520;
                t.surface = 0x382b3d;
                t.border = 0x4b3850;
                t.accent = 0xf1b9db;
            }
            Self::Custom => {
                if let Some(custom) = custom.filter(Tokens::valid) {
                    t = custom;
                }
            }
        }
        t
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_files_accept_partial_tokens_and_reject_invalid_channels() {
        let t: Tokens = serde_json::from_str("{\"background\":0,\"accent\":16711935}").unwrap();
        assert!(t.valid());
        assert_eq!(t.resolve(0x1b1c21), 0);
        let t: Tokens = serde_json::from_str("{\"text\":4294967295}").unwrap();
        assert!(!t.valid());
    }
}
