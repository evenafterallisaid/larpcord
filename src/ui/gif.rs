use super::*;
use alhazen_gpui::{PlayerConfig, PlayerEvent, PlayerState, Source, VideoPlayer, video_view};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};
#[derive(Default)]
pub(super) struct Pool {
    items: HashMap<String, Player>,
    visible: HashSet<String>,
}
pub(super) type SharedPool = Rc<RefCell<Pool>>;
enum Player {
    Video(Entity<Loop>),
    Image(Entity<ImageLoop>),
}
impl Player {
    fn set_active(&self, active: bool, cx: &mut App) {
        match self {
            Self::Video(view) => view.update(cx, |view, cx| view.set_active(active, cx)),
            Self::Image(view) => view.update(cx, |view, cx| view.set_active(active, cx)),
        }
    }
    fn element(&self) -> AnyElement {
        match self {
            Self::Video(view) => view.clone().into_any_element(),
            Self::Image(view) => view.clone().into_any_element(),
        }
    }
    fn set_poster(&self, poster: Option<std::sync::Arc<RenderImage>>, cx: &mut App) {
        let Some(poster) = poster else { return };
        match self {
            Self::Video(view) => view.update(cx, |view, cx| {
                if !view.has_frame && view.poster.is_none() {
                    view.poster = Some(poster);
                    cx.notify();
                }
            }),
            Self::Image(view) => view.update(cx, |view, cx| {
                if view.image.is_none() {
                    view.image = Some(poster);
                    cx.notify();
                }
            }),
        }
    }
}
struct ImageLoop {
    stream: crate::media::animation::Stream,
    image: Option<std::sync::Arc<RenderImage>>,
    retired: Vec<std::sync::Arc<RenderImage>>,
    active: bool,
    cover: bool,
    failed: bool,
    task: Option<Task<()>>,
}
impl ImageLoop {
    fn new(url: String, poster: Option<std::sync::Arc<RenderImage>>, cover: bool) -> Self {
        Self {
            stream: crate::media::animation::start(url),
            image: poster,
            retired: Vec::new(),
            active: false,
            cover,
            failed: false,
            task: None,
        }
    }
    fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        if !active {
            self.task = None;
        }
        if active && !self.failed && self.task.is_none() {
            self.next(cx);
        }
    }
    fn next(&mut self, cx: &mut Context<Self>) {
        let requests = self.stream.requests.clone();
        let frames = self.stream.frames.clone();
        self.task = Some(cx.spawn(async move |view, cx| {
            match requests.try_send(()) {
                Ok(()) | Err(std::sync::mpsc::TrySendError::Full(())) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(())) => {
                    let _ = view.update(cx, |this, cx| {
                        this.failed = true;
                        cx.notify();
                    });
                    return;
                }
            }
            let frame = frames.recv().await.ok().flatten();
            let delay = frame.as_ref().map(|f| f.delay);
            let _ = view.update(cx, |this, cx| {
                if let Some(frame) = frame {
                    if let Some(old) = this.image.replace(frame.image) {
                        this.retired.push(old);
                    }
                } else {
                    this.failed = true;
                }
                cx.notify();
            });
            if let Some(delay) = delay {
                cx.background_executor().timer(delay).await;
                let _ = view.update(cx, |this, cx| {
                    this.task = None;
                    if this.active {
                        this.next(cx);
                    }
                });
            }
        }));
    }
}
impl Render for ImageLoop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.set_active(window.is_window_active(), cx);
        crate::media::retire(std::mem::take(&mut self.retired), window, 3);
        div()
            .size_full()
            .when_some(self.image.clone(), |s, image| {
                s.child(img(image).size_full().object_fit(if self.cover {
                    ObjectFit::Cover
                } else {
                    ObjectFit::ScaleDown
                }))
            })
            .when(self.image.is_none() && !self.cover, |s| {
                s.child(gif_placeholder(if self.failed {
                    "GIF unavailable"
                } else {
                    "Loading GIF…"
                }))
            })
    }
}
pub(super) struct Loop {
    player: Entity<VideoPlayer>,
    active: bool,
    started: bool,
    has_frame: bool,
    poster: Option<std::sync::Arc<RenderImage>>,
    _subscriptions: Vec<Subscription>,
}
impl Loop {
    fn new(
        source: Source,
        poster: Option<std::sync::Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let player = cx.new(|cx| {
            VideoPlayer::new(
                source,
                PlayerConfig {
                    autoplay: false,
                    decoder_threads: 1,
                    frame_queue_len: 2,
                    packet_queue_len: 8,
                    max_output_size: Some((800, 600)),
                    audio_output: alhazen_gpui::alhazen_core::audio::AudioOutputConfig::Disabled,
                    ..Default::default()
                },
                cx,
            )
        });
        let subscriptions = vec![
            cx.observe(&player, |_, _, cx| cx.notify()),
            cx.subscribe(&player, |this, _, event: &PlayerEvent, cx| match event {
                PlayerEvent::StateChanged(_) => this.start_when_ready(cx),
                PlayerEvent::FrameReady if !this.has_frame => {
                    this.has_frame = true;
                    this.poster = None;
                    cx.notify();
                }
                // The player rewinds a seekable clip when play is called at the end.
                PlayerEvent::Ended if this.active => {
                    this.player.update(cx, |p, cx| p.play(cx));
                }
                _ => {}
            }),
        ];
        Self {
            player,
            active: false,
            started: false,
            has_frame: false,
            poster,
            _subscriptions: subscriptions,
        }
    }
    fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.start_when_ready(cx);
        } else if std::mem::take(&mut self.started) {
            self.player.update(cx, |p, cx| p.pause(cx));
        }
    }

    fn start_when_ready(&mut self, cx: &mut Context<Self>) {
        if self.active && !self.started && self.player.read(cx).player().is_some() {
            self.started = true;
            self.player.update(cx, |p, cx| p.play(cx));
        }
    }
}
impl Render for Loop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.set_active(window.is_window_active(), cx);
        let failed = matches!(self.player.read(cx).state(), PlayerState::Error(_));
        div()
            .relative()
            .size_full()
            .when(!self.has_frame, |s| {
                if let Some(poster) = self.poster.clone() {
                    s.child(img(poster).size_full().object_fit(ObjectFit::Contain))
                } else {
                    s.child(gif_placeholder(if failed {
                        "GIF unavailable"
                    } else {
                        "Loading GIF…"
                    }))
                }
            })
            .child(video_view(self.player.clone()).absolute().inset_0())
    }
}

