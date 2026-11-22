use super::*;

impl Larpcord {
    pub(super) fn timeline_divider(&self, title: &str, unread: bool) -> AnyElement {
        let color = if unread {
            rgb(0xed4245)
        } else {
            self.color(LINE)
        };
        row()
            .w_full()
            .h(px(32.))
            .px_4()
            .gap_3()
            .child(div().h(px(1.)).flex_1().bg(color))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .flex_shrink_0()
                    .text_color(if unread {
                        rgb(0xff8086)
                    } else {
                        self.color(TEXT)
                    })
                    .child(title.to_string()),
            )
            .child(div().h(px(1.)).flex_1().bg(color))
            .into_any_element()
    }

    pub(super) fn reply_preview(&self, message: &Message, cx: &mut Context<Self>) -> AnyElement {
        let target = message.message_reference.as_ref().and_then(|reference| {
            let in_channel = reference["channel_id"]
                .as_str()
                .is_none_or(|id| id == message.channel_id);
            in_channel
                .then(|| reference["message_id"].as_str())
                .flatten()
        });
        let cached = target.and_then(|id| {
            self.store
                .channel_messages(&message.channel_id)
                .iter()
                .find(|m| m.id == id)
        });
        let original = cached
            .map(crate::model::ReferencedMessage::from)
            .or_else(|| message.referenced_message.as_deref().cloned());
        row()
            .id("reply-preview")
            .debug_selector({
                let id = message.id.clone();
                move || format!("reply-preview-{id}")
            })
            .relative()
            .when_some(target.map(str::to_owned), |s, target| {
                s.cursor_pointer()
                    .hover(|s| s.text_color(self.color(TEXT)))
                    .tooltip(|_, cx| cx.new(|_| Hint("Jump to original message".into())).into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.jump_to_message(&target, window, cx);
                    }))
            })
            .h(px(20.))
            .gap_1()
            .min_w_0()
            .text_size(px(12.))
            .text_color(self.color(MUTED))
            .child(
                div()
                    .absolute()
                    .left(px(-32.))
                    .top(px(10.))
                    .w(px(27.))
                    .h(px(14.))
                    .border_l_2()
                    .border_t_2()
                    .border_color(rgb(0x4e5058))
                    .rounded_tl(px(6.)),
            )
            .map(|s| match original {
                Some(original) => s
                    .child(self.avatar(&original.author, 16.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("@{}", original.name())),
                    )
                    .child(
                        label(if gif::gif_link(&original.content, &original.embeds) {
                            "GIF".into()
                        } else if !original.content.is_empty() {
                            self.mention_excerpt(
                                super::mentions::MentionScope {
                                    channel: Some(&message.channel_id),
                                    users: &original.mentions,
                                    channels: &original.mention_channels,
                                },
                                &original.content,
                            )
                            .chars()
                            .take(120)
                            .collect()
                        } else {
                            original.excerpt()
                        })
                        .truncate()
                        .flex_1()
                        .min_w_0(),
                    ),
                None => s.child(label("Original message unavailable").italic()),
            })
            .into_any_element()
    }

    pub(super) fn stickers(&self, message: &Message) -> AnyElement {
        row()
            .gap_2()
            .flex_wrap()
            .children(
                message
                    .sticker_items
                    .iter()
                    .take(3)
                    .enumerate()
                    .map(|(index, sticker)| {
                        let url =
                            crate::media::sticker(&sticker.id, sticker.format_type, self.motion());
                        let image = url.as_deref().and_then(|url| self.media.get(url));
                        let name = sticker.name.clone();
                        div()
                            .id(("sticker", index))
                            .size(px(160.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .tooltip(move |_, cx| cx.new(|_| Hint(name.clone())).into())
                            .map(|s| match image {
                                Some(image) => s.child(
                                    img(image.clone())
                                        .id(("sticker-image", image.id.0))
                                        .size_full()
                                        .object_fit(ObjectFit::Contain),
                                ),
                                None => s.child(
                                    column()
                                        .gap_2()
                                        .items_center()
                                        .text_color(self.color(MUTED))
                                        .text_size(px(12.))
                                        .child(sticker.name.clone())
                                        .when(sticker.format_type == 3, |s| {
                                            s.child(label("Lottie sticker").text_size(px(10.)))
                                        }),
                                ),
                            })
                    }),
            )
            .into_any_element()
    }

    pub(super) fn reaction_bar(&self, message: &Message, cx: &mut Context<Self>) -> AnyElement {
        let mut chips = Vec::new();
        for reaction in message.reactions.iter().take(128) {
            let counts = reaction.counts();
            for (burst, count, own) in [
                (false, counts.normal, reaction.me),
                (true, counts.burst, reaction.me_burst),
            ] {
                if count == 0 {
                    continue;
                }
                let tint = if burst {
                    reaction
                        .burst_colors
                        .iter()
                        .find_map(|color| {
                            u32::from_str_radix(color.trim_start_matches('#'), 16)
                                .ok()
                                .filter(|v| *v <= 0xffffff)
                        })
                        .unwrap_or(0xb59bff)
                } else {
                    0x5865f2
                };
                let name = reaction
                    .emoji
                    .name
                    .as_deref()
                    .unwrap_or("emoji")
                    .to_string();
                let tooltip = format!(
                    "{} {name} {}{}",
                    count,
                    if burst {
                        "super reactions"
                    } else {
                        "reactions"
                    },
                    if own { " · includes yours" } else { "" }
                );
                let image = reaction
                    .emoji
                    .url()
                    .as_deref()
                    .and_then(|url| self.media.get(url));
                let replay_key = format!(
                    "{}:{}:{}",
                    message.id,
                    reaction
                        .emoji
                        .id
                        .as_deref()
                        .or(reaction.emoji.name.as_deref())
                        .unwrap_or("emoji"),
                    burst
                );
                let hover_key = replay_key.clone();
                let replay = self.reaction_replays.get(&replay_key).copied().unwrap_or(0);
                let animation_key =
                    SharedString::from(format!("burst-{replay_key}-{count}-{replay}"));
                let animate = burst && self.motion();
                let emoji_key = SharedString::from(format!("emoji-{animation_key}"));
                let clicked_message = message.clone();
                let clicked_emoji = reaction
                    .emoji
                    .id
                    .as_ref()
                    .map(|id| {
                        format!(
                            "<:{}:{id}>",
                            reaction.emoji.name.as_deref().unwrap_or("emoji")
                        )
                    })
                    .unwrap_or_else(|| reaction.emoji.name.clone().unwrap_or_default());
                chips.push(
                    row()
                        .relative()
                        .id(("reaction", chips.len()))
                        .gap_1()
                        .px_2()
                        .h(px(28.))
                        .rounded_lg()
                        .border_1()
                        .border_color(if own || burst {
                            rgb(tint)
                        } else {
                            rgba(0xffffff00)
                        })
                        .bg(if own || burst {
                            rgba((tint << 8) | 0x25)
                        } else {
                            self.color(SURFACE)
                        })
                        .text_color(self.color(if own || burst { 0xd8d3ff } else { MUTED }))
                        .text_size(px(12.))
                        .child(
                            div()
                                .size(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(16.))
                                .map(|s| match image {
                                    Some(image) => s.child(
                                        img(image).size_full().object_fit(ObjectFit::Contain),
                                    ),
                                    None => s.child(if reaction.emoji.id.is_none() {
                                        name
                                    } else {
                                        "◌".into()
                                    }),
                                })
                                .map(|s| {
                                    if animate {
                                        s.with_animation(
                                            emoji_key,
                                            Animation::new(std::time::Duration::from_millis(1100)),
                                            |s, delta| {
                                                let bounce = (delta * std::f32::consts::TAU * 2.)
                                                    .sin()
                                                    * (1. - delta).powi(2)
                                                    * 2.;
                                                s.relative().top(px(-bounce))
                                            },
                                        )
                                        .into_any_element()
                                    } else {
                                        s.into_any_element()
                                    }
                                }),
                        )
                        .child(count.to_string())
                        .when(!burst, |s| {
                            s.cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.react_to(&clicked_message, &clicked_emoji, cx)
                                }))
                        })
                        .tooltip(move |_, cx| cx.new(|_| Hint(tooltip.clone())).into())
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered && burst && this.motion() {
                                if this.reaction_replays.len() >= 128
                                    && !this.reaction_replays.contains_key(&hover_key)
                                    && let Some(old) = this.reaction_replays.keys().next().cloned()
                                {
                                    this.reaction_replays.remove(&old);
                                }
                                let replay =
                                    this.reaction_replays.entry(hover_key.clone()).or_default();
                                *replay = replay.wrapping_add(1);
                                cx.notify();
                            }
                        }))
                        .map(|s| {
                            if animate {
                                s.with_animation(
                                    animation_key,
                                    Animation::new(std::time::Duration::from_millis(1100)),
                                    move |s, delta| s.child(super::burst::overlay(tint, delta)),
                                )
                                .into_any_element()
                            } else {
                                s.into_any_element()
                            }
                        }),
                );
            }
        }
        row()
            .flex_wrap()
            .gap_1()
            .mt_1()
            .children(chips)
            .into_any_element()
    }

    pub(super) fn latest_button(&self, cx: &mut Context<Self>) -> AnyElement {
        row()
            .id("jump-latest")
            .cursor_pointer()
            .gap_2()
            .child(icon("arrow-down", 18.))
            .child("Jump to latest")
            .debug_selector(|| "jump-latest".into())
            .opacity(self.latest_opacity)
            .absolute()
            .bottom(px(6. - (1. - self.latest_opacity) * 5.))
            .right(px(16.))
            .rounded_full()
            .px_4()
            .py_2()
            .border_1()
            .border_color(rgba(0xffffff45))
            .bg(linear_gradient(
                180.,
                linear_color_stop(rgba(0x7275c6c8), 0.),
                linear_color_stop(rgba(0x33354eb8), 1.),
            ))
            .text_color(rgb(0xffffff))
            .shadow_lg()
            .hover(|s| s.bg(rgba(0x666ad9e8)))
            .on_click(cx.listener(|this, _, window, cx| this.jump_latest(window, cx)))
            .into_any_element()
    }
}
