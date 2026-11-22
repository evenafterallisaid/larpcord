use super::{panels::Panel, *};
use crate::model::{Pin, PinPage};

const MAX_PINS: usize = 300;

#[derive(Default)]
pub(super) struct Pins {
    pub open: bool,
    request: u64,
    waiting: bool,
    stale: bool,
    entries: Vec<Pin>,
    before: Option<String>,
    error: Option<String>,
}

#[derive(Default)]
pub(super) struct MessageJump {
    request: u64,
    pub pending: Option<(String, String)>,
}

impl Larpcord {
    pub(super) fn close_pins(&mut self, cx: &mut Context<Self>) {
        self.cancel_panel_exit(Panel::Pins);
        if std::mem::take(&mut self.pins.open) {
            self.message_jump.pending = None;
        }
        self.pins.waiting = false;
        self.pins.stale = false;
        self.pins.entries.clear();
        self.pins.before = None;
        self.pins.error = None;
        cx.notify();
    }

    pub(super) fn reset_message_navigation(&mut self, cx: &mut Context<Self>) {
        self.close_pins(cx);
        self.message_jump.pending = None;
    }

    fn open_pins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.conversation_search_open {
            self.close_conversation_search(window, cx);
        }
        self.close_picker();
        self.close_composer_tools();
        self.cancel_panel_exit(Panel::Pins);
        self.pins.open = true;
        self.fetch_pins(None, cx);
    }

    pub(super) fn refresh_open_pins(&mut self, cx: &mut Context<Self>) {
        if self.pins.open && !self.panel_closing(Panel::Pins) {
            if self.pins.waiting {
                self.pins.stale = true;
            } else {
                self.fetch_pins(None, cx);
            }
        }
    }

    fn fetch_pins(&mut self, before: Option<String>, cx: &mut Context<Self>) {
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        self.pins.waiting = false;
        self.pins.request += 1;
        self.pins.error = None;
        if before.is_none() {
            self.pins.entries.clear();
            self.pins.before = None;
        }
        if self.preview {
            self.pins.entries = self
                .store
                .channel_messages(&channel)
                .iter()
                .rev()
                .filter(|message| message.pinned)
                .map(|message| Pin {
                    pinned_at: message.timestamp.clone(),
                    message: message.clone(),
                })
                .collect();
        } else if self.status != "Connected" {
            self.pins.error = Some("Reconnect to view pinned messages.".into());
        } else {
            match self.backend.command(Command::Pins {
                channel,
                request: self.pins.request,
                before,
            }) {
                Ok(()) => self.pins.waiting = true,
                Err(error) => {
                    self.pins.waiting = false;
                    self.pins.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn receive_pins(
        &mut self,
        request: u64,
        result: Result<PinPage, String>,
        cx: &mut Context<Self>,
    ) {
        if !self.pins.open || self.pins.request != request {
            return;
        }
        self.pins.waiting = false;
        if std::mem::take(&mut self.pins.stale) {
            self.refresh_open_pins(cx);
            return;
        }
        match result {
            Ok(page) => {
                let next = page.items.last().and_then(|pin| {
                    (page.has_more
                        && self.pins.before.as_deref() != Some(&pin.pinned_at)
                        && chrono::DateTime::parse_from_rfc3339(&pin.pinned_at).is_ok())
                    .then(|| pin.pinned_at.clone())
                });
                let mut known: HashSet<_> = self
                    .pins
                    .entries
                    .iter()
                    .map(|pin| pin.message.id.clone())
                    .collect();
                self.pins.entries.extend(
                    page.items
                        .into_iter()
                        .filter(|pin| {
                            self.selected_channel.as_ref() == Some(&pin.message.channel_id)
                                && known.insert(pin.message.id.clone())
                        })
                        .take(MAX_PINS.saturating_sub(self.pins.entries.len())),
                );
                self.pins.before = next.filter(|_| self.pins.entries.len() < MAX_PINS);
            }
            Err(error) => self.pins.error = Some(error),
        }
    }

    pub(super) fn reveal_message(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.rendered_ids.iter().position(|message| message == id) else {
            return false;
        };
        self.message_jump.pending = None;
        self.jumping = false;
        self.jump_task = None;
        self.jump_after_latest = None;
        self.read_task = None;
        self.scrolled_away = index + 1 < self.rendered_ids.len()
            || self
                .selected_channel
                .as_ref()
                .and_then(|id| self.pages.get(id))
                .is_some_and(|p| p.newer);
        self.searched_message = Some(id.to_string());
        self.messages.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        self.close_pins(cx);
        if self.conversation_search_open {
            self.close_conversation_search(window, cx);
        }
        cx.notify();
        true
    }

    pub(super) fn jump_to_message(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reveal_message(id, window, cx) {
            return;
        }
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        if self.preview
            || self.status != "Connected"
            || !crate::expressions::id(&channel)
            || !crate::expressions::id(id)
        {
            self.message_jump_error("That message is unavailable here.".into(), cx);
            return;
        }
        let target = id.to_string();
        if self
            .message_jump
            .pending
            .as_ref()
            .is_some_and(|(c, m)| c == &channel && m == &target)
        {
            return;
        }
        self.message_jump.request += 1;
        self.message_jump.pending = Some((channel.clone(), target.clone()));
        if let Err(error) = self.backend.command(Command::MessageContext {
            channel,
            target,
            request: self.message_jump.request,
            started: self.store.revision(),
        }) {
            self.message_jump.pending = None;
            self.message_jump_error(error, cx);
        }
        cx.notify();
    }

    fn message_jump_error(&mut self, error: String, cx: &mut Context<Self>) {
        if self.pins.open {
            self.pins.error = Some(error);
        } else {
            self.error = Some(error);
        }
        cx.notify();
    }

    pub(super) fn receive_message_context(
        &mut self,
        request: u64,
        started: u64,
        result: Result<Vec<Message>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.message_jump.request != request {
            return;
        }
        let Some((channel, target)) = self.message_jump.pending.take() else {
            return;
        };
        if self.home_page || self.selected_channel.as_ref() != Some(&channel) {
            return;
        }
        if self.reveal_message(&target, window, cx) {
            return;
        }
        let messages = match result {
            Ok(messages) => messages,
            Err(error) => return self.message_jump_error(error, cx),
        };
        let messages = messages
            .into_iter()
            .filter(|message| message.channel_id == channel)
            .collect();
        if !self
            .store
            .merge_context(&channel, &target, messages, started)
        {
            return self
                .message_jump_error("The original message is no longer available.".into(), cx);
        }
        self.loading.remove(&channel);
        let latest = self
            .store
            .channels
            .get(&channel)
            .and_then(|c| c.last_message_id.clone());
        let newest = self.store.channel_messages(&channel).last().map(|m| &m.id);
        let page = self.pages.entry(channel.clone()).or_default();
        if let Some(latest) = latest
            && page
                .latest
                .as_ref()
                .is_none_or(|id| snowflake_after(&latest, id))
        {
            page.latest = Some(latest);
        }
        page.no_older = false;
        page.newer = page
            .latest
            .as_ref()
            .is_none_or(|latest| newest.is_some_and(|newest| snowflake_after(latest, newest)));
        self.scrolled_away = true;
        self.refresh_messages();
        self.reveal_message(&target, window, cx);
    }

    pub(super) fn pins_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        toolbar_button("conversation-pins", "pin", "Pinned messages")
            .debug_selector(|| "conversation-pins".into())
            .when(self.pins.open, |s| s.bg(self.color(SURFACE)))
            .on_click(cx.listener(|this, _, window, cx| {
                if this.pins.open && !this.panel_closing(Panel::Pins) {
                    this.animate_panel_close(Panel::Pins, window, cx);
                } else if this.pins.open {
                    this.cancel_panel_exit(Panel::Pins);
                    cx.notify();
                } else {
                    this.open_pins(window, cx);
                }
            }))
    }

    fn pin_row(&self, pin: &Pin, cx: &mut Context<Self>) -> AnyElement {
        let message = &pin.message;
        let id = message.id.clone();
        let selector = id.clone();
        let excerpt = if gif::gif_only(message) {
            "GIF".into()
        } else if !message.content.trim().is_empty() {
            self.message_excerpt(message, &message.content)
        } else if let Some(file) = message.attachments.first() {
            file.filename.clone()
        } else if let Some(sticker) = message.sticker_items.first() {
            sticker.name.clone()
        } else {
            message
                .embeds
                .iter()
                .find_map(|embed| embed.title.clone())
                .unwrap_or_else(|| "Pinned message".into())
        };
        column()
            .id(SharedString::from(format!("pin-{id}")))
            .debug_selector(move || format!("pin-{selector}"))
            .h(px(66.))
            .w_full()
            .gap_1()
            .p_2()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(self.color(SURFACE)))
            .child(
                row()
                    .gap_2()
                    .child(self.avatar(&message.author, 22.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(message.name().to_string()),
                    )
                    .child(div().flex_1())
                    .child(icon("arrow-right", 14.).text_color(self.color(MUTED))),
            )
            .child(label(excerpt).line_clamp(2).line_height(px(15.)))
            .on_click(cx.listener(move |this, _, window, cx| this.jump_to_message(&id, window, cx)))
            .into_any_element()
    }

    pub(super) fn pins_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let panel = column()
            .id("pinned-messages")
            .debug_selector(|| "pinned-messages".into())
            .absolute()
            .top(px(55.))
            .right(px(12.))
            .w(relative(0.94))
            .max_w(px(360.))
            .max_h(relative(0.76))
            .occlude()
            .p_3()
            .gap_2()
            .bg(self.color(SIDEBAR))
            .border_1()
            .border_color(self.color(LINE))
            .rounded_lg()
            .shadow_lg()
            .child(
                row()
                    .gap_2()
                    .child(icon("pin", 16.))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Pinned messages"),
                    )
                    .child(
                        toolbar_button("close-pins", "close", "Close pinned messages")
                            .size(px(26.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.animate_panel_close(Panel::Pins, window, cx)
                            })),
                    ),
            )
            .when_some(self.pins.error.clone(), |s, error| {
                s.child(label(error).text_color(rgb(0xf0b7c3)))
            })
            .when(
                self.pins.entries.is_empty() && !self.pins.waiting && self.pins.error.is_none(),
                |s| s.child(label("No pinned messages in this conversation.")),
            )
            .when(!self.pins.entries.is_empty(), |s| {
                let count = self.pins.entries.len();
                s.child(
                    uniform_list(
                        "pin-list",
                        count,
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .filter_map(|index| this.pins.entries.get(index))
                                .map(|pin| this.pin_row(pin, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .w_full()
                    .min_h_0()
                    .h(px((count.min(5) * 66) as f32)),
                )
            })
            .when(self.message_jump.pending.is_some(), |s| {
                s.child(label("Opening message…"))
            })
            .when(self.pins.waiting, |s| s.child(label("Loading pins…")))
            .when_some(
                self.pins.before.clone().filter(|_| !self.pins.waiting),
                |s, cursor| {
                    s.child(
                        button("load-more-pins", "Load older pins").on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.fetch_pins(Some(cursor.clone()), cx);
                            },
                        )),
                    )
                },
            )
            .when(
                self.pins.error.is_some() && self.pins.entries.is_empty() && !self.pins.waiting,
                |s| {
                    s.child(button("retry-pins", "Reload pins").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.fetch_pins(None, cx);
                        },
                    )))
                },
            );
        self.panel_transition(panel, Panel::Pins, "pins-enter", |s, t| {
            s.top(px(55. - 7. * (1. - t))).opacity(t)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Larpcord, Message};
    use gpui::{Modifiers, TestAppContext, point, px};

    #[gpui::test]
    fn opening_a_pin_reveals_the_message_and_keeps_the_draft(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        let viewport = cx.update(|window, _| window.viewport_size());
        view.update(cx, |v, cx| {
            v.settings.animations = false;
            v.composer.update(cx, |input, cx| input.set("draft", cx));
        });
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
        let button = cx.debug_bounds("conversation-pins").unwrap();
        cx.simulate_click(button.center(), Modifiers::default());
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
        let card = cx.debug_bounds("pin-5").unwrap();
        cx.simulate_click(card.center(), Modifiers::default());
        view.update(cx, |v, cx| {
            assert!(!v.pins.open);
            assert_eq!(v.searched_message.as_deref(), Some("5"));
            assert_eq!(v.composer.read(cx).content.as_ref(), "draft");
        });
    }

    #[gpui::test]
    fn old_message_context_replaces_the_window_and_stale_results_are_ignored(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.store.insert(Message {
                    id: "300".into(),
                    channel_id: "campfire".into(),
                    ..Default::default()
                });
                v.refresh_messages();
                v.message_jump.request = 9;
                v.message_jump.pending = Some(("campfire".into(), "44".into()));
                let started = v.store.revision();
                let fetch = || {
                    [43, 44, 45]
                        .map(|id| Message {
                            id: id.to_string(),
                            channel_id: "campfire".into(),
                            ..Default::default()
                        })
                        .into()
                };
                v.receive_message_context(8, started, Ok(fetch()), window, cx);
                assert!(v.rendered_ids.iter().any(|id| id == "300"));
                v.receive_message_context(9, started, Ok(fetch()), window, cx);
                assert_eq!(v.rendered_ids, ["43", "44", "45"]);
                assert_eq!(v.searched_message.as_deref(), Some("44"));
                assert!(v.pages["campfire"].newer);
            });
        });
    }
}
