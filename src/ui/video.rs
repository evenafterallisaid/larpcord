use super::*;

impl Larpcord {
    pub(super) fn video_card(&self, url: &str, title: &str, cx: &mut Context<Self>) -> AnyElement {
        let url = url.to_string();
        let title = title.to_string();
        column()
            .id(SharedString::from(format!("video-{url}")))
            .w(px(400.))
            .max_w_full()
            .p_4()
            .gap_3()
            .rounded_lg()
            .bg(rgb(PANEL))
            .child(label(title.clone()).truncate())
            .child(button("play-video", "▶ Play video").on_click(
                cx.listener(move |this, _, window, cx| this.open_video(&url, &title, window, cx)),
            ))
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
            .into_any_element()
    }
}