fn gif_placeholder(text: &'static str) -> Div {
    row().size_full().justify_center().child(label(text))
}

impl Pool {
    pub(super) fn frame(pool: &SharedPool, window: &mut Window, cx: &mut App, active: bool) {
        {
            let mut pool = pool.borrow_mut();
            pool.visible.clear();
            if !active {
                for item in pool.items.values() {
                    item.set_active(false, cx);
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
            view.set_active(false, cx);
        }
    }
}
impl Larpcord {
    pub(super) fn banner_media(
        &self,
        url: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let poster = self.media.get(url);
        if active
            && self.motion()
            && !self.preview_only
            && self.settings_tab.is_none()
            && self.video.is_none()
        {
            let moving = crate::media::motion_url(url.to_string(), true);
            if moving != url && crate::media::animation::image_url(&moving) {
                let mut pool = self.banner_pool.borrow_mut();
                pool.visible.insert(moving.clone());
                if !pool.items.contains_key(&moving) && pool.items.len() < 2 {
                    let view = cx.new(|_| ImageLoop::new(moving.clone(), poster.clone(), true));
                    pool.items.insert(moving.clone(), Player::Image(view));
                }
                if let Some(Player::Image(view)) = pool.items.get(&moving) {
                    view.update(cx, |player, cx| {
                        if player.image.is_none() && poster.is_some() {
                            player.image = poster.clone();
                            cx.notify();
                        }
                        player.set_active(true, cx);
                    });
                    return Some(view.clone().into_any_element());
                }
            }
        }
        poster.map(|image| {
            img(image)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        })
    }

    pub(super) fn gif_player(
        &self,
        url: &str,
        still: Option<std::sync::Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let animated = self.motion()
            && !self.preview_only
            && self.settings_tab.is_none()
            && !self.profile_is_modal()
            && self.video.is_none();
        if animated {
            let mut pool = self.gif_pool.borrow_mut();
            pool.visible.insert(url.to_string());
            if !pool.items.contains_key(url)
                && pool.items.len() < 4
                && (crate::media::animation::image_url(url) || crate::video::source(url).is_some())
            {
                let player = if crate::media::animation::image_url(url) {
                    Player::Image(cx.new(|_| ImageLoop::new(url.to_string(), still.clone(), false)))
                } else {
                    Player::Video(
                        cx.new(|cx| {
                            Loop::new(crate::video::source(url).unwrap(), still.clone(), cx)
                        }),
                    )
                };
                pool.items.insert(url.to_string(), player);
            }
            pool.items.get(url).map(|player| {
                player.set_poster(still, cx);
                player.set_active(true, cx);
                player.element()
            })
        } else {
            None
        }
    }
    pub(super) fn gif_card(
        &self,
        url: &str,
        poster: Option<&crate::model::EmbedMedia>,
        dimensions: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let still = poster
            .and_then(|poster| {
                [poster.proxy_url.as_deref(), poster.url.as_deref()]
                    .into_iter()
                    .flatten()
                    .find_map(|url| self.media.get(url))
            })
            .or_else(|| {
                crate::media::animation::image_url(url)
                    .then(|| self.media.get(url))
                    .flatten()
            });
        let dimensions = dimensions
            .or_else(|| poster.and_then(|p| p.width.zip(p.height)))
            .filter(|(w, h)| *w > 0 && *h > 0)
            .or_else(|| {
                still.as_ref().map(|image| {
                    let size = image.size(0);
                    (size.width.0 as u32, size.height.0 as u32)
                })
            });
        let (width, height) = media_size(dimensions, 400., 300.);
        let player = if self.emoji_open && self.picker.tab == crate::expressions::Tab::Gifs {
            None
        } else {
            self.gif_player(url, still.clone(), cx)
        };
        let original = url.to_string();
        div()
            .id(SharedString::from(format!("gif-{url}")))
            .debug_selector(|| "autoplay-gif".into())
            .cursor_pointer()
            .tooltip(|_, cx| cx.new(|_| Hint("Open GIF".into())).into())
            .on_click(move |_, _, cx| open_https(&original, cx))
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
                    s.child(gif_placeholder("Open GIF"))
                }
            })
            .into_any_element()
    }
}

