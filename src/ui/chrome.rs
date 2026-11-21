use super::*;

pub(super) fn presence_badge(status: &str, size: f32, background: u32) -> AnyElement {
    let color = match status {
        "online" => 0x43a25a,
        "idle" => 0xf0b232,
        "dnd" => 0xf23f43,
        _ => 0x80848e,
    };
    div()
        .relative()
        .size(px(size))
        .rounded_full()
        .border_2()
        .border_color(rgb(background))
        .bg(rgb(color))
        .when(status == "idle", |s| {
            s.child(
                div()
                    .absolute()
                    .top(px(-1.))
                    .right(px(-1.))
                    .size(px(size * 0.62))
                    .rounded_full()
                    .bg(rgb(background)),
            )
        })
        .when(status == "dnd", |s| {
            s.child(
                div()
                    .absolute()
                    .left(px(size * 0.18))
                    .top(px(size * 0.35))
                    .w(px(size * 0.4))
                    .h(px(size * 0.13))
                    .rounded_full()
                    .bg(rgb(background)),
            )
        })
        .when(matches!(status, "offline" | "invisible"), |s| {
            s.child(
                div()
                    .absolute()
                    .inset(px(2.))
                    .rounded_full()
                    .bg(rgb(background)),
            )
        })
        .into_any_element()
}

pub(super) fn toolbar_button(
    id: &'static str,
    name: &'static str,
    hint: &'static str,
) -> Stateful<Div> {
    toolbar_button_hover(id, name, hint, SURFACE)
}

fn toolbar_button_hover(
    id: &'static str,
    name: &'static str,
    hint: &'static str,
    hover_color: u32,
) -> Stateful<Div> {
    row()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(32.))
        .justify_center()
        .rounded_md()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .cursor_pointer()
        .text_color(rgb(MUTED))
        .hover(move |s| s.bg(rgb(hover_color)).text_color(rgb(TEXT)))
        .active(|s| s.bg(rgb(0x404249)).text_color(rgb(0xffffff)))
        .child(icon(name, 20.))
        .tooltip(move |_, cx| cx.new(|_| Hint(hint.into())).into())
}
impl Larpcord {
    pub(super) fn channel_avatar(&self, channel: &Channel, size: f32) -> AnyElement {
        if channel.kind == 3 {
            let image = crate::media::group_icon(&channel.id, channel.icon.as_deref())
                .and_then(|url| self.media.get(&url));
            row()
                .size(px(size))
                .flex_shrink_0()
                .rounded_full()
                .overflow_hidden()
                .justify_center()
                .bg(rgb(SURFACE))
                .child(
                    image
                        .map(|image| {
                            img(image)
                                .size_full()
                                .rounded_full()
                                .object_fit(ObjectFit::Cover)
                                .into_any_element()
                        })
                        .unwrap_or_else(|| icon("users", size * 0.6).into_any_element()),
                )
                .into_any_element()
        } else {
            channel
                .recipients
                .first()
                .map(|user| self.presence_avatar(user, size))
                .unwrap_or_else(|| icon("user", size).into_any_element())
        }
    }
    pub(super) fn presence_avatar(&self, user: &User, size: f32) -> AnyElement {
        self.avatar_with_status(
            user,
            None,
            None,
            size,
            self.hovered_name.as_deref().is_some_and(|key| {
                key == format!("dm-user-{}", user.id) || key == format!("member-{}", user.id)
            }),
            self.store.presences.get(&user.id).map(|p| {
                (
                    p.status.as_str(),
                    self.settings
                        .theme
                        .tokens(self.settings.custom_theme)
                        .sidebar,
                    if size < 30. { 11. } else { 14. },
                )
            }),
        )
    }

