use super::*;
use alhazen_gpui::{PlayerConfig, PlayerEvent, Source, VideoPlayer, video_view};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};
#[derive(Default)]
pub(super) struct Pool {
    items: HashMap<String, Entity<Loop>>,
    visible: HashSet<String>,
}
pub(super) type SharedPool = Rc<RefCell<Pool>>;
pub(super) struct Loop {
    player: Entity<VideoPlayer>,
    active: bool,
    _subscriptions: Vec<Subscription>,
}
impl Loop {
    fn new(source: Source, cx: &mut Context<Self>) -> Self {
        let player = cx.new(|cx| {
            VideoPlayer::new(
                source,
                PlayerConfig {
                    autoplay: false,
                    decoder_threads: 1,
                    frame_queue_len: 2,
                    packet_queue_len: 8,
                    max_output_size: Some((400, 300)),
                    audio_output: alhazen_gpui::alhazen_core::audio::AudioOutputConfig::Disabled,
                    ..Default::default()
                },
                cx,
            )
        });
        let subscriptions = vec![
            cx.observe(&player, |_, _, cx| cx.notify()),
            cx.subscribe(&player, |this, _, event: &PlayerEvent, cx| {
                if matches!(event, PlayerEvent::Ended) && this.active {
                    this.player.update(cx, |p, cx| {
                        p.seek(std::time::Duration::ZERO, cx);
                        p.play(cx);
                    });
                }
            }),
        ];
        Self {
            player,
            active: false,
            _subscriptions: subscriptions,
        }
    }
    fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active != active {
            self.active = active;
            self.player
                .update(cx, |p, cx| if active { p.play(cx) } else { p.pause(cx) });
        }
    }
}
impl Render for Loop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.set_active(window.is_window_active(), cx);
        video_view(self.player.clone()).size_full()
    }
}
impl Pool {
    pub(super) fn frame(pool: &SharedPool, window: &mut Window, cx: &mut App, active: bool) {
        {
            let mut pool = pool.borrow_mut();
            pool.visible.clear();
            if !active {
                for item in pool.items.values() {
                    item.update(cx, |view, cx| view.set_active(false, cx));
                }
            }
        }
        let pool = pool.clone();
        window.on_next_frame(move |window, cx| {
            let mut pool = pool.borrow_mut();
            let before = pool.items.len();
            let visible = pool.visible.clone();
            pool.items.retain(|url, _| visible.contains(url));
            if before != pool.items.len() {
                window.refresh();
            }
            let _ = cx;
        });
    }
    pub(super) fn pause(pool: &SharedPool, cx: &mut App) {
        for view in pool.borrow().items.values() {
            view.update(cx, |view, cx| view.set_active(false, cx));
        }
    }
}
impl Larpcord {
    pub(super) fn gif_card(
        &self,
        url: &str,
        poster: Option<&crate::model::EmbedMedia>,
        dimensions: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (width, height) = media_size(
            dimensions.or_else(|| poster.and_then(|p| p.width.zip(p.height))),
            400.,
            300.,
        );
        let still = poster
            .and_then(|p| p.proxy_url.as_deref().or(p.url.as_deref()))
            .filter(|u| crate::media::cdn_url(u))
            .and_then(|u| self.media.get(u));
        let animated = self.motion()
            && !self.preview_only
            && self.settings_tab.is_none()
            && !self.profile_is_modal()
            && self.video.is_none();
        let player = if animated {
            let mut pool = self.gif_pool.borrow_mut();
            pool.visible.insert(url.to_string());
            if !pool.items.contains_key(url)
                && pool.items.len() < 4
                && let Some(source) = crate::video::source(url)
            {
                pool.items
                    .insert(url.to_string(), cx.new(|cx| Loop::new(source, cx)));
            }
            pool.items.get(url).cloned()
        } else {
            None
        };
        div()
            .id(SharedString::from(format!("gif-{url}")))
            .debug_selector(|| "autoplay-gif".into())
            .w(px(width))
            .h(px(height))
            .max_w_full()
            .rounded_lg()
            .overflow_hidden()
            .bg(self.color(PANEL))
            .map(|s| {
                if let Some(player) = player {
                    s.child(player)
                } else if let Some(image) = still {
                    s.child(img(image).size_full().object_fit(ObjectFit::Contain))
                } else {
                    s.flex()
                        .items_center()
                        .justify_center()
                        .child(label(if self.motion() {
                            "Loading GIF…"
                        } else {
                            "GIF · reduced motion"
                        }))
                }
            })
            .into_any_element()
    }
}
