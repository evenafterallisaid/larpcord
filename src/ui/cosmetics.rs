use super::*;
use crate::{cosmetics::NameStyle, model::Member};
use unicode_segmentation::UnicodeSegmentation;

impl Larpcord {
    pub(super) fn display_name(
        &self,
        name: &str,
        user: &User,
        member: Option<&Member>,
        _full: bool,
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
        let highlights = if !style.colors.is_empty() {
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
        let effect = style.effect_id.unwrap_or(1);
        let tint = style.color_at(0., role_color);
        let copies: Vec<(f32, f32, u32, f32)> = match effect {
            3 | 6 => vec![
                (-1.2, 0., tint, 0.22),
                (1.2, 0., tint, 0.22),
                (0., -1.2, tint, 0.22),
                (0., 1.2, tint, 0.22),
            ],
            4 => vec![
                (-1., 0., 0x08090b, 0.9),
                (1., 0., 0x08090b, 0.9),
                (0., -1., 0x08090b, 0.9),
                (0., 1., 0x08090b, 0.9),
            ],
            5 => vec![(1.3, 1.5, 0x08090b, 0.9)],
            8 => vec![(0., 1.2, 0x08090b, 0.6)],
            _ => Vec::new(),
        };
        div()
            .relative()
            .text_color(rgb(role_color))
            .font_family(style.font())
            .font_weight(FontWeight::SEMIBOLD)
            .children(copies.into_iter().map(|(x, y, color, opacity)| {
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .opacity(opacity)
                    .text_color(rgb(color))
                    .child(name.to_string())
            }))
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
            .filter(|_| size >= 30.)
            .and_then(|d| {
                d.asset
                    .as_deref()
                    .and_then(crate::media::decoration)
                    .or_else(|| d.sku_id.as_deref().and_then(crate::media::collectible))
            })
            .and_then(|url| {
                let moving = crate::media::motion_url(url.clone(), motion && self.motion());
                self.media.get(&moving).or_else(|| self.media.get(&url))
            });
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

    pub(super) fn nameplate_art(
        &self,
        user: &User,
        member: Option<&Member>,
        height: f32,
    ) -> AnyElement {
        let palette = member
            .and_then(|m| m.collectibles.as_ref())
            .or(user.collectibles.as_ref())
            .and_then(|c| c.nameplate.as_ref())
            .and_then(|p| p.palette.as_deref());
        let tint = match palette {
            Some("crimson") => 0x900007,
            Some("berry") => 0x893a99,
            Some("sky") => 0x0080b7,
            Some("teal") => 0x086460,
            Some("forest") => 0x2d5401,
            Some("bubble_gum") => 0xdc3e97,
            Some("violet") => 0x730bc8,
            Some("cobalt") => 0x0131c2,
            Some("clover") => 0x047b20,
            Some("lemon") => 0xf6cd12,
            Some("white") => 0xffffff,
            _ => 0,
        };
        div()
            .absolute()
            .inset_0()
            .rounded_md()
            .overflow_hidden()
            .bg(linear_gradient(
                90.,
                linear_color_stop(rgba(tint << 8), 0.),
                linear_color_stop(rgba((tint << 8) | 0x4d), 1.),
            ))
            .when_some(self.nameplate(user, member), |s, image| {
                let ratio = image.size(0).width.0 as f32 / image.size(0).height.0.max(1) as f32;
                s.child(
                    img(image.clone())
                        .id(("nameplate-art", image.id.0))
                        .absolute()
                        .right_0()
                        .top_0()
                        .h(px(height))
                        .w(px(height * ratio))
                        .max_w_full()
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
        let plate = member
            .and_then(|m| m.collectibles.as_ref())
            .or(user.collectibles.as_ref())
            .and_then(|c| c.nameplate.as_ref())?;
        let still = plate.url()?;
        let motion = self.motion()
            && self.hovered_name.as_deref().is_some_and(|key| {
                key == format!("member-{}", user.id) || key == format!("dm-user-{}", user.id)
            });
        if motion && let Some(url) = plate.sku_id.as_deref().and_then(crate::media::collectible) {
            let url = crate::media::motion_url(url, true);
            return self.media.get(&url).or_else(|| self.media.get(&still));
        }
        self.media.get(&still)
    }

    pub(super) fn typing_indicator(&self) -> AnyElement {
        let mut element = row().min_h(px(18.)).gap_2();
        let custom = self.typing.len() == 1
            && self
                .typing
                .keys()
                .next()
                .and_then(|id| self.typing_styles.get(id))
                .is_some_and(|s| !s.emojis.is_empty());
        if !custom {
            element = element.child(row().gap(px(3.)).children((0..3).map(|index| {
                let dot = div()
                    .relative()
                    .size(px(4.))
                    .rounded_full()
                    .bg(self.color(MUTED));
                if self.motion() {
                    dot.with_animation(
                        ("typing-dot", index as usize),
                        Animation::new(std::time::Duration::from_millis(1200)).repeat(),
                        move |s, t| {
                            let wave = ((t * std::f32::consts::TAU) - (index as f32 * 0.7))
                                .sin()
                                .max(0.);
                            s.top(px(-2. * wave)).opacity(0.4 + 0.6 * wave)
                        },
                    )
                    .into_any_element()
                } else {
                    dot.into_any_element()
                }
            })));
        }
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
            element = element.child(if self.motion() && (1..=3).contains(&animation) {
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
            });
        }
        element
            .child(label(self.typing_label()).text_size(px(11.)))
            .into_any_element()
    }
}
