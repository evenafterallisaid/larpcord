use super::*;

#[derive(Clone)]
pub(super) struct Reply {
    pub id: String,
    pub channel: String,
    pub name: String,
    pub text: String,
    pub ping: bool,
}
impl Larpcord {
    pub(super) fn reply_to_message(
        &mut self,
        message: &Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reply_to = Some(Reply {
            id: message.id.clone(),
            channel: message.channel_id.clone(),
            name: message.name().to_string(),
            ping: self.settings.reply_mentions,
            text: if super::gif::gif_only(message) {
                "GIF".into()
            } else {
                self.message_excerpt(message, &message.content)
                    .chars()
                    .take(100)
                    .collect()
            },
        });
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }
    pub(super) fn reply_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(reply) = &self.reply_to else {
            return div().into_any_element();
        };
        row()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .rounded_t_lg()
            .bg(rgb(0x242429))
            .child(icon("reply", 16.).text_color(self.color(MUTED)))
            .child(label("Replying to"))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(12.))
                    .child(reply.name.clone()),
            )
            .child(label(reply.text.clone()).truncate().flex_1().min_w_0())
            .child(
                button(
                    "toggle-reply-ping",
                    if reply.ping {
                        "Mention on"
                    } else {
                        "Mention off"
                    },
                )
                .debug_selector(|| "reply-mentions-toggle".into())
                .text_size(px(11.))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(reply) = this.reply_to.as_mut() {
                        reply.ping = !reply.ping;
                        this.settings.reply_mentions = reply.ping;
                        this.persist(cx);
                    }
                    cx.notify();
                })),
            )
            .child(
                button("cancel-reply", "")
                    .p_1()
                    .child(icon("close", 14.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.reply_to = None;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    pub(super) fn react_to(&mut self, message: &Message, text: &str, cx: &mut Context<Self>) {
        let key = (message.channel_id.clone(), message.id.clone());
        if self.reaction_pending.contains(&key) || self.reaction_pending.len() >= 16 {
            return;
        }
        let (emoji, payload) = match crate::content::parts(text).as_slice() {
            [crate::content::Part::Emoji { name, id }] => (
                format!("{name}:{id}"),
                serde_json::json!({"name":name,"id":id}),
            ),
            _ => (text.to_string(), serde_json::json!({"name":text,"id":null})),
        };
        let remove = self
            .store
            .channel_messages(&message.channel_id)
            .iter()
            .find(|m| m.id == message.id)
            .is_some_and(|m| {
                m.reactions.iter().any(|r| {
                    r.me && match payload["id"].as_str() {
                        Some(id) => r.emoji.id.as_deref() == Some(id),
                        None => {
                            r.emoji.id.is_none()
                                && r.emoji.name.as_deref() == payload["name"].as_str()
                        }
                    }
                })
            });
        if self.preview {
            self.store.reaction(if remove {"MESSAGE_REACTION_REMOVE"} else {"MESSAGE_REACTION_ADD"},&serde_json::json!({"channel_id":message.channel_id,"message_id":message.id,"emoji":payload,"user_id":self.store.user.id}));
        } else if self.status != "Connected" {
            self.error = Some("Waiting for the connection to return. No reaction was sent.".into());
        } else {
            match self.backend.command(Command::React {
                channel: message.channel_id.clone(),
                message: message.id.clone(),
                emoji,
                remove,
            }) {
                Ok(()) => {
                    self.reaction_pending.insert(key);
                }
                Err(error) => self.error = Some(error),
            }
        }
        cx.notify();
    }
    pub(super) fn open_reaction_picker(
        &mut self,
        message: Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.message_menu = None;
        self.close_picker();
        self.toggle_picker(crate::expressions::Tab::Emoji, window, cx);
        self.reaction_target = Some(message);
        self.picker_catalog_ready(cx);
        cx.notify();
    }
    fn quick_reactions(&self, message: &Message, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let keys = if self.picker.reaction_recent.is_empty() {
            vec!["❤️".into(), "😂".into(), "👍".into()]
        } else {
            self.picker
                .reaction_recent
                .iter()
                .take(3)
                .cloned()
                .collect()
        };
        keys.into_iter()
            .enumerate()
            .map(|(n, key)| {
                let (text, image) = if crate::expressions::id(&key)
                    && crate::expressions::unicode_alias(&key).is_none()
                {
                    let emoji = self
                        .store
                        .guilds
                        .iter()
                        .flat_map(|g| g.emojis.iter().flatten())
                        .find(|e| e.id == key)
                        .cloned()
                        .unwrap_or(crate::expressions::Emoji {
                            id: key.clone(),
                            name: "emoji".into(),
                            ..Default::default()
                        });
                    let image = self.media.get(&emoji.image(false));
                    (emoji.markup(), image)
                } else {
                    (
                        crate::expressions::unicode_alias(&key)
                            .unwrap_or(&key)
                            .to_string(),
                        None,
                    )
                };
                let message = message.clone();
                row()
                    .id(("quick-reaction", n))
                    .size(px(32.))
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(self.color(PANEL)))
                    .active(|s| s.bg(self.color(LINE)))
                    .child(
                        image
                            .map(|image| img(image).size(px(21.)).into_any_element())
                            .unwrap_or_else(|| {
                                div()
                                    .text_size(px(19.))
                                    .child(
                                        if crate::content::parts(&text).iter().any(|p| {
                                            matches!(p, crate::content::Part::Emoji { .. })
                                        }) {
                                            "◌".to_string()
                                        } else {
                                            text.clone()
                                        },
                                    )
                                    .into_any_element()
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.react_to(&message, &text, cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }
    pub(super) fn message_tools(&self, message: &Message, cx: &mut Context<Self>) -> AnyElement {
        let reply = message.clone();
        let content = message.content.clone();
        let add_reaction = message.clone();
        let menu = message.clone();
        let mention = message.author.id.clone();
        let channel = message.channel_id.clone();
        let quick_mention = self
            .modules
            .feature(crate::modules::NativeFeature::QuickMention)
            && crate::expressions::id(&mention);
        row()
            .id("message-actions")
            .debug_selector(|| "message-actions".into())
            .absolute()
            .right(px(16.))
            .top(self.message_tools_top)
            .h(px(34.))
            .rounded_lg()
            .border_1()
            .border_color(self.color(LINE))
            .bg(self.color(SURFACE))
            .shadow_md()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(self.quick_reactions(message, cx))
            .child(
                toolbar_button("add-message-reaction", "reaction", "Add reaction")
                    .size(px(32.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_reaction_picker(add_reaction.clone(), window, cx)
                    })),
            )
            .child(div().w(px(1.)).h(px(20.)).mx_1().bg(self.color(LINE)))
            .child(
                toolbar_button("reply-message", "reply", "Reply")
                    .size(px(32.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reply_to_message(&reply, window, cx)
                    })),
            )
            .when(quick_mention, |s| {
                s.child(
                    toolbar_button("quick-mention", "at-sign", "Mention author")
                        .debug_selector(|| "quick-mention".into())
                        .size(px(32.))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.attachment_channel().as_deref() == Some(channel.as_str()) {
                                this.composer.update(cx, |input, cx| {
                                    input.insert(&format!("<@{mention}> "), cx)
                                });
                                window.focus(&this.composer.focus_handle(cx));
                            }
                        })),
                )
            })
            .child(
                toolbar_button("copy-message", "copy", "Copy text")
                    .size(px(32.))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(content.clone()))
                    }),
            )
            .child(
                toolbar_button("message-more", "more", "More options")
                    .size(px(32.))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_message_menu(menu.clone(), event.position(), window, cx);
                    })),
            )
            .into_any_element()
    }
    pub(super) fn open_message_menu(
        &mut self,
        message: Message,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_tab.is_some()
            || self.profile_is_modal()
            || self.video.is_some()
            || self.login_open
        {
            return;
        }
        self.close_picker();
        self.previous_focus = window.focused(cx);
        self.message_menu = Some((message, position));
        window.focus(&self.root_focus);
        cx.notify();
    }
    pub(super) fn message_menu_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((message, position)) = &self.message_menu else {
            return div().into_any_element();
        };
        let mut menu = column()
            .id("message-context-menu")
            .debug_selector(|| "message-context-menu".into())
            .w(px(220.))
            .p_1()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(self.color(LINE))
            .bg(self.color(PANEL))
            .shadow_lg()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());
        for (index, name, glyph) in [
            (0, "Reply", "reply"),
            (1, "Copy Text", "copy"),
            (2, "Copy Message Link", "external-link"),
            (3, "Copy Message ID", "hash"),
            (4, "View Profile", "user"),
            (5, "Add Reaction…", "reaction"),
            (6, "Edit Message…", "edit"),
        ] {
            if index == 6 && !self.can_edit_message(message) {
                continue;
            }
            let message = message.clone();
            let guild = self.selected_guild.clone().unwrap_or_else(|| "@me".into());
            menu = menu.child(
                row()
                    .id(("message-action", index as usize))
                    .debug_selector(move || format!("message-action-{index}"))
                    .h(px(34.))
                    .px_2()
                    .gap_3()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x5865f2)).text_color(rgb(0xffffff)))
                    .child(div().flex_1().text_size(px(13.)).child(name))
                    .child(icon(glyph, 17.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.message_menu = None;
                        if let Some(focus) = this.previous_focus.take() {
                            window.focus(&focus);
                        }
                        match index {
                            0 => this.reply_to_message(&message, window, cx),
                            1 => cx.write_to_clipboard(ClipboardItem::new_string(
                                message.content.clone(),
                            )),
                            2 => cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                "https://discord.com/channels/{guild}/{}/{}",
                                message.channel_id, message.id
                            ))),
                            3 => {
                                cx.write_to_clipboard(ClipboardItem::new_string(message.id.clone()))
                            }
                            5 => this.open_reaction_picker(message.clone(), window, cx),
                            6 => this.open_message_edit(&message, window, cx),
                            _ => this.open_profile(message.author.clone(), window, cx),
                        };
                        cx.notify();
                    })),
            );
        }
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.message_menu = None;
                    if let Some(focus) = this.previous_focus.take() {
                        window.focus(&focus);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.message_menu = None;
                    cx.notify();
                }),
            )
            .child(anchored().position(*position).snap_to_window().child(menu))
            .into_any_element()
    }
}
