use super::*;

pub(super) const INSPECTOR_WIDTH: f32 = 292.;
const PREVIEW_WIDTH: f32 = 344.;
const ACTIVITY_TAB: u8 = 3;
const TITLEBAR: Pixels = px(36.);
const POPOVER_GAP: Pixels = px(16.);

fn popup_bounds(
    anchor: Point<Pixels>,
    viewport: Size<Pixels>,
    width: Pixels,
    frame: [Pixels; 3],
) -> Bounds<Pixels> {
    let [side, above, below] = frame;
    let margin = POPOVER_GAP + side;
    let height = px(512.).min((viewport.height - TITLEBAR - px(32.) - above - below).max(px(240.)));
    let right = anchor.x + margin;
    let left = anchor.x - margin - width;
    let x = if right + width + margin <= viewport.width {
        right
    } else {
        left
    }
    .clamp(margin, (viewport.width - margin - width).max(margin));
    let y_min = px(16.) + above;
    let y = (anchor.y - TITLEBAR - px(48.)).clamp(
        y_min,
        (viewport.height - TITLEBAR - height - px(16.) - below).max(y_min),
    );
    Bounds::new(point(x, y), size(width, height))
}

impl Larpcord {
    pub(super) fn demo_profile(&self, mut profile: Profile) -> Profile {
        let (bio, colors) = match profile.user.id.as_str() {
            "moss" => (
                "Making little things for the internet. Usually here with a playlist on.",
                [0x28645b, 0x334067],
            ),
            "clover" => (
                "Taking the scenic route. Say hi if you find a good song.",
                [0x684572, 0x344073],
            ),
            "me" => (
                "A little more room for the conversations that matter.",
                [0x424b71, 0x583c69],
            ),
            _ => return profile,
        };
        profile.user_profile = Some(crate::model::ProfileDetails {
            bio: Some(bio.into()),
            theme_colors: Some(colors),
            accent_color: Some(colors[0]),
            ..Default::default()
        });
        if profile.user.id != self.store.user.id {
            profile.mutual_friends = ["moss", "clover"]
                .into_iter()
                .filter(|id| *id != profile.user.id)
                .filter_map(|id| self.store.users.get(id).cloned())
                .collect();
            profile.mutual_guilds = Some(vec![crate::model::MutualGuild { id: "larp".into() }]);
        }
        profile
    }

