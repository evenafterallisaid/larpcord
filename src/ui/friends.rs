use super::*;

impl Larpcord {
    pub(super) fn show_friends(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.home_page = true;
        self.read_task = None;
        self.opening_dm = None;
        self.selected_guild = None;
        self.profile = None;
        self.profile_inspector = false;
        self.emoji_open = false;
        self.command(Command::ViewGuild(None));
        window.focus(&self.friend_search.focus_handle(cx));
        cx.notify();
    }

    fn friend_user(&self, id: &str) -> Option<&User> {
        self.store.users.get(id).or_else(|| {
            self.store
                .channels
                .values()
                .flat_map(|c| c.recipients.iter())
                .find(|u| u.id == id)
        })
    }

    pub(super) fn open_friend_dm(
        &mut self,
        user: User,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_tab.is_some() || self.profile_is_modal() || self.opening_dm.is_some() {
            return;
        }
        let existing = self
            .store
            .channels
            .values()
            .find(|c| c.kind == 1 && c.recipients.iter().any(|u| u.id == user.id))
            .map(|c| c.id.clone());
        if let Some(id) = existing {
            self.selected_guild = None;
            self.choose_channel(id, window, cx);
        } else if self.preview {
            let id = format!("sample-dm-{}", user.id);
            self.store.channels.insert(
                id.clone(),
                Channel {
                    id: id.clone(),
                    kind: 1,
                    recipients: vec![user],
                    ..Default::default()
                },
            );
            self.selected_guild = None;
            self.choose_channel(id, window, cx);
        } else {
            let id = user.id.clone();
            match self.backend.command(Command::OpenDm(id.clone())) {
                Ok(()) => self.opening_dm = Some(id),
                Err(error) => self.error = Some(error),
            }
        }
        cx.notify();
    }

    pub(super) fn friends_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        row()
            .id("friends-navigation")
            .h(px(40.))
            .mx_2()
            .mt_2()
            .px_3()
            .gap_3()
            .rounded_md()
            .bg(if self.home_page {
                rgb(SURFACE)
            } else {
                rgba(0x00000000)
            })
            .hover(|s| s.bg(rgb(SURFACE)))
            .text_color(rgb(if self.home_page { TEXT } else { MUTED }))
            .cursor_pointer()
            .child(icon("users", 22.))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Friends"),
            )
            .on_click(cx.listener(|this, _, window, cx| this.show_friends(window, cx)))
            .into_any_element()
    }

    pub(super) fn friends_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.friend_search.read(cx).content.to_lowercase();
        let mut ids = self
            .store
            .relationships
            .iter()
            .filter(|(_, kind)| **kind == 1)
            .map(|(id, _)| id.clone())
            .filter(|id| {
                !self.friends_online
                    || self
                        .store
                        .presences
                        .get(id)
                        .is_some_and(|p| matches!(p.status.as_str(), "online" | "idle" | "dnd"))
            })
            .filter(|id| {
                query.is_empty()
                    || self.friend_user(id).is_some_and(|u| {
                        u.name().to_lowercase().contains(&query)
                            || u.username.to_lowercase().contains(&query)
                    })
            })
            .collect::<Vec<_>>();
        ids.sort_by_cached_key(|id| {
            self.friend_user(id)
                .map(|u| u.name().to_lowercase())
                .unwrap_or_else(|| id.clone())
        });
        let count = ids.len();
        column()
            .debug_selector(|| "friends-shell".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(rgb(BG))
            .rounded_tl_lg()
            .border_l_1()
            .border_t_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .child(
                row()
                    .h(px(48.))
                    .flex_shrink_0()
                    .px_4()
                    .gap_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(icon("users", 22.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Friends"),
                    )
                    .child(div().h(px(24.)).w(px(1.)).bg(rgb(LINE)))
                    .children(
                        [
                            ("friends-online", "Online", true),
                            ("friends-all", "All", false),
                        ]
                        .into_iter()
                        .map(|(id, title, online)| {
                            button(id, title)
                                .h(px(28.))
                                .py_0()
                                .px_3()
                                .bg(if self.friends_online == online {
                                    rgb(SURFACE)
                                } else {
                                    rgba(0x00000000)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.friends_online = online;
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .when_some(self.error.clone(), |s, error| {
                s.child(label(error).px_5().py_2().text_color(rgb(0xf0b7c3)))
            })
            .child(
                row()
                    .mx_5()
                    .mt_4()
                    .px_3()
                    .rounded_lg()
                    .bg(rgb(PANEL))
                    .child(div().flex_1().child(self.friend_search.clone()))
                    .child(icon("search", 18.)),
            )
            .child(
                label(format!(
                    "{} · {count}",
                    if self.friends_online {
                        "ONLINE"
                    } else {
                        "ALL FRIENDS"
                    }
                ))
                .px_5()
                .pt_4()
                .pb_2()
                .font_weight(FontWeight::SEMIBOLD),
            )
            .when(count == 0, |s| {
                s.child(
                    column()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child(icon("users", 48.).text_color(rgb(MUTED)))
                        .child(div().text_size(px(18.)).child(if query.is_empty() {
                            "A little quiet here"
                        } else {
                            "No friends match your search"
                        }))
                        .child(label(if self.friends_online {
                            "Try All to see your offline friends too."
                        } else {
                            "Your Discord friends appear here when connected."
                        })),
                )
            })
            .child(
                uniform_list(
                    "friend-list",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| this.friend_row(&ids[index], cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }

    fn friend_row(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some(user) = self.friend_user(id).cloned() else {
            return div().h(px(64.)).into_any_element();
        };
        let clicked = user.clone();
        let profile = user.clone();
        let activity = self
            .store
            .presences
            .get(id)
            .and_then(|p| p.activity.clone())
            .unwrap_or_else(|| {
                match self.store.presences.get(id).map(|p| p.status.as_str()) {
                    Some("online") => "Online",
                    Some("idle") => "Idle",
                    Some("dnd") => "Do Not Disturb",
                    _ => "Offline",
                }
                .into()
            });
        div()
            .px_5()
            .child(
                row()
                    .id(SharedString::from(format!("friend-{id}")))
                    .h(px(64.))
                    .w_full()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .px_3()
                    .gap_3()
                    .rounded_md()
                    .hover(|s| s.bg(rgb(SURFACE)))
                    .cursor_pointer()
                    .child(self.presence_avatar(&user, 32.))
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.display_name(user.name(), &user, None, false)),
                            )
                            .child(label(activity).text_size(px(12.)).truncate()),
                    )
                    .child(
                        button(
                            "friend-message",
                            if self.opening_dm.as_deref() == Some(id) {
                                "Opening…"
                            } else {
                                "Message"
                            },
                        )
                        .text_size(px(12.))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_friend_dm(clicked.clone(), window, cx);
                            },
                        )),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_profile(profile.clone(), window, cx)
                    })),
            )
            .into_any_element()
    }
}
