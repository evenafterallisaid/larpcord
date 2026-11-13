use super::*;

impl Larpcord {
    pub(super) fn video_card(
        &self,
        url: &str,
        title: &str,
        poster: Option<&crate::model::EmbedMedia>,
        dimensions: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (width, height) = media_size(
            dimensions.or_else(|| poster.and_then(|p| p.width.zip(p.height))),
            400.,
            300.,
        );
        let image = poster
            .and_then(|poster| poster.proxy_url.as_deref().or(poster.url.as_deref()))
            .filter(|url| crate::media::cdn_url(url))
            .and_then(|url| self.media.get(url));
        let url = url.to_string();
        let title = title.to_string();
        div()
            .id(SharedString::from(format!("video-{url}")))
            .debug_selector(|| "video-tile".into())
            .relative()
            .w(px(width))
            .h(px(height))
            .max_w_full()
            .rounded_lg()
            .overflow_hidden()
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x09090b))
            .cursor_pointer()
            .when_some(image, |s, image| {
                s.child(
                    img(image)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .rounded_lg(),
                )
            })
            .child(
                column()
                    .absolute()
                    .inset_0()
                    .justify_center()
                    .items_center()
                    .child(
                        row()
                            .size(px(48.))
                            .justify_center()
                            .rounded_full()
                            .bg(rgba(0x000000b8))
                            .border_1()
                            .border_color(rgba(0xffffff30))
                            .child(icon("play", 22.).text_color(rgb(TEXT)))
                            .hover(|s| s.bg(rgb(0x5865f2))),
                    ),
            )
            .when(!title.is_empty(), |s| {
                s.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .px_3()
                        .py_2()
                        .bg(rgba(0x0000008c))
                        .child(label(title.clone()).text_color(rgb(TEXT)).truncate()),
                )
            })
            .tooltip(|_, cx| cx.new(|_| Hint("Play video".into())).into())
            .on_click(
                cx.listener(move |this, _, window, cx| this.open_video(&url, &title, window, cx)),
            )
            .into_any_element()
    }

    fn open_video(&mut self, url: &str, title: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_tab.is_some()
            || self.login_open
            || self.profile.is_some()
            || self.component_menu.is_some()
            || self.video.is_some()
        {
            return;
        }
        if self.preview_only && url == "https://cdn.discordapp.com/larpcord-preview/sample.mp4" {
            self.sample_video(window, cx);
            return;
        }
        let Some(source) = crate::video::source(url) else {
            return;
        };
        self.start_video(source, title, Some(url.to_string()), None, window, cx);
    }

    fn start_video(
        &mut self,
        source: alhazen_gpui::Source,
        title: &str,
        url: Option<String>,
        temporary: Option<tempfile::NamedTempFile>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let playback = cx.new(|cx| crate::video::Playback::new(source, temporary, cx));
        self.video = Some(playback);
        self.video_title = title.to_string();
        self.video_url = url;
        self.previous_focus = window.focused(cx);
        window.focus(&self.modal_focus);
        cx.notify();
    }

    pub(super) fn sample_video(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.preview_only || self.video.is_some() {
            return;
        }
        use std::io::Write;
        let Ok(mut temporary) = tempfile::Builder::new().suffix(".mp4").tempfile() else {
            return;
        };
        if temporary
            .write_all(include_bytes!("../../tests/fixtures/video.mp4"))
            .is_err()
        {
            return;
        }
        let source = alhazen_gpui::Source::File(temporary.path().to_path_buf());
        self.close_settings(window, cx);
        self.start_video(source, "Sample video", None, Some(temporary), window, cx);
    }

    pub(super) fn close_video(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.video = None;
        self.video_url = None;
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus);
        }
        cx.notify();
    }

    pub(super) fn video_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        column()
            .id("video-overlay")
            .track_focus(&self.modal_focus)
            .occlude()
            .absolute()
            .inset_0()
            .top(px(32.))
            .bg(rgba(0x000000dd))
            .items_center()
            .justify_center()
            .child(
                column()
                    .w(relative(0.94))
                    .max_w(px(1000.))
                    .h(relative(0.92))
                    .min_h_0()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded_xl()
                    .overflow_hidden()
                    .child(
                        row()
                            .h(px(44.))
                            .flex_shrink_0()
                            .px_3()
                            .gap_3()
                            .child(div().flex_1().truncate().child(self.video_title.clone()))
                            .when_some(self.video_url.clone(), |s, url| {
                                s.child(
                                    button("original-video", "Open original ↗")
                                        .py_1()
                                        .on_click(move |_, _, cx| open_https(&url, cx)),
                                )
                            })
                            .child(button("close-video", "×").py_1().on_click(
                                cx.listener(|this, _, window, cx| this.close_video(window, cx)),
                            )),
                    )
                    .when_some(self.video.clone(), |s, video| s.child(video)),
            )
            .map(|element| fade_in(element, "video-enter", self.settings.animations))
    }
}