    fn profile_tabs(
        &self,
        profile: &Profile,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let friends = profile
            .mutual_friends_count
            .unwrap_or(profile.mutual_friends.len() as u32);
        let servers = profile.mutual_guilds.as_ref().map_or(0, Vec::len);
        let first = if expanded {
            (ACTIVITY_TAB, "Activity")
        } else {
            (0, "About")
        };
        let selected = if expanded && self.profile_tab == 0 {
            ACTIVITY_TAB
        } else {
            self.profile_tab
        };
        let own = profile.user.id == self.store.user.id;
        let tabs = [
            (first.0, first.1, None),
            (1, "Friends", Some(friends as usize)),
            (2, "Servers", Some(servers)),
        ];
        row()
            .id("profile-tabs")
            .debug_selector(|| "profile-tabs".into())
            .w_full()
            .h(px(34.))
            .flex_shrink_0()
            .gap_1()
            .px_1()
            .rounded_lg()
            .bg(self.color(PANEL))
            .children(
                tabs.into_iter()
                    .filter(|(_, _, count)| !own || count.is_none())
                    .map(|(tab, title, count)| {
                        let active = selected == tab;
                        row()
                            .id(("profile-tab", usize::from(tab)))
                            .debug_selector(move || format!("profile-tab-{tab}"))
                            .min_w_0()
                            .flex_1()
                            .h(px(26.))
                            .gap_1()
                            .justify_center()
                            .rounded_md()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .text_color(self.color(if active { TEXT } else { MUTED }))
                            .hover(|s| s.bg(self.color(SURFACE)).text_color(self.color(TEXT)))
                            .when(active, |s| s.bg(self.color(SURFACE)))
                            .child(div().min_w_0().truncate().child(title))
                            .when_some(count, |s, n| {
                                s.child(
                                    div()
                                        .text_size(px(10.))
                                        .flex_shrink_0()
                                        .text_color(self.color(if active { LILAC } else { MUTED }))
                                        .child(n.to_string()),
                                )
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.profile_tab != tab {
                                    this.profile_tab = tab;
                                    cx.notify();
                                }
                            }))
                    }),
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
                                        this.dismiss_profile(window, cx);
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

    pub(super) fn profile_panel(
        &self,
        profile: &Profile,
        viewport: Option<Size<Pixels>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let inspector = self.profile_inspector;
        let expanded = self.profile_expanded && !inspector;
        let popup = viewport
            .zip(self.profile_anchor)
            .filter(|_| !expanded && !inspector);
        let frame = profile.frame_asset.as_ref().filter(|f| f.valid());
        let card_width = if inspector {
            let inner = INSPECTOR_WIDTH - 24.;
            frame.map_or(inner, |f| {
                inner * f.inner_width / (f.inner_width + 2. * f.overflow_horizontal)
            })
        } else if popup.is_some() {
            320.
        } else {
            PREVIEW_WIDTH
        };
        let [frame_side, frame_top, frame_bottom] = frame.map_or([0.; 3], |f| {
            [f.overflow_horizontal, f.overflow_top, f.overflow_bottom]
                .map(|n| n * card_width / f.inner_width)
        });
        let placement = popup.map(|(viewport, anchor)| {
            popup_bounds(
                anchor,
                viewport,
                px(card_width),
                [frame_side, frame_top, frame_bottom].map(|n| px(n.min(68.))),
            )
        });
        // Keep account colors on the surrounding card and use the app palette for readable detail surfaces.
        let base = self.color(SIDEBAR);
        let [top, bottom] = profile.theme().map_or([base, self.color(PANEL)], |colors| {
            colors.map(|color| base.blend(rgba((color << 8) | 0x66)))
        });
        let user = &profile.user;
        let own = user.id == self.store.user.id;
        let presence = self.store.presences.get(&user.id);
        let member = profile.guild_member.as_ref();
        let gutter = if inspector { 12. } else { 16. };
        let cover_height = if inspector {
            100.
        } else if expanded {
            138.
        } else {
            112.
        };
        let avatar_size = if inspector { 80. } else { 96. };
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
        let card_id = user.id.clone();
        let about = self.profile_about(profile, roles, created, joined, expanded, cx);
        let card = column()
            .relative()
            .w_full()
            .debug_selector(|| "profile-card".into())
            .h_full()
            .min_h_0()
            .rounded_xl()
            .border_1()
            .border_color(self.color(LINE))
            .overflow_hidden()
            .bg(linear_gradient(
                165.,
                linear_color_stop(top, 0.),
                linear_color_stop(bottom, 1.),
            ))
            .when(!inspector, |s| s.shadow_lg())
            .child(
                div()
                    .relative()
                    .debug_selector(|| "profile-banner".into())
                    .h(px(cover_height))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .bg(linear_gradient(
                        112.,
                        linear_color_stop(top.blend(rgba((profile.accent() << 8) | 0x80)), 0.),
                        linear_color_stop(top.blend(rgba((profile.accent() << 8) | 0x32)), 1.),
                    ))
                    .when_some(banner, |s, image| {
                        s.child(div().absolute().inset_0().overflow_hidden().child(image))
                    }),
            )
            .child(
                column()
                    .flex_1()
                    .min_h_0()
                    .px(px(gutter))
                    .pt(px(avatar_size / 2. + 10.))
                    .pb(px(gutter))
                    .gap_2()
                    .child(self.profile_identity(profile, cx))
                    .child(self.profile_badge_strip(profile))
                    .when(!expanded && !own, |s| {
                        s.child(self.profile_tabs(profile, false, cx))
                    })
                    .child(
                        column()
                            .id("profile-tab-body")
                            .debug_selector(|| "profile-tab-body".into())
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .rounded_lg()
                            .bg(self.color(PANEL))
                            .overflow_hidden()
                            .child(if expanded || own || self.profile_tab == 0 {
                                about
                            } else {
                                self.profile_mutuals(profile, cx)
                            }),
                    )
                    .when_some(self.profile_error.as_ref(), |s, error| {
                        s.child(
                            label(error.clone())
                                .text_color(rgb(0xf0b7c3))
                                .text_size(px(11.)),
                        )
                    })
                    .child(self.profile_actions(profile, expanded, cx)),
            )
            .children(effect_layers)
            .child(
                div()
                    .absolute()
                    .debug_selector(|| "profile-avatar".into())
                    .top(px(cover_height - avatar_size / 2.))
                    .left(px(gutter))
                    .size(px(avatar_size))
                    .child(div().absolute().inset_0().rounded_full().bg(top))
                    .child(div().absolute().top(px(4.)).left(px(4.)).child(
                        self.avatar_with_status(
                            user,
                            member,
                            self.profile_guild.as_deref(),
                            avatar_size - 8.,
                            true,
                            presence.map(|p| {
                                (
                                    p.status.as_str(),
                                    u32::from(top) >> 8,
                                    if inspector { 16. } else { 20. },
                                )
                            }),
                        ),
                    )),
            )
            .when_some(presence.and_then(|p| p.custom.as_ref()), |s, status| {
                let left = gutter + avatar_size + 12.;
                s.child(self.profile_status(
                    status,
                    point(px(left), px(cover_height + 5.)),
                    px(card_width - left - gutter),
                ))
            })
            .child(
                toolbar_button("copy-user-id", "copy", "Copy user ID")
                    .absolute()
                    .top(px(10.))
                    .right(px(46.))
                    .rounded_full()
                    .bg(rgba(0x111216d9))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(card_id.clone()))
                    }),
            )
            .child(
                toolbar_button("close-profile", "close", "Close profile")
                    .absolute()
                    .top(px(10.))
                    .right(px(10.))
                    .rounded_full()
                    .bg(rgba(0x111216d9))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.animate_panel_close(super::panels::Panel::Profile, window, cx);
                    })),
            );

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
                    .top(TITLEBAR)
                    .when(placement.is_none(), |s| s.bg(rgba(0x00000099)))
                    .items_center()
                    .justify_center()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.animate_panel_close(super::panels::Panel::Profile, window, cx);
                        }),
                    )
            })
            .when(inspector, |s| {
                s.w(px(INSPECTOR_WIDTH))
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
                row()
                    .items_start()
                    .gap(px(if expanded {
                        12. + frame_side.min(68.)
                    } else {
                        12.
                    }))
                    .h(px(if expanded { 608. } else { 512. }))
                    .when(inspector, |s| s.h_full())
                    .max_h(relative(if inspector { 1. } else { 0.9 }))
                    .when_some(placement, |s, area| {
                        s.absolute()
                            .left(area.origin.x)
                            .top(area.origin.y)
                            .h(area.size.height)
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        column()
                            .relative()
                            .w(px(card_width))
                            .h_full()
                            .min_h_0()
                            .children(self.profile_frame(profile, false, card_width))
                            .child(card)
                            .children(self.profile_frame(profile, true, card_width)),
                    )
                    .when(expanded, |s| {
                        s.child(self.expanded_profile_details(profile, cx))
                    }),
            )
            .map(|element| {
                self.panel_transition(
                    element,
                    super::panels::Panel::Profile,
                    if expanded {
                        "profile-expanded-enter"
                    } else if inspector {
                        "profile-inspector-enter"
                    } else {
                        "profile-preview-enter"
                    },
                )
            })
    }

    pub(super) fn dismiss_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.panel_exit = None;
        self.profile = None;
        self.profile_inspector = false;
        self.profile_expanded = false;
        self.profile_anchor = None;
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus);
        }
        cx.notify();
    }

    fn profile_identity(&self, profile: &Profile, cx: &mut Context<Self>) -> AnyElement {
        let user = &profile.user;
        let member = profile.guild_member.as_ref();
        let name = member
            .and_then(|m| m.nick.as_deref())
            .unwrap_or_else(|| user.name());
        column()
            .debug_selector(|| "profile-identity".into())
            .flex_shrink_0()
            .min_w_0()
            .gap_1()
            .child(
                div()
                    .h(px(27.))
                    .min_w_0()
                    .truncate()
                    .text_size(px(22.))
                    .font_weight(FontWeight::BOLD)
                    .child(self.display_name(name, user, member, cx)),
            )
            .child(
                row()
                    .h(px(18.))
                    .gap_2()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(12.))
                            .min_w_0()
                            .truncate()
                            .text_color(self.color(MUTED))
                            .child(format!("@{}", user.username)),
                    )
                    .when_some(
                        profile.pronouns().filter(|text| !text.trim().is_empty()),
                        |s, pronouns| {
                            s.child(
                                div()
                                    .size(px(3.))
                                    .rounded_full()
                                    .flex_shrink_0()
                                    .bg(self.color(MUTED)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(self.color(MUTED))
                                    .child(pronouns.to_string()),
                            )
                        },
                    ),
            )
            .into_any_element()
    }

    fn profile_about(
        &self,
        profile: &Profile,
        roles: Vec<&crate::model::Role>,
        created: Option<String>,
        joined: Option<String>,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bio = profile.bio().filter(|text| !text.trim().is_empty());
        let effect = profile
            .effect_asset
            .as_ref()
            .and_then(|effect| effect.title.as_deref());
        let activity = (!expanded)
            .then(|| {
                self.store
                    .presences
                    .get(&profile.user.id)
                    .and_then(|presence| presence.activity.as_deref())
            })
            .flatten();
        let show_mutuals =
            profile.user.id != self.store.user.id && !profile.mutual_friends.is_empty();
        let empty = bio.is_none()
            && roles.is_empty()
            && created.is_none()
            && joined.is_none()
            && activity.is_none()
            && effect.is_none()
            && !show_mutuals;
        column()
            .id("profile-about-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .text_size(px(13.))
            .line_height(px(19.))
            .when_some(bio, |s, bio| {
                s.child(
                    column()
                        .gap_1()
                        .child(self.profile_heading("About me"))
                        .child(self.rich_text(bio)),
                )
            })
            .when_some(activity, |s, activity| {
                s.child(self.activity_card(activity))
            })
            .when_some(effect, |s, title| {
                s.child(
                    row()
                        .gap_1()
                        .child(icon("sparkles", 13.).text_color(self.color(LILAC)))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(self.color(MUTED))
                                .child(title.to_string()),
                        ),
                )
            })
            .when(!roles.is_empty(), |s| {
                s.child(
                    column().gap_1().child(self.profile_heading("Roles")).child(
                        row()
                            .gap_1()
                            .flex_wrap()
                            .children(roles.into_iter().map(|role| {
                                let role_color = role.display_color();
                                row()
                                    .gap_1()
                                    .max_w_full()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(self.color(SURFACE))
                                    .child(div().size(px(7.)).flex_shrink_0().rounded_full().bg(
                                        if role_color == 0 {
                                            self.color(MUTED)
                                        } else {
                                            rgb(role_color)
                                        },
                                    ))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(11.))
                                            .child(role.name.clone()),
                                    )
                            })),
                    ),
                )
            })
            .when(created.is_some() || joined.is_some(), |s| {
                s.child(
                    row().gap_3().flex_wrap().children(
                        [("Member since", created), ("Server member since", joined)]
                            .into_iter()
                            .filter_map(|(title, date)| {
                                date.map(|date| {
                                    column()
                                        .gap_1()
                                        .flex_grow()
                                        .child(self.profile_heading(title))
                                        .child(
                                            row()
                                                .gap_1()
                                                .child(
                                                    icon("clock", 13.)
                                                        .text_color(self.color(MUTED)),
                                                )
                                                .child(date),
                                        )
                                })
                            }),
                    ),
                )
            })
            .when(show_mutuals, |s| {
                let count = profile
                    .mutual_friends_count
                    .unwrap_or(profile.mutual_friends.len() as u32);
                s.child(
                    row()
                        .id("profile-mutual-summary")
                        .gap_2()
                        .p_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(self.color(SURFACE)))
                        .child(
                            row().gap_1().children(
                                profile
                                    .mutual_friends
                                    .iter()
                                    .take(3)
                                    .map(|user| self.avatar(user, 18.)),
                            ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(self.color(MUTED))
                                .child(format!(
                                    "{count} mutual {}",
                                    if count == 1 { "friend" } else { "friends" }
                                )),
                        )
                        .child(icon("chevron-right", 13.).text_color(self.color(MUTED)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.profile_tab = 1;
                            cx.notify();
                        })),
                )
            })
            .when(empty, |s| {
                s.child(
                    div()
                        .text_size(px(12.))
                        .text_color(self.color(MUTED))
                        .child("No profile details to display."),
                )
            })
            .into_any_element()
    }

    fn profile_heading(&self, title: &'static str) -> Div {
        div()
            .text_size(px(11.))
            .line_height(px(16.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(self.color(MUTED))
            .child(title)
    }

    fn profile_actions(
        &self,
        profile: &Profile,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let own = profile.user.id == self.store.user.id;
        let user = profile.user.clone();
        row()
            .gap_2()
            .flex_shrink_0()
            .child(
                button(
                    "profile-message",
                    if own { "Edit profile" } else { "Message" },
                )
                .debug_selector(|| "profile-message".into())
                .h(px(32.))
                .flex_1()
                .min_w_0()
                .px_2()
                .justify_center()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .bg(self.color(SURFACE))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.dismiss_profile(window, cx);
                    if own {
                        this.open_settings(super::profile_edit::TAB, window, cx);
                    } else {
                        this.open_friend_dm(user.clone(), window, cx);
                    }
                })),
            )
            .when(!expanded, |s| {
                s.child(
                    button("expand-profile", "Full profile")
                        .debug_selector(|| "expand-profile".into())
                        .h(px(32.))
                        .flex_1()
                        .min_w_0()
                        .px_2()
                        .justify_center()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(self.color(PANEL))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.profile_inspector {
                                this.previous_focus = window.focused(cx);
                            }
                            this.profile_inspector = false;
                            this.profile_expanded = true;
                            this.profile_anchor.get_or_insert(window.mouse_position());
                            window.focus(&this.modal_focus);
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn activity_card(&self, activity: &str) -> AnyElement {
        let glyph = if activity.starts_with("Listening") {
            "volume"
        } else {
            "play"
        };
        row()
            .gap_2()
            .p_2()
            .rounded_md()
            .bg(self.color(SIDEBAR))
            .child(
                row()
                    .size(px(30.))
                    .flex_shrink_0()
                    .justify_center()
                    .rounded_md()
                    .bg(self.color(SURFACE))
                    .child(icon(glyph, 14.).text_color(self.color(LILAC))),
            )
            .child(
                column()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(self.profile_heading("Current activity"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_clamp(2)
                            .child(activity.to_string()),
                    ),
            )
            .into_any_element()
    }

    fn expanded_profile_details(&self, profile: &Profile, cx: &mut Context<Self>) -> AnyElement {
        let activity = self
            .store
            .presences
            .get(&profile.user.id)
            .and_then(|p| p.activity.as_deref());
        let own = profile.user.id == self.store.user.id;
        column()
            .debug_selector(|| "expanded-profile-details".into())
            .w(px(348.))
            .h_full()
            .min_h_0()
            .p_4()
            .gap_3()
            .rounded_xl()
            .bg(self.color(SIDEBAR))
            .border_1()
            .border_color(self.color(LINE))
            .child(
                row()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                if own {
                                    "Your profile"
                                } else {
                                    profile.user.name()
                                }
                                .to_string(),
                            ),
                    )
                    .child(
                        button("collapse-profile", "Back to preview")
                            .text_size(px(11.))
                            .flex_shrink_0()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.panel_exit = None;
                                this.profile_expanded = false;
                                if this.profile_tab == ACTIVITY_TAB {
                                    this.profile_tab = 0;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(self.profile_tabs(profile, true, cx))
            .child(
                column()
                    .flex_1()
                    .min_h_0()
                    .p_3()
                    .rounded_lg()
                    .bg(self.color(PANEL))
                    .overflow_hidden()
                    .child(if matches!(self.profile_tab, 1 | 2) && !own {
                        self.profile_mutuals(profile, cx)
                    } else {
                        activity.map_or_else(
                            || {
                                div()
                                    .text_size(px(12.))
                                    .text_color(self.color(MUTED))
                                    .child("No current activity.")
                                    .into_any_element()
                            },
                            |activity| self.activity_card(activity),
                        )
                    }),
            )
            .into_any_element()
    }

    fn profile_status(
        &self,
        status: &crate::presence::CustomStatus,
        origin: Point<Pixels>,
        max_width: Pixels,
    ) -> AnyElement {
        let emoji_only = status.text.is_none();
        let emoji_size = if emoji_only { 32. } else { 16. };
        let emoji = status.emoji_id.as_ref().and_then(|id| {
            self.media.get(&format!(
                "https://cdn.discordapp.com/emojis/{id}.webp?size=64&quality=lossless"
            ))
        });
        let surface = self.color(PANEL);
        let hint = status
            .text
            .clone()
            .or_else(|| status.emoji.clone())
            .unwrap_or_default();
        let bubble = row()
            .id("profile-status-bubble")
            .debug_selector(|| "profile-status-bubble".into())
            .absolute()
            .left(origin.x)
            .top(origin.y)
            .max_w(max_width)
            .when(emoji_only, |s| s.size(px(44.)).justify_center())
            .min_h(px(29.))
            .max_h(px(44.))
            .gap_1()
            .px_2()
            .py_1()
            .rounded_lg()
            .bg(surface)
            .border_1()
            .border_color(self.color(LINE))
            .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
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
                s.child(
                    img(img_data)
                        .size(px(emoji_size))
                        .object_fit(ObjectFit::Contain)
                        .flex_shrink_0(),
                )
            })
            .when(status.emoji_id.is_none(), |s| {
                s.when_some(status.emoji.as_ref(), |s, emoji| {
                    s.child(label(emoji.clone()).text_size(px(emoji_size)))
                })
            })
            .when_some(status.text.as_ref(), |s, text| {
                s.child(
                    div()
                        .min_w_0()
                        .line_clamp(2)
                        .line_height(px(15.))
                        .text_size(px(11.))
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
            .h(px(25.))
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
