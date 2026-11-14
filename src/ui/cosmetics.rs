use super::*;
use crate::{cosmetics::NameStyle, model::Member};
use unicode_segmentation::UnicodeSegmentation;

impl Larpcord {
    pub(super) fn display_name(
        &self,
        name: &str,
        user: &User,
        member: Option<&Member>,
        full: bool,
    ) -> AnyElement {
        let style = member
            .and_then(|m| m.display_name_styles.as_ref())
            .or(user.display_name_styles.as_ref());
        let default = NameStyle::default();
        let style = style.unwrap_or(&default);
        let role_color = member
            .and_then(|m| {
                self.selected_guild
                    .as_ref()
                    .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id))
                    .and_then(|g| {
                        g.roles
                            .iter()
                            .filter(|r| m.roles.contains(&r.id) && r.display_color() != 0)
                            .max_by_key(|r| r.position)
                    })
            })
            .map_or(TEXT, |r| r.display_color());
        let count = name.graphemes(true).count().max(2) - 1;
        let highlights = if full {
            name.grapheme_indices(true)
                .enumerate()
                .map(|(n, (offset, g))| {
                    (
                        offset..offset + g.len(),
                        HighlightStyle {
                            color: Some(
                                rgb(style.color_at(n as f32 / count as f32, role_color)).into(),
                            ),
                            ..Default::default()
                        },
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        div()
            .text_color(rgb(role_color))
            .font_family(style.font())
            .font_weight(FontWeight::SEMIBOLD)
            .child(StyledText::new(name.to_string()).with_highlights(highlights))
            .into_any_element()
    }

    pub(super) fn decorated_avatar(
        &self,
        user: &User,
        member: Option<&Member>,
        guild: Option<&str>,
        size: f32,
        motion: bool,
    ) -> AnyElement {
        let url = member
            .and_then(|m| m.avatar.as_deref())
            .zip(guild)
            .and_then(|(hash, guild)| crate::media::member_avatar(guild, &user.id, hash))
            .or_else(|| crate::media::avatar(&user.id, user.avatar.as_deref()));
        let avatar = url.and_then(|url| self.media.get(&url));
        let decoration = member
            .and_then(|m| m.avatar_decoration_data.as_ref())
            .or(user.avatar_decoration_data.as_ref())
            .and_then(|d| {
                d.asset
                    .as_deref()
                    .and_then(crate::media::decoration)
                    .or_else(|| d.sku_id.as_deref().and_then(crate::media::collectible))
            })
            .map(|url| crate::media::motion_url(url, motion && self.settings.animations))
            .and_then(|url| self.media.get(&url));
        div()
            .relative()
            .size(px(size))
            .flex_shrink_0()
            .child(
                row()
                    .size_full()
                    .justify_center()
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgb(0x4e5058))
                    .map(|s| match avatar {
                        Some(image) => s.child(
                            img(image)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .rounded_full(),
                        ),
                        None => s.child(user.name().chars().next().unwrap_or('?').to_string()),
                    }),
            )
            .when_some(decoration, |s, image| {
                s.child(
                    img(image.clone())
                        .id(("avatar-decoration", image.id.0))
                        .absolute()
                        .left(px(-size * 0.1))
                        .top(px(-size * 0.1))
                        .size(px(size * 1.2))
                        .object_fit(ObjectFit::Contain),
                )
            })
            .into_any_element()
    }

    pub(super) fn nameplate(
        &self,
        user: &User,
        member: Option<&Member>,
    ) -> Option<std::sync::Arc<RenderImage>> {
        member
            .and_then(|m| m.collectibles.as_ref())
            .or(user.collectibles.as_ref())
            .and_then(|c| c.nameplate.as_ref())
            .and_then(|n| n.url())
            .and_then(|url| self.media.get(&url))
    }

    pub(super) fn typing_indicator(&self) -> AnyElement {
        let mut element = row().min_h(px(18.)).gap_2();
        if self.typing.len() == 1
            && let Some(style) = self
                .typing
                .keys()
                .next()
                .and_then(|id| self.typing_styles.get(id))
        {
            let mut emojis = row().id("typing-emoji").gap_1();
            for emoji in style.emojis.iter().take(3) {
                if let Some(id) = emoji.custom_emoji_id.as_deref().filter(|id| {
                    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
                }) {
                    let url = format!("https://cdn.discordapp.com/emojis/{id}.png?size=32");
                    if let Some(image) = self.media.get(&url) {
                        emojis = emojis.child(img(image).size(px(16.)));
                    }
                } else if let Some(text) = emoji.unicode_emoji.as_ref() {
                    emojis = emojis.child(
                        div()
                            .text_size(px(15.))
                            .child(text.chars().take(16).collect::<String>()),
                    );
                }
            }
            let animation = style.animation;
            element = element.child(
                if self.settings.animations && (1..=3).contains(&animation) {
                    emojis
                        .with_animation(
                            "typing-motion",
                            Animation::new(std::time::Duration::from_millis(1400)).repeat(),
                            move |s, delta| {
                                let wave = (delta * std::f32::consts::TAU).sin();
                                match animation {
                                    2 => s
                                        .border_1()
                                        .border_color(rgba(0xc1b5ed77))
                                        .rounded_full()
                                        .px_1()
                                        .opacity(0.7 + 0.3 * wave.abs()),
                                    3 => s.top(px(wave * 2.)).relative(),
                                    _ => s.opacity(0.55 + 0.45 * wave.abs()),
                                }
                            },
                        )
                        .into_any_element()
                } else {
                    emojis.into_any_element()
                },
            );
        }
        element
            .child(label(self.typing_label()).text_size(px(11.)))
            .into_any_element()
    }
}
