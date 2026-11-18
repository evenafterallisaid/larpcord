use super::*;

impl Larpcord {
    fn profile_tabs(&self, profile: &Profile, cx: &mut Context<Self>) -> AnyElement {
        if profile.user.id == self.store.user.id {
            return div()
                .h(px(1.))
                .w_full()
                .bg(self.color(LINE))
                .into_any_element();
        }
        let friends = profile
            .mutual_friends_count
            .unwrap_or(profile.mutual_friends.len() as u32);
        let servers = profile.mutual_guilds.as_ref().map_or(0, Vec::len);
        column()
            .id("profile-tabs")
            .debug_selector(|| "profile-tabs".into())
            .w_full()
            .h(px(68.))
            .flex_shrink_0()
            .gap_2()
            .child(
                row()
                    .h(px(20.))
                    .flex_shrink_0()
                    .gap_2()
                    .children(
                        profile
                            .mutual_friends
                            .iter()
                            .take(3)
                            .map(|user| self.avatar(user, 20.)),
                    )
                    .when(friends > 0, |s| {
                        s.child(label(format!("{friends} mutual friends")).text_size(px(11.)))
                    }),
            )
            .child(
                row()
                    .w_full()
                    .gap_1()
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .h(px(1.))
                            .bg(self.color(LINE)),
                    )
                    .children(
                        [
                            (0, "About".to_string()),
                            (1, format!("Friends · {friends}")),
                            (2, format!("Servers · {servers}")),
                        ]
                        .into_iter()
                        .map(|(tab, title)| {
                            let selected = self.profile_tab == tab;
                            row()
                                .id(("profile-tab", tab as usize))
                                .debug_selector(move || format!("profile-tab-{tab}"))
                                .relative()
                                .min_w_0()
                                .flex_1()
                                .h(px(36.))
                                .justify_center()
                                .rounded_t_md()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .text_color(self.color(if selected { TEXT } else { MUTED }))
                                .hover(|s| s.bg(rgba(0xffffff09)).text_color(self.color(TEXT)))
                                .child(div().min_w_0().truncate().px_1().child(title))
                                .when(selected, |s| {
                                    s.child(
                                        div()
                                            .absolute()
                                            .bottom_0()
                                            .left_0()
                                            .right_0()
                                            .h(px(2.))
                                            .bg(self.color(LILAC)),
                                    )
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.profile_tab != tab {
                                        this.profile_tab = tab;
                                        cx.notify();
                                    }
                                }))
                        }),
                    ),
            )
            .into_any_element()
    }

    fn profile_mutuals(&self, profile: &Profile, cx: &mut Context<Self>) -> AnyElement {
        if self.profile_tab == 1 {
            if profile.mutual_friends.is_empty() {
                return label("No mutual friends to display.")
                    .text_size(px(12.))
                    .into_any_element();
            }
            let friends = profile
                .mutual_friends
                .iter()
                .take(200)
                .cloned()
                .collect::<Vec<_>>();
            uniform_list(
                "profile-mutual-friends",
                friends.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| {
                            let user = friends[index].clone();
                            row()
                                .id(("mutual-friend", index))
                                .h(px(48.))
                                .gap_3()
                                .px_2()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgba(0xffffff10)))
                                .child(this.presence_avatar(&user, 32.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(13.))
                                        .child(user.name().to_string()),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_profile(user.clone(), window, cx)
                                }))
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h_0()
            .into_any_element()
        } else {
            let servers = profile
                .mutual_guilds
                .as_ref()
                .into_iter()
                .flatten()
                .take(200)
                .filter_map(|g| {
                    self.store
                        .guilds
                        .iter()
                        .find(|known| known.id == g.id)
                        .cloned()
                })
                .collect::<Vec<_>>();
            if servers.is_empty() {
                return label("No mutual servers to display.")
                    .text_size(px(12.))
                    .into_any_element();
            }
            uniform_list(
                "profile-mutual-servers",
                servers.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| {
                            let guild = servers[index].clone();
                            let image = crate::media::guild_icon(&guild.id, guild.icon.as_deref())
                                .and_then(|url| this.media.get(&url));
                            row()
                                .id(("mutual-server", index))
                                .h(px(48.))
                                .gap_3()
                                .px_2()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgba(0xffffff10)))
                                .child(
                                    row()
                                        .size(px(32.))
                                        .rounded_lg()
                                        .bg(this.color(SURFACE))
                                        .justify_center()
                                        .child(
                                            image
                                                .map(|i| {
                                                    img(i)
                                                        .size_full()
                                                        .rounded_lg()
                                                        .object_fit(ObjectFit::Cover)
                                                        .into_any_element()
                                                })
                                                .unwrap_or_else(|| {
                                                    div()
                                                        .child(
                                                            guild
                                                                .name
                                                                .chars()
                                                                .take(2)
                                                                .collect::<String>(),
                                                        )
                                                        .into_any_element()
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(13.))
                                        .child(guild.name.clone()),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let channel =
                                        this.last_channels.get(&guild.id).cloned().or_else(|| {
                                            this.store
                                                .channels
                                                .values()
                                                .filter(|c| {
                                                    c.guild_id.as_ref() == Some(&guild.id)
                                                        && c.kind == 0
                                                })
                                                .min_by_key(|c| c.position)
                                                .map(|c| c.id.clone())
                                        });
                                    if let Some(channel) = channel {
                                        this.profile = None;
                                        this.profile_inspector = false;
                                        this.selected_guild = Some(guild.id.clone());
                                        this.choose_channel(channel, window, cx);
                                    }
                                }))
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h_0()
            .into_any_element()
        }
    }

    pub(super) fn profile_panel(&self, profile: &Profile, cx: &mut Context<Self>) -> AnyElement {
        let inspector = self.profile_inspector;
        let frame = profile.frame_asset.as_ref().filter(|f| f.valid());
        let card_width = if inspector {
            frame.map_or(296., |f| {
                296. * f.inner_width / (f.inner_width + 2. * f.overflow_horizontal)
            })
        } else {
            440.
        };
        let (frame_top, frame_bottom) = frame.map_or((0., 0.), |f| {
            (
                f.overflow_top * card_width / f.inner_width,
                f.overflow_bottom * card_width / f.inner_width,
            )
        });
        let [top, bottom] = profile.theme().unwrap_or([0x232428; 2]);
        let (top, bottom) = if profile.theme().is_some() {
            (
                rgb(top).blend(rgba(0x111216b0)),
                rgb(bottom).blend(rgba(0x111216b0)),
            )
        } else {
            (rgb(top), rgb(bottom))
        };
        let user = &profile.user;
        let message_user = user.clone();
        let presence = self.store.presences.get(&user.id);
        let member = profile.guild_member.as_ref();
        let name = member
            .and_then(|m| m.nick.as_deref())
            .unwrap_or_else(|| user.name());
        let banner = profile
            .banner()
            .and_then(|(hash, server)| {
                crate::media::banner(
                    &user.id,
                    if server {
                        self.profile_guild.as_deref()
                    } else {
                        None
                    },
                    hash,
                )
            })
            .and_then(|url| self.banner_media(&url, true, cx));
        let mut roles = self
            .profile_guild
            .as_ref()
            .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id))
            .map(|g| {
                g.roles
                    .iter()
                    .filter(|r| member.is_some_and(|m| m.roles.contains(&r.id)) && r.id != g.id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        roles.sort_by(|a, b| b.position.cmp(&a.position).then(a.name.cmp(&b.name)));
        let created = user
            .id
            .parse::<u64>()
            .ok()
            .and_then(|id| {
                chrono::DateTime::from_timestamp_millis(((id >> 22) + 1420070400000) as i64)
            })
            .map(|t| t.format("%b %d, %Y").to_string());
        let joined = member
            .and_then(|m| m.joined_at.as_deref())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.format("%b %d, %Y").to_string());
        let mut effect_layers = Vec::new();
        if let Some(effect) = profile.effect_asset.as_ref() {
            // reduced-motion assets remain the fallback; animated layers exist only while open.
            if let Some(url) = effect
                .still()
                .filter(|_| !self.motion() || effect.effects.is_empty())
            {
                if let Some(image) = self.media.get(url) {
                    effect_layers.push(
                        img(image)
                            .absolute()
                            .inset_0()
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .into_any_element(),
                    );
                }
            } else {
                let mut layers = effect.effects.iter().take(8).collect::<Vec<_>>();
                layers.sort_by_key(|layer| layer.z_index);
                for layer in layers {
                    let url = crate::media::motion_url(layer.src.clone(), self.motion());
                    if let Some(image) = self.media.get(&url) {
                        let (x, y) = layer.position.as_ref().map_or((0., 0.), |p| {
                            (p.x.clamp(-440., 440.), p.y.clamp(-600., 600.))
                        });
                        let (x, y) = (x * card_width / 440., y * card_width / 440.);
                        let h = image.size(0).height.0 as f32 / image.size(0).width.0.max(1) as f32
                            * card_width;
                        let started = self.profile_started;
                        let layer = layer.clone();
                        let animated = self.motion();
                        effect_layers.push(
                            canvas(
                                move |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let delays = (0..image.frame_count())
                                        .map(|n| {
                                            let (num, den) = image.delay(n).numer_denom_ms();
                                            (num as f32 / den.max(1) as f32).max(10.)
                                        })
                                        .collect::<Vec<_>>();
                                    let duration = delays.iter().sum::<f32>();
                                    let elapsed = started.elapsed().as_secs_f32() * 1000.;
                                    let phase = if animated && image.frame_count() > 1 {
                                        layer.phase(elapsed, duration)
                                    } else {
                                        Some(0.)
                                    };
                                    if animated
                                        && image.frame_count() > 1
                                        && window.is_window_active()
                                        && (layer.looping
                                            || elapsed < layer.start + layer.duration.max(duration))
                                    {
                                        window.request_animation_frame();
                                    }
                                    if let Some(mut time) = phase {
                                        let mut frame = 0;
                                        while frame + 1 < delays.len() && time >= delays[frame] {
                                            time -= delays[frame];
                                            frame += 1;
                                        }
                                        let _ = window.paint_image(
                                            bounds,
                                            Corners::default(),
                                            image,
                                            frame,
                                            false,
                                        );
                                    }
                                },
                            )
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .w(px(card_width))
                            .h(px(h))
                            .into_any_element(),
                        );
                    }
                }
            }
        }
        let back = self.profile_frame(profile, false, card_width);
        let front = self.profile_frame(profile, true, card_width);
        let effect_title = profile.effect_asset.as_ref().and_then(|e| e.title.clone());
        let id = user.id.clone();
        column()
            .id(if inspector {
                "profile-inspector"
            } else {
                "profile-modal"
            })
            .when(!inspector, |s| {
                s.track_focus(&self.modal_focus)
                    .occlude()
                    .absolute()
                    .inset_0()
                    .top(px(32.))
                    .bg(rgba(0x00000099))
                    .items_center()
                    .justify_center()
            })
            .when(inspector, |s| {
                s.w(px(320.))
                    .debug_selector(|| "profile-side-column".into())
                    .h_full()
                    .flex_shrink_0()
                    .min_h_0()
                    .px_3()
                    .pt(px(12. + frame_top))
                    .pb(px(12. + frame_bottom))
                    .items_center()
                    .bg(self.color(SIDEBAR))
                    .border_l_1()
                    .border_color(self.color(LINE))
            })
            .child(
                column()
                    .relative()
                    .w(px(card_width))
                    .h(px(600.))
                    .when(inspector, |s| s.h_full())
                    .min_h_0()
                    .max_h(relative(if inspector { 1. } else { 0.9 }))
                    .children(back)
                    .child(
                        column()
                            .relative()
                            .w_full()
                            .debug_selector(|| "profile-card".into())
                            .h_full()
                            .min_h_0()
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(top, 0.),
                                linear_color_stop(bottom, 1.),
                            ))
                            .shadow_lg()
                            .child(
                                row()
                                    .relative()
                                    .debug_selector(|| "profile-banner".into())
                                    .h(px(if inspector { 108. } else { 132. }))
                                    .flex_shrink_0()
                                    .rounded_t_xl()
                                    .overflow_hidden()
                                    .px_4()
                                    .items_start()
                                    .pt_3()
                                    .justify_end()
                                    .bg(rgb(profile.accent()))
                                    .when_some(banner, |s, image| {
                                        s.child(
                                            div()
                                                .absolute()
                                                .inset_0()
                                                .overflow_hidden()
                                                .child(image),
                                        )
                                    }),
                            )
                            .child(
                                column()
                                    .id("profile-content")
                                    .flex_1()
                                    .min_h_0()
                                    .p(px(if inspector { 16. } else { 20. }))
                                    .pt(px(if inspector { 52. } else { 64. }))
                                    .gap_4()
                                    .child(
                                        column()
                                            .debug_selector(|| "profile-identity".into())
                                            .flex_shrink_0()
                                            .gap_1()
                                            .child(
                                                row()
                                                    .w_full()
                                                    .h(px(36.))
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .min_w_0()
                                                            .flex_1()
                                                            .truncate()
                                                            .text_size(px(if inspector {
                                                                22.
                                                            } else {
                                                                24.
                                                            }))
                                                            .font_weight(FontWeight::BOLD)
                                                            .child(self.display_name(
                                                                name, user, member, cx,
                                                            )),
                                                    )
                                                    .when(user.id != self.store.user.id, |s| {
                                                        s.child(
                                                            toolbar_button(
                                                                "profile-message",
                                                                "message",
                                                                "Open direct message",
                                                            )
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    this.profile = None;
                                                                    this.profile_inspector = false;
                                                                    this.previous_focus = None;
                                                                    window.focus(&this.root_focus);
                                                                    this.open_friend_dm(
                                                                        message_user.clone(),
                                                                        window,
                                                                        cx,
                                                                    );
                                                                },
                                                            )),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                row()
                                                    .w_full()
                                                    .h(px(18.))
                                                    .gap_2()
                                                    .overflow_hidden()
                                                    .child(
                                                        label(format!("@{}", user.username))
                                                            .truncate(),
                                                    )
                                                    .when_some(
                                                        profile
                                                            .pronouns()
                                                            .filter(|s| !s.is_empty())
                                                            .map(str::to_string),
                                                        |s, pronouns| {
                                                            s.child(label("·"))
                                                                .child(label(pronouns).truncate())
                                                        },
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .h(px(16.))
                                                    .w_full()
                                                    .overflow_hidden()
                                                    .when_some(
                                                        presence.and_then(|p| p.activity.as_ref()),
                                                        |s, activity| {
                                                            s.child(
                                                                label(activity.clone())
                                                                    .text_size(px(12.))
                                                                    .truncate(),
                                                            )
                                                        },
                                                    ),
                                            ),
                                    )
                                    .child(self.profile_badge_strip(profile))
                                    .child(self.profile_tabs(profile, cx))
                                    .child(
                                        column()
                                            .id("profile-tab-body")
                                            .debug_selector(|| "profile-tab-body".into())
                                            .w_full()
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_hidden()
                                            .when(
                                                self.profile_tab != 0
                                                    && user.id != self.store.user.id,
                                                |s| s.child(self.profile_mutuals(profile, cx)),
                                            )
                                            .when(
                                                self.profile_tab == 0
                                                    || user.id == self.store.user.id,
                                                |s| {
                                                    s.child(
                                                        column()
                                                            .id("profile-about-scroll")
                                                            .flex_1()
                                                            .min_h_0()
                                                            .overflow_y_scroll()
                                                            .gap_4()
                                                            .when_some(
                                                                profile
                                                                    .bio()
                                                                    .filter(|s| !s.is_empty())
                                                                    .map(str::to_string),
                                                                |s, bio| {
                                                                    s.child(
                                                            column()
                                                                .p_3()
                                                                .rounded_lg()
                                                                .bg(rgba(0x00000020))
                                                                .gap_2()
                                                                .child(
                                                                    label("ABOUT ME")
                                                                        .text_size(px(10.))
                                                                        .font_weight(
                                                                            FontWeight::BOLD,
                                                                        ),
                                                                )
                                                                .child(self.rich_text(&bio)),
                                                        )
                                                                },
                                                            )
                                                            .when(!roles.is_empty(), |s| {
                                                                s.child(
                                                        column()
                                                            .p_3()
                                                            .rounded_lg()
                                                            .bg(rgba(0x00000020))
                                                            .gap_2()
                                                            .child(
                                                                label("ROLES")
                                                                    .text_size(px(10.))
                                                                    .font_weight(FontWeight::BOLD),
                                                            )
                                                            .child(
                                                                row().gap_1().flex_wrap().children(
                                                                    roles.into_iter().map(|role| {
                                                                        row()
                                                            .gap_1()
                                                            .px_2()
                                                            .py_1()
                                                            .rounded_md()
                                                            .border_1()
                                                            .border_color(self.color(SURFACE))
                                                            .bg(rgb(0x292b30))
                                                            .child(
                                                                div()
                                                                    .size(px(9.))
                                                                    .rounded_full()
                                                                    .bg(rgb(
                                                                        if role.display_color() == 0
                                                                        {
                                                                            MUTED
                                                                        } else {
                                                                            role.display_color()
                                                                        },
                                                                    )),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(11.))
                                                                    .child(role.name.clone()),
                                                            )
                                                                    }),
                                                                ),
                                                            ),
                                                    )
                                                            })
                                                            .when_some(created, |s, date| {
                                                                s.child(
                                                        column()
                                                            .gap_1()
                                                            .child(
                                                                label("DISCORD MEMBER SINCE")
                                                                    .text_size(px(10.))
                                                                    .font_weight(FontWeight::BOLD),
                                                            )
                                                            .child(label(date)),
                                                    )
                                                            })
                                                            .when_some(joined, |s, date| {
                                                                s.child(
                                                        column()
                                                            .gap_1()
                                                            .child(
                                                                label("SERVER MEMBER SINCE")
                                                                    .text_size(px(10.))
                                                                    .font_weight(FontWeight::BOLD),
                                                            )
                                                            .child(label(date)),
                                                    )
                                                            }),
                                                    )
                                                },
                                            ),
                                    )
                                    .when_some(effect_title, |s, title| {
                                        s.child(label(title).text_size(px(10.)))
                                    })
                                    .when_some(self.profile_error.clone(), |s, error| {
                                        s.child(label(error).text_color(rgb(0xf0b7c3)))
                                    })
                                    .when(inspector, |s| {
                                        s.child(
                                            button("expand-profile", "View full profile")
                                                .w_full()
                                                .justify_center()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.profile_inspector = false;
                                                    this.previous_focus = window.focused(cx);
                                                    window.focus(&this.modal_focus);
                                                    cx.notify();
                                                })),
                                        )
                                    }),
                            )
                            .children(effect_layers)
                            .child(
                                div()
                                    .absolute()
                                    .debug_selector(|| "profile-avatar".into())
                                    .top(px(if inspector { 68. } else { 84. }))
                                    .left(px(if inspector { 16. } else { 20. }))
                                    .size(px(if inspector { 80. } else { 96. }))
                                    .rounded_full()
                                    .border_4()
                                    .border_color(top)
                                    .child(self.avatar_with_status(
                                        user,
                                        member,
                                        self.profile_guild.as_deref(),
                                        if inspector { 72. } else { 88. },
                                        true,
                                        presence.map(|p| {
                                            (
                                                p.status.as_str(),
                                                u32::from(top) >> 8,
                                                if inspector { 16. } else { 20. },
                                            )
                                        }),
                                    )),
                            )
                            .when_some(presence.and_then(|p| p.custom.as_ref()), |s, status| {
                                s.child(self.profile_status(status, inspector))
                            })
                            .child(
                                toolbar_button("copy-user-id", "copy", "Copy user ID")
                                    .absolute()
                                    .top(px(12.))
                                    .right(px(48.))
                                    .rounded_full()
                                    .bg(rgba(0x111216aa))
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                                    }),
                            )
                            .child(
                                toolbar_button("close-profile", "close", "Close profile")
                                    .absolute()
                                    .top(px(12.))
                                    .right(px(12.))
                                    .rounded_full()
                                    .bg(rgba(0x111216aa))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.profile = None;
                                        this.profile_inspector = false;
                                        if let Some(focus) = this.previous_focus.take() {
                                            window.focus(&focus);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .children(front),
            )
            .into_any_element()
    }

    fn profile_status(
        &self,
        status: &crate::presence::CustomStatus,
        inspector: bool,
    ) -> AnyElement {
        let emoji = status.emoji_id.as_ref().and_then(|id| {
            self.media.get(&format!(
                "https://cdn.discordapp.com/emojis/{id}.webp?size=32"
            ))
        });
        let surface = self.color(SURFACE);
        let bubble = row()
            .absolute()
            .left(px(if inspector { 108. } else { 130. }))
            .right(px(16.))
            .top(px(if inspector { 72. } else { 94. }))
            .h(px(28.))
            .gap_1()
            .px_2()
            .rounded_lg()
            .bg(surface)
            .border_1()
            .border_color(self.color(LINE))
            .shadow_md()
            .child(
                div()
                    .absolute()
                    .left(px(-5.))
                    .bottom_1()
                    .size(px(6.))
                    .rounded_full()
                    .bg(surface),
            )
            .child(
                div()
                    .absolute()
                    .left(px(-10.))
                    .bottom_0()
                    .size(px(4.))
                    .rounded_full()
                    .bg(surface),
            )
            .when_some(emoji, |s, img_data| {
                s.child(img(img_data).size(px(16.)).flex_shrink_0())
            })
            .when(status.emoji_id.is_none(), |s| {
                s.when_some(status.emoji.as_ref(), |s, emoji| {
                    s.child(label(emoji.clone()))
                })
            })
            .when_some(status.text.as_ref(), |s, text| {
                s.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.))
                        .child(text.clone()),
                )
            });
        fade_in(bubble, "profile-status-enter", self.motion())
    }

    fn profile_badge_strip(&self, profile: &Profile) -> AnyElement {
        let mut seen = Vec::<&crate::model::ProfileBadge>::new();
        let badges = profile
            .badges
            .iter()
            .chain(&profile.guild_badges)
            .take(80)
            .filter(|badge| {
                if seen.iter().any(|earlier| earlier.same_as(badge)) {
                    return false;
                }
                seen.push(badge);
                true
            })
            .take(40)
            .enumerate()
            .map(|(index, badge)| {
                let image = badge
                    .image()
                    .and_then(|url| self.media.get(&url))
                    .or_else(|| {
                        badge
                            .fallback
                            .as_deref()
                            .filter(|url| crate::media::cdn_url(url))
                            .and_then(|url| self.media.get(url))
                    });
                let hint = badge.description.chars().take(160).collect::<String>();
                row()
                    .id(("profile-badge", index))
                    .size(px(22.))
                    .flex_shrink_0()
                    .justify_center()
                    .child(match image {
                        Some(image) => img(image)
                            .size(px(20.))
                            .object_fit(ObjectFit::Contain)
                            .into_any_element(),
                        None => icon("star", 14.)
                            .text_color(self.color(MUTED))
                            .into_any_element(),
                    })
                    .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
            });
        row()
            .id("profile-badges")
            .w_full()
            .h(px(24.))
            .flex_shrink_0()
            .gap_1()
            .overflow_x_scroll()
            .when(profile.user.bot, |s| s.child(badge("APP")))
            .children(badges)
            .into_any_element()
    }
    fn profile_frame(&self, profile: &Profile, front: bool, card_width: f32) -> Vec<AnyElement> {
        let Some(frame) = profile.frame_asset.as_ref().filter(|f| f.valid()) else {
            return Vec::new();
        };
        let scale = card_width / frame.inner_width;
        let width = card_width + 2. * frame.overflow_horizontal * scale;
        frame
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| {
                (layer.order == "front") == front && !(layer.kind == "rail" && layer.responsive)
            })
            .filter_map(|(index, layer)| {
                let image = self.media.get(&frame.url(layer)?)?;
                let h = image.size(0).height.0 as f32 / image.size(0).width.0.max(1) as f32 * width;
                if !h.is_finite() || h < 1. {
                    return None;
                }
                let mut wrapper = div()
                    .absolute()
                    .left(px(-frame.overflow_horizontal * scale))
                    .w(px(width));
                if layer.kind == "staple" {
                    wrapper = wrapper.h(px(h));
                    wrapper = if layer.anchor == "bottom" {
                        wrapper.bottom(px(-frame.overflow_bottom * scale))
                    } else {
                        wrapper.top(px(-frame.overflow_top * scale))
                    };
                    wrapper = wrapper.child(img(image).size_full().object_fit(ObjectFit::Contain));
                } else {
                    wrapper = wrapper.top_0().h_full().overflow_hidden();
                    if layer.kind == "border" {
                        // cards are capped below a screen; twelve tiles covers the supported layer set.
                        for tile in 0..12 {
                            wrapper = wrapper.child(
                                img(image.clone())
                                    .absolute()
                                    .top(px(tile as f32 * h))
                                    .w_full()
                                    .h(px(h)),
                            );
                        }
                    } else if layer.kind == "rail" {
                        let mut image = img(image).absolute().w_full().h(px(h));
                        image = match layer.anchor.as_str() {
                            "bottom" => image.bottom_0(),
                            "center" => image.top(relative(0.5)).mt(px(-h / 2.)),
                            _ => image.top_0(),
                        };
                        wrapper = wrapper.child(image);
                    } else {
                        return None;
                    }
                }
                Some(wrapper.id(("frame-layer", index)).into_any_element())
            })
            .collect()
    }
}