    pub(super) fn titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = self
            .selected_guild
            .as_ref()
            .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id))
            .map_or(
                if self.home_page {
                    "Friends"
                } else {
                    "Direct Messages"
                },
                |g| g.name.as_str(),
            )
            .to_string();
        row()
            .debug_selector(|| "window-chrome".into())
            .relative()
            .h(px(36.))
            .flex_shrink_0()
            .px_2()
            .gap_1()
            .bg(self.color(PANEL))
            .border_b_1()
            .border_color(self.color(LINE))
            .when(cfg!(target_os = "macos"), |s| s.pl(px(86.)))
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                if event.click_count == 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
            })
            .child(
                toolbar_button("nav-back", "arrow-left", "Go back")
                    .size(px(28.))
                    .opacity(if self.navigation.can_back() { 1. } else { 0.3 })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(id) = this.navigation.back() {
                            this.choose_channel(id, window, cx);
                        }
                    })),
            )
            .child(
                toolbar_button("nav-forward", "arrow-right", "Go forward")
                    .size(px(28.))
                    .opacity(if self.navigation.can_forward() {
                        1.
                    } else {
                        0.3
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(id) = this.navigation.forward() {
                            this.choose_channel(id, window, cx);
                        }
                    })),
            )
            .child(
                row().absolute().inset_0().justify_center().child(
                    div()
                        .max_w(relative(0.45))
                        .truncate()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(self.color(MUTED))
                        .child(title),
                ),
            )
            .child(div().flex_1())
            .child(
                toolbar_button("nav-settings", "settings", "Larpcord settings")
                    .size(px(28.))
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(1, window, cx))),
            )
            .when(!cfg!(target_os = "macos"), |s| {
                s.child(self.window_controls(cx))
            })
            .into_any_element()
    }
    pub(super) fn window_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        row()
            .ml_2()
            .children([
                toolbar_button("minimize", "minus", "Minimize")
                    .w(px(40.))
                    .h(px(32.))
                    .rounded_none()
                    .on_click(|_, window, _| window.minimize_window())
                    .into_any_element(),
                toolbar_button("maximize", "maximize", "Maximize")
                    .w(px(40.))
                    .h(px(32.))
                    .rounded_none()
                    .on_click(|_, window, _| window.zoom_window())
                    .into_any_element(),
                toolbar_button_hover("close-window", "close", "Close", 0xda373c)
                    .w(px(40.))
                    .h(px(32.))
                    .rounded_none()
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.settings.keep_connected {
                            window.minimize_window();
                        } else {
                            cx.quit();
                        }
                    }))
                    .into_any_element(),
            ])
            .into_any_element()
    }
    pub(super) fn account_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let server_shortcut = self
            .modules
            .feature(crate::modules::NativeFeature::AccountServerProfile);
        row()
            .relative()
            .h(px(52.))
            .mx_2()
            .mb_2()
            .rounded_xl()
            .border_1()
            .border_color(self.color(LINE))
            .overflow_hidden()
            .child(self.nameplate_art(&self.store.user, None, 52.))
            .px_2()
            .gap_2()
            .bg(self.color(PANEL))
            .child(
                div()
                    .id("self-avatar")
                    .debug_selector(|| "self-avatar".into())
                    .cursor_pointer()
                    .child(self.presence_avatar(&self.store.user, 32.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if server_shortcut {
                            this.open_profile_in(this.store.user.clone(), None, window, cx);
                        } else {
                            this.open_profile(this.store.user.clone(), window, cx);
                        }
                    }))
                    .when(server_shortcut, |s| {
                        s.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, _, window, cx| {
                                if let Some(guild) = this.selected_guild.clone() {
                                    this.open_profile_in(
                                        this.store.user.clone(),
                                        Some(guild),
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        )
                    }),
            )
            .child(
                column()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.display_name(
                                self.store.user.name(),
                                &self.store.user,
                                None,
                                cx,
                            )),
                    )
                    .child(
                        label(if self.preview {
                            "Preview".to_string()
                        } else {
                            self.status.clone()
                        })
                        .text_size(px(10.)),
                    ),
            )
            .child(
                toolbar_button("settings", "settings", "User settings")
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(1, window, cx))),
            )
            .into_any_element()
    }
}
