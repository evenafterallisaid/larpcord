use super::*;

impl Larpcord {
    pub(super) fn group_members(&self, channel: &Channel, cx: &mut Context<Self>) -> AnyElement {
        let mut users = channel.recipients.clone();
        if !users.iter().any(|u| u.id == self.store.user.id) {
            users.push(self.store.user.clone());
        }
        users.sort_by_cached_key(|u| u.name().to_lowercase());
        column()
            .w(px(240.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(SIDEBAR))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(
                label(format!("MEMBERS · {}", users.len()))
                    .px_4()
                    .pt_4()
                    .pb_2()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(
                uniform_list(
                    "group-members",
                    users.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let user = users[index].clone();
                                let clicked = user.clone();
                                div()
                                    .px_2()
                                    .child(
                                        row()
                                            .id(SharedString::from(format!(
                                                "group-member-{}",
                                                user.id
                                            )))
                                            .h(px(44.))
                                            .px_2()
                                            .gap_3()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(SURFACE)))
                                            .child(this.presence_avatar(&user, 32.))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .truncate()
                                                    .text_size(px(14.))
                                                    .child(this.display_name(
                                                        user.name(),
                                                        &user,
                                                        None,
                                                        false,
                                                    )),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_profile(clicked.clone(), window, cx)
                                            })),
                                    )
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }
    pub(super) fn subscribe_members(&mut self) {
        let Some(channel) = self
            .selected_channel
            .as_ref()
            .and_then(|id| self.store.channels.get(id))
        else {
            return;
        };
        if let Some(guild) = channel.guild_id.clone() {
            self.command(Command::ViewMembers {
                guild,
                channel: self
                    .members
                    .channel
                    .clone()
                    .unwrap_or_else(|| channel.id.clone()),
                bucket: self.members.bucket,
                enabled: self.members_open,
            });
        }
    }
    pub(super) fn member_rail(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = if self.members.received {
            self.members.total
        } else {
            0
        };
        column()
            .w(px(240.))
            .flex_shrink_0()
            .h_full()
            .bg(rgb(SIDEBAR))
            .border_l_1()
            .border_color(rgb(LINE))
            .when(count == 0, |s| {
                s.child(
                    column()
                        .p_3()
                        .gap_2()
                        .when(self.members.received, |s| {
                            s.child(label("No members to show"))
                        })
                        .when(!self.members.received, |s| {
                            s.children((0..8).map(|_| member_placeholder()))
                        }),
                )
            })
            .child(
                uniform_list(
                    "member-list",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let bucket = range.start / 100 * 100;
                        if this.members.bucket != bucket {
                            this.members.bucket = bucket;
                            this.members.trim();
                            this.subscribe_members();
                        }
                        range
                            .map(|index| this.member_row(index, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .size_full(),
            )
            .into_any_element()
    }
    fn member_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        use crate::members::Row;
        let Some(item) = self.members.rows.get(&index) else {
            return member_placeholder();
        };
        match item {
            Row::Group { id, count } => {
                let name = self
                    .selected_guild
                    .as_ref()
                    .and_then(|guild| self.store.guilds.iter().find(|g| &g.id == guild))
                    .and_then(|g| g.roles.iter().find(|r| &r.id == id))
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| match id.as_str() {
                        "online" => "Online".into(),
                        "offline" => "Offline".into(),
                        _ => "Members".into(),
                    });
                div()
                    .h(px(40.))
                    .w_full()
                    .flex()
                    .items_end()
                    .px_4()
                    .pb_2()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child(format!("{} · {count}", name.to_uppercase()))
                    .into_any_element()
            }
            Row::Member(member, status, activity) => {
                let Some(user) = member.user.as_ref() else {
                    return div().h(px(40.)).into_any_element();
                };
                let user = user.clone();
                let clicked = user.clone();
                let member = *member.clone();
                let selected = member.clone();
                let name = member.nick.as_deref().unwrap_or_else(|| user.name());
                let key = format!("member-{}", user.id);
                let hover_key = key.clone();
                div()
                    .w_full()
                    .px_2()
                    .child(
                        row()
                            .id(SharedString::from(key))
                            .relative()
                            .w_full()
                            .h(px(40.))
                            .px_2()
                            .gap_3()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(SURFACE)))
                            .when_some(self.nameplate(&user, Some(&member)), |s, image| {
                                s.child(
                                    img(image)
                                        .absolute()
                                        .inset_0()
                                        .size_full()
                                        .rounded_md()
                                        .object_fit(ObjectFit::Cover),
                                )
                            })
                            .child(
                                div()
                                    .relative()
                                    .size(px(32.))
                                    .child(self.decorated_avatar(
                                        &user,
                                        Some(&member),
                                        self.selected_guild.as_deref(),
                                        32.,
                                        false,
                                    ))
                                    .child(
                                        div()
                                            .absolute()
                                            .right(px(-1.))
                                            .bottom(px(-1.))
                                            .child(chrome::presence_badge(status, 12., SIDEBAR)),
                                    ),
                            )
                            .child(
                                column()
                                    .relative()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().truncate().text_size(px(14.)).child(
                                        self.display_name(
                                            name,
                                            &user,
                                            Some(&member),
                                            self.hovered_name.as_deref() == Some(&hover_key),
                                        ),
                                    ))
                                    .when_some(activity.clone(), |s, text| {
                                        s.child(label(text).truncate().text_size(px(11.)))
                                    }),
                            )
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                this.hovered_name = hovered.then(|| hover_key.clone());
                                cx.notify();
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_profile(clicked.clone(), window, cx);
                                if let Some(profile) = this.profile.as_mut() {
                                    profile.guild_member = Some(selected.clone());
                                }
                            })),
                    )
                    .into_any_element()
            }
        }
    }
}

fn member_placeholder() -> AnyElement {
    row()
        .h(px(40.))
        .px_2()
        .gap_3()
        .child(div().size(px(32.)).rounded_full().bg(rgb(0x222329)))
        .child(
            column()
                .gap_1()
                .child(div().w(px(92.)).h(px(9.)).rounded_sm().bg(rgb(0x25262c)))
                .child(div().w(px(60.)).h(px(7.)).rounded_sm().bg(rgb(0x1c1d22))),
        )
        .into_any_element()
}