type GifEmbed<'a> = (
    &'a str,
    Option<&'a crate::model::EmbedMedia>,
    Option<(u32, u32)>,
);
pub(super) fn embed_gif(embed: &crate::model::Embed) -> Option<GifEmbed<'_>> {
    if embed.kind == "gifv"
        && let Some(video) = embed.video.as_ref()
        && let Some(url) = [video.proxy_url.as_deref(), video.url.as_deref()]
            .into_iter()
            .flatten()
            .find(|url| crate::video::source(url).is_some())
    {
        return Some((
            url,
            embed.thumbnail.as_ref().or(embed.image.as_ref()),
            video.width.zip(video.height),
        ));
    }
    if embed.description.is_some()
        || !embed.fields.is_empty()
        || embed.author.is_some()
        || embed.footer.is_some()
    {
        return None;
    }
    // A GIF image is media in its own right, even when Discord wraps it in an image embed.
    for image in [embed.image.as_ref(), embed.thumbnail.as_ref()]
        .into_iter()
        .flatten()
    {
        if let Some(url) = image
            .url
            .as_deref()
            .filter(|u| crate::media::animation::image_url(u))
            .or_else(|| {
                image
                    .proxy_url
                    .as_deref()
                    .filter(|u| crate::media::animation::image_url(u))
            })
        {
            return Some((url, Some(image), image.width.zip(image.height)));
        }
    }
    None
}
/// Only bare image links become media tiles. Rich bot cards retain their content.
pub(super) fn embed_image(embed: &crate::model::Embed) -> Option<&crate::model::EmbedMedia> {
    if embed.video.is_some()
        || embed.description.is_some()
        || !embed.fields.is_empty()
        || embed.author.is_some()
        || embed.footer.is_some()
        || !(embed.kind == "image" || (embed.kind.is_empty() && embed.title.is_none()))
    {
        return None;
    }
    embed
        .image
        .as_ref()
        .or(embed.thumbnail.as_ref())
        .filter(|image| image_source(image).is_some())
}
pub(super) fn image_source(image: &crate::model::EmbedMedia) -> Option<&str> {
    image
        .proxy_url
        .as_deref()
        .filter(|u| crate::media::cdn_url(u))
        .or_else(|| image.url.as_deref().filter(|u| crate::media::cdn_url(u)))
}
pub(super) fn direct_gif(message: &Message) -> Option<&str> {
    let url = message.content.trim();
    (message.flags & 4 == 0
        && url.split_whitespace().count() == 1
        && crate::media::animation::image_url(url)
        && !message
            .embeds
            .iter()
            .take(10)
            .any(|embed| embed_matches_link(embed, url))
        && !message
            .attachments
            .iter()
            .any(|file| file.url == url || file.proxy_url.as_deref() == Some(url)))
    .then_some(url)
}

