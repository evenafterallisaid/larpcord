use super::*;

impl Larpcord {
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
                        .p_4()
                        .gap_2()
                        .child(label(if self.members.received {
                            "No members to show"
                        } else {
                            "Loading members…"
                        }))
                        .child(label("Profiles and server roles appear here.").text_size(px(11.))),
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
            return div().h(px(40.)).w_full().into_any_element();
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
                let dot = match status.as_str() {
                    "online" => 0x43a25a,
                    "idle" => 0xf0b232,
                    "dnd" => 0xf23f43,
                    _ => 0x80848e,
                };
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
                                            .size(px(11.))
                                            .rounded_full()
                                            .border_2()
                                            .border_color(rgb(SIDEBAR))
                                            .bg(rgb(dot)),
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
                                        s.child(label(text).truncate().text_size(px(10.)))
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
