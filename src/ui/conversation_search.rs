use super::{panels::Panel, *};

const MAX_RESULTS: usize = 20;

#[derive(Default)]
pub(super) struct HistorySearch {
    request: u64,
    offset: u16,
    pending: bool,
    result: Option<Result<crate::network::SearchPage, String>>,
}

impl Larpcord {
    pub(super) fn reset_history_search(&mut self) {
        if self.history_search.pending {
            self.command(Command::CancelSearch);
        }
        self.history_search = HistorySearch {
            request: self.history_search.request.wrapping_add(1),
            ..HistorySearch::default()
        };
    }

    pub(super) fn search_history(&mut self, offset: u16, cx: &mut Context<Self>) {
        if self.preview || self.status != "Connected" || !self.conversation_search_open {
            return;
        }
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        let query = self.conversation_search.read(cx).content.trim().to_string();
        if query.is_empty() {
            return;
        }
        let request = self.history_search.request.wrapping_add(1);
        self.history_search = HistorySearch {
            request,
            offset,
            pending: true,
            result: None,
        };
        let guild = self
            .store
            .channels
            .get(&channel)
            .and_then(|c| c.guild_id.clone());
        if let Err(error) = self.backend.command(Command::SearchHistory {
            channel,
            guild,
            query,
            offset,
            request,
        }) {
            self.history_search.pending = false;
            self.history_search.result = Some(Err(error));
        }
        cx.notify();
    }

    pub(super) fn receive_history_search(
        &mut self,
        request: u64,
        result: Result<crate::network::SearchPage, String>,
    ) {
        if self.conversation_search_open && self.history_search.request == request {
            self.history_search.pending = false;
            self.history_search.result = Some(result);
        }
    }

    pub(super) fn reset_conversation_search(&mut self, cx: &mut Context<Self>) {
        self.cancel_panel_exit(Panel::Search);
        self.conversation_search_open = false;
        self.searched_message = None;
        self.reset_history_search();
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
        self.close_pins(cx);
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
        self.reset_history_search();
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }

    pub(super) fn conversation_search_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        toolbar_button("conversation-search", "search", "Search messages")
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
            return self.panel_transition(
                panel.child(label(
                    "Search loaded messages as you type, or press Enter to search Discord history.",
                )),
                Panel::Search,
                "conversation-search-enter",
                |s, t| s.top(px(55. - 7. * (1. - t))).opacity(t),
            );
        }
        let messages = self
            .selected_channel
            .as_deref()
            .map_or(&[][..], |id| self.store.channel_messages(id));
        let mut results = messages
            .iter()
            .rev()
            .filter(|message| message.content.to_lowercase().contains(&query));
        let loaded = results.by_ref().take(MAX_RESULTS).collect::<Vec<_>>();
        let more = results.next().is_some();
        let history = self.history_search.result.as_ref();
        let pending = self.history_search.pending;
        let matches = match history {
            Some(Ok(page)) => page.matches.iter().collect(),
            _ if !pending => loaded,
            _ => Vec::new(),
        };
        if !self.preview && self.status == "Connected" {
            panel = panel.child(
                button(
                    "search-history",
                    if pending {
                        "Searching…"
                    } else {
                        "Search history"
                    },
                )
                .when(!pending, |s| {
                    s.on_click(cx.listener(|this, _, _, cx| this.search_history(0, cx)))
                }),
            );
        }
        panel = panel.child(
            label(match history {
                Some(Ok(page)) => format!("{} matches in Discord history", page.total),
                Some(Err(error)) => error.clone(),
                None if pending => "Searching Discord history…".to_string(),
                None => match matches.len() {
                    0 => "No matches in loaded messages".to_string(),
                    1 => "1 match".to_string(),
                    count if more => format!("{count}+ matches"),
                    count => format!("{count} matches"),
                },
            })
            .text_size(px(11.)),
        );
        if let Some(Ok(page)) = history {
            use crate::network::{MAX_SEARCH_OFFSET, SEARCH_PAGE_SIZE};
            let offset = self.history_search.offset;
            let next = offset + SEARCH_PAGE_SIZE;
            if offset > 0 || usize::from(next) < page.total && next <= MAX_SEARCH_OFFSET {
                panel = panel.child(
                    row()
                        .gap_2()
                        .when(offset > 0, |s| {
                            s.child(button("back-search-history", "Previous").on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.search_history(offset - SEARCH_PAGE_SIZE, cx)
                                }),
                            ))
                        })
                        .when(
                            usize::from(next) < page.total && next <= MAX_SEARCH_OFFSET,
                            |s| {
                                s.child(button("more-search-history", "Next").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.search_history(next, cx)
                                    }),
                                ))
                            },
                        ),
                );
            }
        }
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
                                this.jump_to_message(&id, window, cx);
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