pub(super) fn media_only(message: &Message) -> bool {
    if message.flags & 4 != 0 {
        return false;
    }
    let text = message.content.trim();
    gif_link(text, &message.embeds)
        || (!text.is_empty()
            && message
                .embeds
                .iter()
                .take(10)
                .any(|embed| embed_image(embed).is_some() && embed_matches_link(embed, text)))
}
pub(super) fn gif_only(message: &Message) -> bool {
    message.flags & 4 == 0
        && (gif_link(&message.content, &message.embeds) || direct_gif(message).is_some())
}
pub(super) fn gif_link(content: &str, embeds: &[crate::model::Embed]) -> bool {
    let text = content.trim();
    !text.is_empty()
        && embeds
            .iter()
            .take(10)
            .any(|embed| embed_gif(embed).is_some() && embed_matches_link(embed, text))
}

fn embed_matches_link(embed: &crate::model::Embed, text: &str) -> bool {
    embed.url.as_deref() == Some(text)
        || [
            embed.image.as_ref(),
            embed.thumbnail.as_ref(),
            embed.video.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|media| media.url.as_deref() == Some(text) || media.proxy_url.as_deref() == Some(text))
}

impl Larpcord {
    pub(super) fn send_gif(
        &mut self,
        gif: &crate::expressions::Gif,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.home_page
            || self.reaction_target.is_some()
            || self.sending.is_some()
            || self.failed_send.is_some()
            || self.settings_tab.is_some()
            || self.login_open
            || self.profile_is_modal()
            || self.video.is_some()
            || !crate::expressions::gif_url(&gif.url)
        {
            return;
        }
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        if !self.preview && self.status != "Connected" {
            self.error = Some("Waiting for the connection to return. The GIF was not sent.".into());
            cx.notify();
            return;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
            .to_string();
        let reply = self
            .reply_to
            .as_ref()
            .filter(|r| r.channel == channel)
            .map(|r| r.id.clone());
        if self.preview {
            self.store.insert(Message {
                id: nonce,
                channel_id: channel,
                content: gif.url.clone(),
                author: self.store.user.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                message_reference: reply.map(|id| serde_json::json!({"message_id":id})),
                ..Message::default()
            });
            self.reply_to = None;
            self.scrolled_away = false;
            self.refresh_messages();
        } else {
            if let Err(error) = self.backend.command(Command::Send {
                channel: channel.clone(),
                content: gif.url.clone(),
                nonce: nonce.clone(),
                reply,
                ping_reply: self.reply_to.as_ref().is_some_and(|r| r.ping),
                sticker: None,
                files: Vec::new(),
            }) {
                self.error = Some(error);
                cx.notify();
                return;
            }
            self.pending_reply = self.reply_to.take();
            self.pending_sticker = None;
            self.sending = Some((nonce, channel, gif.url.clone()));
        }
        self.settings.recent_gifs.retain(|g| g.url != gif.url);
        self.settings.recent_gifs.insert(0, gif.clone());
        self.settings.sanitize_recents();
        self.persist(cx);
        self.close_picker();
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Larpcord, Message, embed_gif, gif_only};
    use gpui::{AppContext, Modifiers, TestAppContext, point, px, size};

    fn choice() -> crate::expressions::Gif {
        crate::expressions::Gif {
            title: "cat".into(),
            url: "https://klipy.com/gifs/cat".into(),
            preview: None,
            thumbnail: None,
            width: 400,
            height: 240,
        }
    }
    #[::core::prelude::v1::test]
    fn only_hide_a_standalone_link_with_an_actual_gif_tile() {
        let mut message: Message = serde_json::from_value(serde_json::json!({"id":"5","channel_id":"campfire","author":{"id":"x"},"content":"https://cdn.discordapp.com/a.gif","embeds":[{"type":"image","url":"https://cdn.discordapp.com/a.gif","image":{"url":"https://cdn.discordapp.com/a.gif","width":400,"height":240}}]})).unwrap();
        assert!(gif_only(&message));
        assert!(embed_gif(&message.embeds[0]).is_some());
        message.content.insert_str(0, "look at this ");
        assert!(!gif_only(&message));
        message.content = "https://cdn.discordapp.com/a.gif".into();
        message.flags = 4;
        assert!(!gif_only(&message));
        message.flags = 0;
        message.embeds[0].description = Some("important bot help".into());
        assert!(embed_gif(&message.embeds[0]).is_none());
        assert!(!gif_only(&message));
        message.embeds.clear();
        assert_eq!(super::direct_gif(&message), Some(message.content.as_str()));
        assert!(gif_only(&message));
        message.flags = 4;
        assert!(super::direct_gif(&message).is_none());
        message.flags = 0;
        message.content = "https://untrusted.invalid/a.gif".into();
        assert!(super::direct_gif(&message).is_none());
    }

    #[::core::prelude::v1::test]
    fn gifv_falls_back_to_an_approved_video_when_the_proxy_is_invalid() {
        let direct = "https://cdn.discordapp.com/attachments/1/2/loop.mp4";
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "6",
            "channel_id": "campfire",
            "content": direct,
            "author": {"id": "x"},
            "embeds": [{
                "type": "gifv",
                "video": {"url": direct, "proxy_url": "https://untrusted.invalid/loop.mp4", "width": 320, "height": 200}
            }]
        })).unwrap();
        let (source, _, dimensions) = embed_gif(&message.embeds[0]).unwrap();
        assert_eq!(source, direct);
        assert_eq!(dimensions, Some((320, 200)));
        assert!(super::media_only(&message));
    }

    #[gpui::test]
    fn active_gifv_starts_when_the_native_decoder_finishes_opening(cx: &mut TestAppContext) {
        use super::{Loop, Source};
        use std::{path::Path, time::Duration};

        let player = cx.new(|cx| {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video.webm");
            let mut player = Loop::new(Source::File(path), None, cx);
            assert!(player.player.read(cx).player().is_none());
            player.set_active(true, cx);
            assert!(!player.started);
            player
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(40));
        cx.run_until_parked();
        player.update(cx, |view, cx| {
            assert!(
                view.player.read(cx).is_playing(),
                "decoder state: {:?}",
                view.player.read(cx).state()
            );
            view.set_active(false, cx);
            assert!(!view.player.read(cx).is_playing());
        });
    }
    #[::core::prelude::v1::test]
    fn linked_images_are_plain_tiles_but_rich_embeds_and_suppressed_links_stay_intact() {
        let mut m:Message=serde_json::from_value(serde_json::json!({"id":"1","channel_id":"campfire","author":{"id":"2"},"content":"https://example.com/cat.png","embeds":[{"type":"image","url":"https://example.com/cat.png","thumbnail":{"url":"https://example.com/cat.png","proxy_url":"https://images-ext-1.discordapp.net/external/hash/cat.png","width":200,"height":100}}]})).unwrap();
        assert!(super::media_only(&m));
        assert!(super::embed_image(&m.embeds[0]).is_some());
        m.content.insert_str(0, "look ");
        assert!(!super::media_only(&m));
        m.content = "https://example.com/cat.png".into();
        m.flags = 4;
        assert!(!super::media_only(&m));
        m.flags = 0;
        m.embeds[0].kind = "rich".into();
        m.embeds[0].description = Some("bot help".into());
        assert!(!super::media_only(&m));
        assert!(super::embed_image(&m.embeds[0]).is_none());
        m.embeds[0].description = None;
        m.embeds[0].kind = "image".into();
        m.embeds[0].thumbnail.as_mut().unwrap().proxy_url = None;
        assert!(
            !super::media_only(&m),
            "a link without an approved image source must remain visible"
        );
    }
    #[gpui::test]
    fn clicking_a_gif_sends_only_the_gif_and_preserves_the_composer(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.settings.animations = false;
            v.composer
                .update(cx, |input, cx| input.set("keep writing this", cx));
            v.emoji_open = true;
            v.picker.tab = crate::expressions::Tab::Gifs;
            v.picker.gifs = vec![choice()];
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let tile = cx.debug_bounds("gif-choice-0").unwrap();
        cx.simulate_click(tile.center(), Modifiers::default());
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(!v.emoji_open);
            assert_eq!(v.composer.read(cx).content.as_ref(), "keep writing this");
            assert!(
                v.store
                    .channel_messages("campfire")
                    .iter()
                    .any(|m| m.content == choice().url)
            );
            assert_eq!(v.settings.recent_gifs[0].url, choice().url);
        });
    }
    #[gpui::test]
    fn a_favorite_star_does_not_send_the_gif(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.settings.animations = false;
            v.emoji_open = true;
            v.picker.tab = crate::expressions::Tab::Gifs;
            v.picker.gifs = vec![choice()];
            v.picker.favorites.clear();
            v.picker.favorites_loading = false;
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let tile = cx.debug_bounds("gif-choice-0").unwrap();
        cx.simulate_click(
            point(tile.right() - px(20.), tile.top() + px(20.)),
            Modifiers::default(),
        );
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(v.emoji_open);
            assert!(v.sending.is_none());
            assert_eq!(v.picker.favorites.len(), 1);
            assert!(
                !v.store
                    .channel_messages("campfire")
                    .iter()
                    .any(|m| m.content == choice().url)
            );
        });
    }
    #[gpui::test]
    fn paused_image_loops_stop_requesting_frames(cx: &mut TestAppContext) {
        let (requests, received) = std::sync::mpsc::sync_channel(1);
        let (results, frames) = async_channel::bounded(1);
        let image = std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(8, 8),
        )]));
        results
            .try_send(Some(crate::media::animation::AnimatedFrame {
                image,
                delay: std::time::Duration::from_millis(100),
            }))
            .unwrap();
        let view = cx.new(|_| super::ImageLoop {
            stream: crate::media::animation::Stream { requests, frames },
            image: None,
            retired: vec![],
            active: false,
            cover: false,
            failed: false,
            task: None,
        });
        view.update(cx, |v, cx| v.set_active(true, cx));
        cx.run_until_parked();
        assert!(received.try_recv().is_ok());
        view.update(cx, |v, cx| v.set_active(false, cx));
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        assert!(received.try_recv().is_err());
        view.update(cx, |v, cx| v.set_active(true, cx));
        cx.run_until_parked();
        assert!(received.try_recv().is_ok());
    }
    #[gpui::test]
    fn disconnected_gif_clicks_do_not_modify_the_draft_or_queue_a_send(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |v, window, cx| {
                v.preview = false;
                v.status = "Reconnecting".into();
                v.emoji_open = true;
                v.composer.update(cx, |input, cx| input.set("safe", cx));
                v.send_gif(&choice(), window, cx);
                assert!(v.sending.is_none());
                assert!(v.emoji_open);
                assert_eq!(v.composer.read(cx).content.as_ref(), "safe");
            })
            .unwrap();
    }
    #[gpui::test]
    fn gif_acknowledgement_does_not_consume_a_staged_sticker_or_text(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |v, window, cx| {
                v.preview = false;
                v.status = "Connected".into();
                v.composer.update(cx, |input, cx| input.set("text", cx));
                v.selected_sticker = Some(crate::expressions::Sticker {
                    id: "123".into(),
                    name: "sticker".into(),
                    format_type: 1,
                    ..Default::default()
                });
                v.send_gif(&choice(), window, cx);
                let nonce = v.sending.as_ref().unwrap().0.clone();
                assert_eq!(v.sending.as_ref().unwrap().2, choice().url);
                v.send_gif(&choice(), window, cx);
                assert_eq!(v.sending.as_ref().unwrap().0, nonce);
                v.receive(
                    crate::network::Envelope {
                        generation: 0,
                        event: crate::network::Event::Sent {
                            nonce,
                            result: Ok(Box::new(Message {
                                id: "999".into(),
                                channel_id: "campfire".into(),
                                content: choice().url,
                                ..Default::default()
                            })),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(v.composer.read(cx).content.as_ref(), "text");
                assert_eq!(v.selected_sticker.as_ref().unwrap().id, "123");
                assert!(v.sending.is_none());
            })
            .unwrap();
    }
}
