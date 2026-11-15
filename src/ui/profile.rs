use super::*;

impl Larpcord {
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
            .and_then(|url| self.media.get(&url));
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
                .filter(|_| !self.settings.animations || effect.effects.is_empty())
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
                    let url = crate::media::motion_url(layer.src.clone(), self.settings.animations);
                    if let Some(image) = self.media.get(&url) {
                        let (x, y) = layer.position.as_ref().map_or((0., 0.), |p| {
                            (p.x.clamp(-440., 440.), p.y.clamp(-600., 600.))
                        });
                        let (x, y) = (x * card_width / 440., y * card_width / 440.);
                        let h = image.size(0).height.0 as f32 / image.size(0).width.0.max(1) as f32
                            * card_width;
                        let started = self.profile_started;
                        let layer = layer.clone();
                        let animated = self.settings.animations;
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
                    .bg(rgb(SIDEBAR))
                    .border_l_1()
                    .border_color(rgb(LINE))
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
                                            img(image)
                                                .absolute()
                                                .inset_0()
                                                .size_full()
                                                .object_fit(ObjectFit::Cover),
                                        )
                                    }),
                            )
                            .child(
                                column()
                                    .id("profile-content")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .p(px(if inspector { 16. } else { 20. }))
                                    .pt(px(if inspector { 52. } else { 64. }))
                                    .gap_4()
                                    .child(
                                        column()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(if inspector {
                                                        22.
                                                    } else {
                                                        24.
                                                    }))
                                                    .font_weight(FontWeight::BOLD)
                                                    .child(
                                                        self.display_name(name, user, member, true),
                                                    ),
                                            )
                                            .child(label(format!("@{}", user.username)))
                                            .when_some(
                                                profile
                                                    .pronouns()
                                                    .filter(|s| !s.is_empty())
                                                    .map(str::to_string),
                                                |s, pronouns| s.child(label(pronouns)),
                                            ),
                                    )
                                    .when(user.bot, |s| s.child(badge("APP")))
                                    .when(
                                        profile.mutual_guilds.is_some()
                                            || profile.mutual_friends_count.is_some(),
                                        |s| {
                                            let mut counts = Vec::new();
                                            if let Some(n) = profile.mutual_friends_count {
                                                counts.push(format!("{n} mutual friends"));
                                            }
                                            if let Some(guilds) = &profile.mutual_guilds {
                                                counts.push(format!(
                                                    "{} mutual servers",
                                                    guilds.len()
                                                ));
                                            }
                                            let images = profile
                                                .mutual_guilds
                                                .as_ref()
                                                .into_iter()
                                                .flatten()
                                                .take(2)
                                                .filter_map(|g| {
                                                    self.store
                                                        .guilds
                                                        .iter()
                                                        .find(|known| known.id == g.id)
                                                })
                                                .filter_map(|g| {
                                                    crate::media::guild_icon(
                                                        &g.id,
                                                        g.icon.as_deref(),
                                                    )
                                                })
                                                .filter_map(|url| self.media.get(&url))
                                                .map(|image| {
                                                    img(image)
                                                        .size(px(20.))
                                                        .rounded_full()
                                                        .into_any_element()
                                                })
                                                .collect::<Vec<_>>();
                                            s.child(
                                                row().gap_2().children(images).child(
                                                    label(counts.join(" · "))
                                                        .min_w_0()
                                                        .flex_1()
                                                        .text_size(px(11.)),
                                                ),
                                            )
                                        },
                                    )
                                    .child(div().h(px(1.)).bg(rgb(LINE)))
                                    .when_some(
                                        profile.bio().filter(|s| !s.is_empty()).map(str::to_string),
                                        |s, bio| {
                                            s.child(
                                                column()
                                                    .gap_2()
                                                    .child(
                                                        label("ABOUT ME")
                                                            .text_size(px(10.))
                                                            .font_weight(FontWeight::BOLD),
                                                    )
                                                    .child(self.rich_text(&bio)),
                                            )
                                        },
                                    )
                                    .when(!roles.is_empty(), |s| {
                                        s.child(
                                            column()
                                                .gap_2()
                                                .child(
                                                    label("ROLES")
                                                        .text_size(px(10.))
                                                        .font_weight(FontWeight::BOLD),
                                                )
                                                .child(row().gap_1().flex_wrap().children(
                                                    roles.into_iter().map(|role| {
                                                        row()
                                                            .gap_1()
                                                            .px_2()
                                                            .py_1()
                                                            .rounded_md()
                                                            .border_1()
                                                            .border_color(rgb(SURFACE))
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
                                                )),
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
                                    })
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
                                    .child(self.decorated_avatar(
                                        user,
                                        member,
                                        self.profile_guild.as_deref(),
                                        if inspector { 72. } else { 88. },
                                        true,
                                    )),
                            )
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
            .map(|element| {
                fade_in(
                    element,
                    if inspector {
                        "inspector-enter"
                    } else {
                        "profile-enter"
                    },
                    self.settings.animations,
                )
            })
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
