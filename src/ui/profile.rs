use super::*;

impl Larpcord {
    pub(super) fn profile_panel(&self, profile: &Profile, cx: &mut Context<Self>) -> AnyElement {
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
        let avatar = member
            .and_then(|m| m.avatar.as_deref())
            .zip(self.profile_guild.as_deref())
            .and_then(|(hash, guild)| crate::media::member_avatar(guild, &user.id, hash))
            .and_then(|url| self.media.get(&url));
        let decoration = member
            .and_then(|m| m.avatar_decoration_data.as_ref())
            .or(user.avatar_decoration_data.as_ref())
            .and_then(|d| {
                d.sku_id
                    .as_deref()
                    .and_then(crate::media::collectible)
                    .or_else(|| d.asset.as_deref().and_then(crate::media::decoration))
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
        let effect = profile
            .effect_asset
            .as_ref()
            .and_then(|e| e.still())
            .and_then(|url| self.media.get(url));
        let effect_layers = if effect.is_none() {
            profile
                .effect_asset
                .as_ref()
                .map(|effect| {
                    effect
                        .effects
                        .iter()
                        .take(4)
                        .filter_map(|layer| {
                            let image = self.media.get(&layer.src)?;
                            let (x, y) = layer.position.as_ref().map_or((0., 0.), |p| {
                                (p.x.clamp(-440., 440.), p.y.clamp(-600., 600.))
                            });
                            Some(
                                img(image)
                                    .absolute()
                                    .left(px(x))
                                    .top(px(y))
                                    .w_full()
                                    .h_full()
                                    .object_fit(ObjectFit::Contain)
                                    .opacity(0.55)
                                    .into_any_element(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let effect_title = profile.effect_asset.as_ref().and_then(|e| e.title.clone());
        let id = user.id.clone();
        column()
            .id("profile-modal")
            .track_focus(&self.modal_focus)
            .occlude()
            .absolute()
            .inset_0()
            .top(px(32.))
            .bg(rgba(0x00000099))
            .items_center()
            .justify_center()
            .child(
                column()
                    .relative()
                    .w(px(440.))
                    .max_h(relative(0.9))
                    .rounded_xl()
                    .overflow_hidden()
                    .bg(rgb(PANEL))
                    .shadow_lg()
                    .children(effect_layers)
                    .when_some(effect, |s, image| {
                        s.child(
                            img(image)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .opacity(0.55),
                        )
                    })
                    .child(
                        row()
                            .relative()
                            .h(px(132.))
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
                            })
                            .child(
                                button("close-profile", "×")
                                    .relative()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.profile = None;
                                        if let Some(focus) = this.previous_focus.take() {
                                            window.focus(&focus);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        column()
                            .id("profile-content")
                            .overflow_y_scroll()
                            .p_5()
                            .gap_3()
                            .child(
                                div()
                                    .relative()
                                    .size(px(106.))
                                    .child(
                                        div()
                                            .absolute()
                                            .left(px(9.))
                                            .top(px(9.))
                                            .size(px(88.))
                                            .rounded_full()
                                            .overflow_hidden()
                                            .map(|s| match avatar {
                                                Some(image) => s.child(
                                                    img(image)
                                                        .size_full()
                                                        .object_fit(ObjectFit::Cover)
                                                        .rounded_full(),
                                                ),
                                                None => s.child(self.avatar(user, 88.)),
                                            }),
                                    )
                                    .when_some(decoration, |s, image| {
                                        s.child(img(image).absolute().inset_0().size_full())
                                    }),
                            )
                            .child(
                                column()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(24.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(name.to_string()),
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
                                                    .child(div().size(px(9.)).rounded_full().bg(
                                                        rgb(if role.display_color() == 0 {
                                                            MUTED
                                                        } else {
                                                            role.display_color()
                                                        }),
                                                    ))
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
                            .child(
                                button("copy-user-id", "Copy user ID")
                                    .text_size(px(11.))
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}
