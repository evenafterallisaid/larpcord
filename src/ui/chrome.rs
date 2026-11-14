use super::*;

pub(super) fn toolbar_button(
    id: &'static str,
    name: &'static str,
    hint: &'static str,
) -> Stateful<Div> {
    row()
        .id(id)
        .size(px(32.))
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .text_color(rgb(MUTED))
        .hover(|s| s.bg(rgb(SURFACE)).text_color(rgb(TEXT)))
        .child(icon(name, 20.))
        .tooltip(move |_, cx| cx.new(|_| Hint(hint.into())).into())
}
impl Larpcord {
    pub(super) fn navigation_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = self
            .selected_guild
            .as_ref()
            .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id))
            .map_or("Direct Messages", |g| g.name.as_str())
            .to_string();
        row()
            .h(px(32.))
            .flex_shrink_0()
            .px_4()
            .gap_1()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                toolbar_button("nav-back", "arrow-left", "Go back")
                    .opacity(if self.navigation.can_back() { 1. } else { 0.3 })
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(id) = this.navigation.back() {
                            this.choose_channel(id, window, cx);
                        }
                    })),
            )
            .child(
                toolbar_button("nav-forward", "arrow-right", "Go forward")
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
            .child(div().flex_1())
            .child(
                row()
                    .gap_2()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(svg().path("home.svg").size(px(18.)).text_color(rgb(MUTED)))
                    .child(title),
            )
            .child(div().flex_1())
            .child(
                toolbar_button("nav-settings", "settings", "Larpcord settings")
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(1, window, cx))),
            )
            .into_any_element()
    }
    pub(super) fn account_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        row()
            .relative()
            .h(px(54.))
            .mx_2()
            .mb_2()
            .rounded_lg()
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .when_some(self.nameplate(&self.store.user, None), |s, image| {
                s.child(
                    img(image)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
            })
            .px_2()
            .gap_2()
            .bg(rgb(PANEL))
            .child(
                div()
                    .id("self-avatar")
                    .cursor_pointer()
                    .child(self.avatar(&self.store.user, 32.))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_profile(this.store.user.clone(), window, cx)
                    })),
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
                                true,
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
                button("settings", "")
                    .child(icon("settings", 18.))
                    .px_2()
                    .bg(rgb(PANEL))
                    .text_color(rgb(MUTED))
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(1, window, cx))),
            )
            .into_any_element()
    }
}
