use super::{panels::Panel, *};

const MAX_RESULTS: usize = 20;

impl Larpcord {
    pub(super) fn reset_conversation_search(&mut self, cx: &mut Context<Self>) {
        self.cancel_panel_exit(Panel::Search);
        self.conversation_search_open = false;
        self.searched_message = None;
        self.conversation_search
            .update(cx, |input, cx| input.set("", cx));
    }

    pub(super) fn open_conversation_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.home_page
            || self.selected_channel.is_none()
            || self.settings_tab.is_some()
            || self.login_open
            || self.profile_is_modal()
            || self.video.is_some()
            || self.component_menu.is_some()
            || self.message_menu.is_some()
        {
            return;
        }
        self.close_composer_tools();
        self.close_picker();
        self.cancel_panel_exit(Panel::Search);
        self.conversation_search_open = true;
        self.searched_message = None;
        window.focus(&self.conversation_search.focus_handle(cx));
        cx.notify();
    }

    pub(super) fn close_conversation_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_panel_exit(Panel::Search);
        self.conversation_search_open = false;
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }

    pub(super) fn conversation_search_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        toolbar_button("conversation-search", "search", "Search loaded messages")
            .when(self.conversation_search_open, |s| s.bg(self.color(SURFACE)))
            .on_click(cx.listener(|this, _, window, cx| {
                if this.conversation_search_open && !this.panel_closing(Panel::Search) {
                    this.animate_panel_close(Panel::Search, window, cx);
                } else {
                    this.open_conversation_search(window, cx);
                }
            }))
    }

    pub(super) fn conversation_search_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self
            .conversation_search
            .read(cx)
            .content
            .trim()
            .to_lowercase();
        let mut panel = column()
            .id("conversation-search-panel")
            .debug_selector(|| "conversation-search-panel".into())
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
            .rounded_lg()
            .border_1()
            .border_color(self.color(LINE))
            .shadow_lg()
            .child(
                row()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Search this conversation"),
                    )
                    .child(
                        toolbar_button("close-conversation-search", "close", "Close search")
                            .size(px(26.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.animate_panel_close(Panel::Search, window, cx);
                            })),
                    ),
            )
            .child(
                row()
                    .gap_2()
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(self.color(LINE))
                    .bg(self.color(PANEL))
                    .child(icon("search", 15.).text_color(self.color(MUTED)))
                    .child(self.conversation_search.clone()),
            );
        if query.is_empty() {
            return panel
                .child(label(
                    "Searches messages already loaded in this conversation.",
                ))
                .into_any_element();
        }
        let messages = self
            .selected_channel
            .as_deref()
            .map_or(&[][..], |id| self.store.channel_messages(id));
        let mut results = messages
            .iter()
            .rev()
            .filter(|message| message.content.to_lowercase().contains(&query));
        let matches = results.by_ref().take(MAX_RESULTS).collect::<Vec<_>>();
        let more = results.next().is_some();
        panel = panel.child(
            label(match matches.len() {
                0 => "No matches in loaded messages".to_string(),
                1 => "1 match".to_string(),
                count if more => format!("{count}+ matches"),
                count => format!("{count} matches"),
            })
            .text_size(px(11.)),
        );
        panel
            .child(
                column()
                    .id("conversation-results")
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_1()
                    .children(matches.into_iter().map(|message| {
                        let id = message.id.clone();
                        let selector = id.clone();
                        let excerpt = self
                            .message_excerpt(message, &message.content)
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        let date = chrono::DateTime::parse_from_rfc3339(&message.timestamp)
                            .ok()
                            .map(|time| {
                                time.with_timezone(&chrono::Local)
                                    .format("%b %-d · %H:%M")
                                    .to_string()
                            });
                        column()
                            .id(SharedString::from(format!("search-hit-{id}")))
                            .debug_selector(move || format!("search-hit-{selector}"))
                            .gap_1()
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(self.color(SURFACE)))
                            .child(
                                row()
                                    .gap_2()
                                    .child(self.avatar(&message.author, 20.))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .truncate()
                                            .text_size(px(12.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(message.name().to_string()),
                                    )
                                    .when_some(date, |s, date| {
                                        s.child(label(date).text_size(px(11.)).flex_shrink_0())
                                    }),
                            )
                            .child(div().min_w_0().truncate().text_size(px(13.)).child(excerpt))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(index) = this
                                    .rendered_ids
                                    .iter()
                                    .position(|rendered| rendered == &id)
                                {
                                    this.jumping = false;
                                    this.jump_task = None;
                                    this.scrolled_away = index + 1 < this.rendered_ids.len();
                                    this.searched_message = Some(id.clone());
                                    this.messages.scroll_to(ListOffset {
                                        item_ix: index,
                                        offset_in_item: px(0.),
                                    });
                                    this.close_conversation_search(window, cx);
                                }
                            }))
                    })),
            )
            .map(|s| {
                self.panel_transition(s, Panel::Search, "conversation-search-enter", |s, t| {
                    s.top(px(55. - 7. * (1. - t))).opacity(t)
                })
            })
    }
}
