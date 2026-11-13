mod components;
mod cosmetics;
mod profile;
mod text;
mod video;

use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::icons::icon;
use gpui::{div, list, prelude::*, px, rgb, *};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    auth::{AuthEvent, QrImage},
    input::{Changed, Submit, TextInput},
    model::{Channel, Message, Page, Profile, Store, User},
    network::{Backend, Command, Envelope, Event, guild_channels, guild_from_ready},
    settings::Settings,
};

const BG: u32 = 0x111214;
const PANEL: u32 = 0x09090b;
const SIDEBAR: u32 = 0x0b0b0d;
const SURFACE: u32 = 0x1e1f22;
const LINE: u32 = 0x252529;
const TEXT: u32 = 0xdcddde;
const MUTED: u32 = 0x96989d;
const ACCENT: u32 = 0xe2e4e9;
const LILAC: u32 = 0xc1b5ed;

actions!(larpcord, [Quit, OpenSettings, ClosePanel, FocusSearch]);

pub struct Larpcord {
    root_focus: FocusHandle,
    modal_focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    backend: Backend,
    store: Store,
    settings: Settings,
    generation: u64,
    preview: bool,
    preview_only: bool,
    status: String,
    selected_guild: Option<String>,
    expanded_folders: HashSet<String>,
    guild_details_attempted: HashMap<String, std::time::Instant>,
    selected_channel: Option<String>,
    composer: Entity<TextInput>,
    search: Entity<TextInput>,
    token: Entity<TextInput>,
    messages: ListState,
    rendered_ids: Vec<String>,
    settings_tab: Option<usize>,
    error: Option<String>,
    loading: HashMap<String, (u64, Page)>,
    pages: HashMap<String, Paging>,
    page_request: u64,
    typing: HashMap<String, (String, std::time::Instant)>,
    typing_task: Option<Task<()>>,
    typing_styles: HashMap<String, crate::cosmetics::TypingStyle>,
    hovered_name: Option<String>,
    read_task: Option<Task<()>>,
    typing_sent: Option<(String, std::time::Instant)>,
    scrolled_away: bool,
    media: crate::media::Media,
    profile: Option<Profile>,
    profile_guild: Option<String>,
    profile_request: u64,
    profile_started: std::time::Instant,
    component_menu: Option<(Message, crate::components::Component)>,
    video: Option<Entity<crate::video::Playback>>,
    video_title: String,
    video_url: Option<String>,
    component_values: HashSet<String>,
    interaction: Option<String>,
    profile_error: Option<String>,
    profiles: HashMap<(String, Option<String>), (Profile, std::time::Instant)>,
    failed_send: Option<(String, String)>,
    live_tail: Vec<Message>,
    drafts: HashMap<String, String>,
    sending: Option<(String, String, String)>,
    login_open: bool,
    qr_attempt: u64,
    qr: QrState,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct Paging {
    no_older: bool,
    newer: bool,
    latest: Option<String>,
}

enum QrState {
    Idle,
    Generating,
    Ready(std::sync::Arc<QrImage>),
    Confirm,
    Failed(String),
}

fn column() -> Div {
    div().flex().flex_col()
}
fn row() -> Div {
    div().flex().items_center()
}
fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.))
        .text_color(rgb(MUTED))
        .child(text.into())
}
fn badge(text: &'static str) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(SURFACE))
        .text_size(px(10.))
        .text_color(rgb(ACCENT))
        .child(text)
}
fn button(id: impl Into<ElementId>, text: &'static str) -> Stateful<Div> {
    let glyph = match text {
        "×" => Some(("close", "Close")),
        "⚙" => Some(("settings", "Settings")),
        "−" => Some(("minus", "Minimize")),
        "□" => Some(("maximize", "Maximize")),
        "↗" => Some(("external-link", "Open attachment")),
        _ => None,
    };
    let mut result = row()
        .id(id)
        .gap_2()
        .px_4()
        .py_2()
        .rounded_lg()
        .cursor_pointer()
        .bg(rgb(SURFACE))
        .text_size(px(13.))
        .hover(|s| s.bg(rgb(LINE)));
    if let Some((name, hint)) = glyph {
        result = result
            .child(icon(name, 18.))
            .tooltip(move |_, cx| cx.new(|_| Hint(hint.into())).into());
    } else if text == "↓ Jump to latest" {
        result = result
            .child(icon("arrow-down", 16.))
            .child("Jump to latest");
    } else if let Some(label) = text.strip_suffix(" ↗") {
        result = result.child(label).child(icon(
            if label == "Send" {
                "send"
            } else {
                "external-link"
            },
            16.,
        ));
    } else {
        result = result.child(text);
    }
    result
}

fn fade_in<E: IntoElement + Styled + 'static>(
    element: E,
    id: &'static str,
    enabled: bool,
) -> AnyElement {
    if enabled {
        element
            .with_animation(
                id,
                Animation::new(std::time::Duration::from_millis(150)).with_easing(ease_out_quint()),
                |element, delta| element.opacity(delta),
            )
            .into_any_element()
    } else {
        element.into_any_element()
    }
}

fn ping_badge(count: u32) -> Div {
    row()
        .min_w(px(18.))
        .h(px(18.))
        .px(px(4.))
        .justify_center()
        .rounded_full()
        .border_2()
        .border_color(rgb(PANEL))
        .bg(rgb(0xf23f43))
        .text_color(rgb(0xffffff))
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .child(if count > 999 {
            "999+".into()
        } else {
            count.to_string()
        })
}

fn media_size(dimensions: Option<(u32, u32)>, max_width: f32, max_height: f32) -> (f32, f32) {
    let ratio = dimensions
        .filter(|(w, h)| *w > 0 && *h > 0)
        .map_or(16. / 9., |(w, h)| w as f32 / h as f32)
        .clamp(0.2, 8.);
    let width = max_width.min(max_height * ratio);
    (width, width / ratio)
}

fn open_https(url: &str, cx: &mut App) {
    if let Ok(url) = reqwest::Url::parse(url)
        && url.scheme() == "https"
    {
        cx.open_url(url.as_str());
    }
}

struct Hint(String);
impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .max_w(px(260.))
            .rounded_md()
            .shadow_lg()
            .bg(rgb(0x111214))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

impl Larpcord {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_preview(window, cx, std::env::args().any(|arg| arg == "--preview"))
    }

    fn with_preview(window: &mut Window, cx: &mut Context<Self>, preview_only: bool) -> Self {
        let root_focus = cx.focus_handle();
        window.focus(&root_focus);
        let (backend, events) = Backend::start(!preview_only);
        let (media, images) = crate::media::Media::new();
        cx.spawn_in(window, async move |view, cx| {
            while let Ok(image) = images.recv().await {
                if view
                    .update_in(cx, |view, window, cx| {
                        if image.image.is_some()
                            && view
                                .profile
                                .as_ref()
                                .and_then(|p| p.effect_asset.as_ref())
                                .is_some_and(|e| {
                                    e.effects.iter().any(|layer| {
                                        layer.src == image.url.trim_end_matches("#motion")
                                    })
                                })
                        {
                            view.profile_started = std::time::Instant::now();
                        }
                        crate::media::retire(view.media.receive(image), window, 3);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let store = Store::demo();
        let rendered_ids: Vec<_> = store
            .channel_messages("campfire")
            .iter()
            .map(|m| m.id.clone())
            .collect();
        let messages = ListState::new(rendered_ids.len(), ListAlignment::Bottom, px(300.));
        let weak = cx.weak_entity();
        messages.set_scroll_handler(move |event, window, cx| {
            if let Some(view) = weak.upgrade() {
                view.update(cx, |view: &mut Self, cx| {
                    if view.settings_tab.is_some()
                        || view.login_open
                        || view.profile.is_some()
                        || view.component_menu.is_some()
                        || view.video.is_some()
                    {
                        return;
                    }
                    view.scrolled_away = event.is_scrolled;
                    if !view.scrolled_away {
                        view.mark_view_read(window, cx);
                    }
                    if event.visible_range.start < 4 {
                        view.load_history(Page::Before);
                    } else if event.visible_range.end + 4 >= event.count {
                        view.load_history(Page::After);
                    }
                    cx.notify();
                });
            }
        });
        let composer = cx.new(|cx| TextInput::new("Message the campfire", false, cx));
        let search = cx.new(|cx| TextInput::new("Find a conversation", false, cx));
        let token = cx.new(|cx| TextInput::new("Paste a session token", true, cx));
        let subscriptions = vec![
            cx.observe_window_activation(window, |this, window, cx| {
                this.mark_view_read(window, cx);
                cx.notify();
            }),
            cx.subscribe_in(&composer, window, |this, _, _: &Submit, window, cx| {
                this.send(window, cx)
            }),
            cx.subscribe_in(&token, window, |this, _, _: &Submit, window, cx| {
                this.login(window, cx)
            }),
            cx.subscribe(&search, |_, _, _: &Changed, cx| cx.notify()),
            cx.subscribe_in(&composer, window, |this, _, _: &Changed, window, cx| {
                if this.composer.read(cx).content.is_empty()
                    || this.preview
                    || this.settings_tab.is_some()
                    || this.login_open
                    || this.profile.is_some()
                    || this.component_menu.is_some()
                    || this.video.is_some()
                    || !window.is_window_active()
                    || this.status != "Connected"
                {
                    return;
                }
                let Some(channel) = this.selected_channel.clone() else {
                    return;
                };
                if this.typing_sent.as_ref().is_some_and(|(id, at)| {
                    id == &channel && at.elapsed() < std::time::Duration::from_secs(8)
                }) {
                    return;
                }
                this.typing_sent = Some((channel.clone(), std::time::Instant::now()));
                this.command(Command::Typing(channel));
            }),
        ];
        cx.spawn_in(window, async move |view, cx| {
            while let Ok(event) = events.recv().await {
                if view
                    .update_in(cx, |view, window, cx| {
                        if !preview_only {
                            view.receive(event, window, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            if let Some(view) = weak.upgrade()
                && view.read(cx).settings.keep_connected
            {
                window.minimize_window();
                false
            } else {
                cx.quit();
                true
            }
        });
        Self {
            root_focus,
            modal_focus: cx.focus_handle(),
            previous_focus: None,
            backend,
            store,
            settings: if preview_only {
                Settings::default()
            } else {
                Settings::load()
            },
            generation: 0,
            preview: true,
            preview_only,
            status: "Preview".into(),
            selected_guild: Some("larp".into()),
            expanded_folders: HashSet::new(),
            guild_details_attempted: HashMap::new(),
            selected_channel: Some("campfire".into()),
            composer,
            search,
            token,
            messages,
            rendered_ids,
            settings_tab: None,
            error: None,
            loading: HashMap::new(),
            pages: HashMap::new(),
            page_request: 0,
            typing: HashMap::new(),
            typing_task: None,
            typing_styles: HashMap::new(),
            hovered_name: None,
            read_task: None,
            typing_sent: None,
            scrolled_away: false,
            media,
            profile: None,
            profile_guild: None,
            profile_request: 0,
            profile_started: std::time::Instant::now(),
            component_menu: None,
            video: None,
            video_title: String::new(),
            video_url: None,
            component_values: HashSet::new(),
            interaction: None,
            profile_error: None,
            profiles: HashMap::new(),
            failed_send: None,
            live_tail: Vec::new(),
            drafts: HashMap::new(),
            sending: None,
            login_open: false,
            qr_attempt: 0,
            qr: QrState::Idle,
            _subscriptions: subscriptions,
        }
    }

    fn open_settings(&mut self, tab: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.login_open
            || self.profile.is_some()
            || self.component_menu.is_some()
            || self.video.is_some()
        {
            return;
        }
        if self.settings_tab.is_none() {
            self.previous_focus = window.focused(cx);
        }
        self.settings_tab = Some(tab);
        window.focus(&self.modal_focus);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_tab = None;
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus);
        } else {
            window.focus(&self.root_focus);
        }
        cx.notify();
    }

    fn command(&mut self, command: Command) {
        if let Err(error) = self.backend.command(command) {
            self.error = Some(error);
        }
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        if !self.preview_only && self.settings.save().is_err() {
            self.error = Some("Could not save settings to your configuration folder.".into());
        }
        cx.notify();
    }

    fn login(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let token = Zeroizing::new(self.token.read(cx).content.trim().to_string());
        if token.len() < 30 || !token.is_ascii() || token.chars().any(char::is_whitespace) {
            self.error = Some("Paste a valid Discord session token to connect.".into());
            cx.notify();
            return;
        }
        self.token.update(cx, |input, cx| input.set("", cx));
        self.connect(token, cx);
    }

    fn open_login(&mut self, cx: &mut Context<Self>) {
        self.login_open = true;
        self.settings_tab = None;
        self.qr_attempt += 1;
        self.qr = QrState::Generating;
        self.command(Command::QrStart {
            attempt: self.qr_attempt,
        });
        cx.notify();
    }

    fn close_login(&mut self, cx: &mut Context<Self>) {
        self.login_open = false;
        self.qr_attempt += 1;
        self.command(Command::QrCancel);
        self.qr = QrState::Idle;
        cx.notify();
    }

    fn connect(&mut self, token: Zeroizing<String>, cx: &mut Context<Self>) {
        self.generation += 1;
        self.preview = false;
        self.store = Store::default();
        self.selected_guild = None;
        self.selected_channel = None;
        self.loading.clear();
        self.pages.clear();
        self.live_tail.clear();
        self.read_task = None;
        self.typing.clear();
        self.typing_styles.clear();
        self.typing_task = None;
        self.typing_sent = None;
        self.profile = None;
        self.component_menu = None;
        self.video = None;
        self.component_values.clear();
        self.interaction = None;
        self.profiles.clear();
        self.guild_details_attempted.clear();
        self.failed_send = None;
        self.drafts.clear();
        self.sending = None;
        self.composer.update(cx, |input, cx| input.set("", cx));
        self.messages.reset(0);
        self.rendered_ids.clear();
        self.error = None;
        self.status = "Connecting".into();
        self.command(Command::Connect {
            generation: self.generation,
            token,
            remember: self.settings.remember_session,
        });
        cx.notify();
    }

    fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.command(Command::Disconnect);
        self.command(Command::Forget);
        self.store = Store::demo();
        self.preview = true;
        self.status = "Preview".into();
        self.selected_guild = Some("larp".into());
        self.selected_channel = Some("campfire".into());
        self.sending = None;
        self.loading.clear();
        self.pages.clear();
        self.live_tail.clear();
        self.read_task = None;
        self.typing.clear();
        self.typing_styles.clear();
        self.typing_task = None;
        self.typing_sent = None;
        self.profile = None;
        self.component_menu = None;
        self.video = None;
        self.component_values.clear();
        self.interaction = None;
        self.profiles.clear();
        self.guild_details_attempted.clear();
        self.failed_send = None;
        self.drafts.clear();
        self.composer.update(cx, |input, cx| input.set("", cx));
        self.messages
            .reset(self.store.channel_messages("campfire").len());
        self.rendered_ids = self
            .store
            .channel_messages("campfire")
            .iter()
            .map(|m| m.id.clone())
            .collect();
        cx.notify();
    }

    fn choose_channel(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(channel) = self.store.channels.get(&id)
            && !channel.readable()
        {
            self.error = Some(
                "Native voice is planned for a later milestone. Text channels are available now."
                    .into(),
            );
            cx.notify();
            return;
        }
        if let Some(old) = &self.selected_channel {
            self.drafts
                .insert(old.clone(), self.composer.read(cx).content.to_string());
            if self.drafts.len() > 32 {
                self.drafts.retain(|id, _| id == old);
            }
        }
        self.live_tail.clear();
        self.read_task = None;
        self.typing.clear();
        self.typing_styles.clear();
        self.typing_task = None;
        self.typing_sent = None;
        self.selected_channel = Some(id.clone());
        if !self.preview {
            self.command(Command::ViewGuild(
                self.store
                    .channels
                    .get(&id)
                    .and_then(|c| c.guild_id.clone()),
            ));
            if let Some(guild) = self
                .store
                .channels
                .get(&id)
                .and_then(|c| c.guild_id.clone())
                && self
                    .guild_details_attempted
                    .get(&guild)
                    .is_none_or(|at| at.elapsed() > std::time::Duration::from_secs(300))
            {
                if self.guild_details_attempted.len() >= 256 {
                    self.guild_details_attempted.clear();
                }
                let started = std::time::Instant::now();
                self.guild_details_attempted.insert(guild.clone(), started);
                self.command(Command::GuildDetails(guild, started));
            }
        }
        let state = self.pages.entry(id.clone()).or_default();
        state.latest = state.latest.take().or_else(|| {
            self.store
                .channels
                .get(&id)
                .and_then(|c| c.last_message_id.clone())
        });
        self.store.pin(&id);

        self.error = None;
        let draft = self.drafts.get(&id).cloned().unwrap_or_default();
        let placeholder = format!(
            "Message {}",
            self.store
                .channels
                .get(&id)
                .map(Channel::label)
                .unwrap_or_default()
        );
        self.composer.update(cx, |input, cx| {
            input.placeholder = placeholder.into();
            input.set(&draft, cx);
        });
        self.scrolled_away = false;
        if self.store.has_history(&id) {
            self.mark_view_read(window, cx);
        }
        self.messages.reset(self.store.channel_messages(&id).len());
        self.rendered_ids = self
            .store
            .channel_messages(&id)
            .iter()
            .map(|m| m.id.clone())
            .collect();
        if !self.preview {
            self.load_history(Page::Latest);
        }
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }

    fn mark_view_read(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview
            || self.status != "Connected"
            || !window.is_window_active()
            || self.scrolled_away
            || self.settings_tab.is_some()
            || self.profile.is_some()
            || self.video.is_some()
            || self.component_menu.is_some()
            || self.login_open
        {
            return;
        }
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        if self.pages.get(&channel).is_some_and(|p| p.newer) {
            return;
        }
        let Some(message) = self
            .store
            .channels
            .get(&channel)
            .and_then(|c| c.last_message_id.clone())
        else {
            return;
        };
        self.store.mark_read(&channel);
        let generation = self.generation;
        self.read_task = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(700))
                .await;
            let _ = view.update_in(cx, |this, window, _| {
                if this.generation == generation
                    && window.is_window_active()
                    && this.selected_channel.as_ref() == Some(&channel)
                    && !this.scrolled_away
                    && this.profile.is_none()
                    && this.settings_tab.is_none()
                    && this.video.is_none()
                    && this.component_menu.is_none()
                    && !this.login_open
                {
                    this.command(Command::Ack { channel, message });
                }
            });
        }));
    }

    fn load_history(&mut self, page: Page) {
        if self.preview {
            return;
        }
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        if self.loading.len() >= 8
            || self
                .loading
                .get(&channel)
                .is_some_and(|(_, pending)| page != Page::Latest || *pending == Page::Latest)
        {
            return;
        }
        let state = self.pages.entry(channel.clone()).or_default();
        if (page == Page::Before && state.no_older) || (page == Page::After && !state.newer) {
            return;
        }
        let messages = self.store.channel_messages(&channel);
        let cursor = match page {
            Page::Latest => None,
            Page::Before => messages.first().map(|m| m.id.clone()),
            Page::After => messages.last().map(|m| m.id.clone()),
        };
        if page != Page::Latest && cursor.is_none() {
            return;
        }
        self.page_request += 1;
        self.loading
            .insert(channel.clone(), (self.page_request, page));
        if let Err(error) = self.backend.command(Command::History {
            channel: channel.clone(),
            started: self.store.revision(),
            request: self.page_request,
            page,
            cursor,
        }) {
            self.loading.remove(&channel);
            self.error = Some(error);
        }
    }

    fn jump_latest(&mut self, cx: &mut Context<Self>) {
        self.scrolled_away = false;
        let newer = self
            .selected_channel
            .as_ref()
            .and_then(|id| self.pages.get(id))
            .is_some_and(|p| p.newer);
        let pending = self
            .selected_channel
            .as_ref()
            .is_some_and(|id| self.loading.contains_key(id));
        if newer || pending {
            self.load_history(Page::Latest);
        }
        self.messages.reset(self.rendered_ids.len());
        cx.notify();
    }

    fn send(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.sending.is_some()
            || self.settings_tab.is_some()
            || self.login_open
            || self.profile.is_some()
            || self.component_menu.is_some()
            || self.video.is_some()
        {
            return;
        }
        if self.failed_send.is_some() {
            self.error = Some("Copy or dismiss the saved unsent message before sending another. Your new draft is safe.".into());
            cx.notify();
            return;
        }
        let content = self.composer.read(cx).content.trim().to_string();
        if content.is_empty() {
            return;
        }
        if content.chars().count() > 2000 {
            self.error = Some("Messages can contain up to 2,000 characters.".into());
            cx.notify();
            return;
        }
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
            .to_string();
        if self.preview {
            self.store.insert(Message {
                id: nonce,
                channel_id: channel,
                content,
                author: self.store.user.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                ..Message::default()
            });
            self.composer.update(cx, |input, cx| input.set("", cx));
            self.refresh_messages();
        } else if self.status != "Connected" {
            self.error = Some("Waiting for the connection to return. Your draft is safe.".into());
        } else {
            match self.backend.command(Command::Send {
                channel: channel.clone(),
                content: content.clone(),
                nonce: nonce.clone(),
            }) {
                Ok(()) => {
                    self.drafts.remove(&channel);
                    self.sending = Some((nonce, channel, content));
                    self.composer.update(cx, |input, cx| input.set("", cx));
                }
                Err(error) => self.error = Some(error),
            }
        }
        cx.notify();
    }

    fn refresh_messages(&mut self) {
        let scroll = self.messages.logical_scroll_top();
        let anchor = self.rendered_ids.get(scroll.item_ix).cloned();
        let ids: Vec<_> = self
            .selected_channel
            .as_deref()
            .map_or(&[][..], |id| self.store.channel_messages(id))
            .iter()
            .map(|m| m.id.clone())
            .collect();
        self.messages.reset(ids.len());
        if self.scrolled_away
            && let Some(anchor) = anchor
            && let Some(index) = ids.iter().position(|id| *id == anchor)
        {
            self.messages.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: scroll.offset_in_item,
            });
        }
        self.rendered_ids = ids;
    }

    fn receive(&mut self, envelope: Envelope, window: &mut Window, cx: &mut Context<Self>) {
        if let Event::SavedSession(token) = envelope.event {
            if self.generation == 0 {
                if let Some(token) = token {
                    self.connect(token, cx);
                } else {
                    self.open_login(cx);
                }
            }
            return;
        }
        if envelope.generation != self.generation {
            return;
        }
        match envelope.event {
            Event::Auth { attempt, event } => {
                if attempt != self.qr_attempt || !self.login_open {
                    return;
                }
                match event {
                    AuthEvent::Code(code) => self.qr = QrState::Ready(code),
                    AuthEvent::Confirm => self.qr = QrState::Confirm,
                    AuthEvent::Failed(error) => self.qr = QrState::Failed(error),
                    AuthEvent::Token(token) => {
                        self.qr = QrState::Idle;
                        self.connect(token, cx);
                    }
                }
            }
            Event::Layout(result) => {
                if let Ok(value) = result
                    && let Some(layout) = crate::navigation::Layout::parse(&value)
                {
                    self.store.apply_layout(layout);
                }
            }
            Event::GuildDetails {
                id,
                started,
                result,
            } => {
                if self.guild_details_attempted.get(&id) == Some(&started)
                    && let Ok(guild) = result
                    && let Some(existing) =
                        self.store.guilds.iter_mut().find(|guild| guild.id == id)
                {
                    existing.banner = guild.banner;
                    existing.icon = guild.icon;
                    existing.premium_subscription_count = guild.premium_subscription_count;
                    existing.premium_tier = guild.premium_tier;
                }
            }
            Event::Status(status) => {
                if status == "Connected" {
                    self.error = None;
                }
                self.status = status;
            }
            Event::Error(error) => {
                if self.login_open {
                    self.qr = QrState::Failed(error.clone());
                }
                self.error = Some(error);
                self.loading.clear();
            }
            Event::History {
                channel,
                started,
                request,
                page,
                result,
            } => {
                if self.loading.get(&channel).map(|(id, _)| *id) != Some(request) {
                    return;
                }
                self.loading.remove(&channel);
                match result {
                    Ok(messages) => {
                        let count = messages.len();
                        let fetched_last = messages
                            .iter()
                            .map(|m| m.id.clone())
                            .max_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
                        let first_before = self
                            .store
                            .channel_messages(&channel)
                            .first()
                            .map(|m| m.id.clone());
                        self.store.merge_history(&channel, messages, started, page);
                        if self.selected_channel.as_ref() == Some(&channel)
                            && (page == Page::Latest || (page == Page::After && count < 100))
                        {
                            for message in self.live_tail.drain(..) {
                                self.store.insert(message);
                            }
                        }
                        let state = self.pages.entry(channel.clone()).or_default();
                        match page {
                            Page::Latest => {
                                state.latest = state
                                    .latest
                                    .take()
                                    .into_iter()
                                    .chain(fetched_last)
                                    .chain(
                                        self.store
                                            .channel_messages(&channel)
                                            .last()
                                            .map(|m| m.id.clone()),
                                    )
                                    .max_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
                                state.no_older = count < 100;
                            }
                            Page::Before => state.no_older = count < 100,
                            Page::After => {
                                if first_before
                                    != self
                                        .store
                                        .channel_messages(&channel)
                                        .first()
                                        .map(|m| m.id.clone())
                                {
                                    state.no_older = false;
                                }
                                if count < 100 {
                                    state.latest = self
                                        .store
                                        .channel_messages(&channel)
                                        .last()
                                        .map(|m| m.id.clone());
                                }
                            }
                        }
                        state.newer = state.latest.as_ref().is_some_and(|id| {
                            self.store
                                .channel_messages(&channel)
                                .last()
                                .is_some_and(|m| {
                                    m.id.len() < id.len() || (m.id.len() == id.len() && m.id < *id)
                                })
                        });
                        if self.pages.len() > 16 {
                            self.pages.retain(|id, _| id == &channel);
                        }
                        if self.selected_channel.as_ref() == Some(&channel) {
                            if page == Page::Latest {
                                self.scrolled_away = false;
                            }
                            self.refresh_messages();
                            self.mark_view_read(window, cx);
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Event::Profile {
                id,
                guild,
                request,
                result,
            } => {
                let current = self.profile.as_ref().is_some_and(|p| p.user.id == id)
                    && self.profile_guild == guild
                    && self.profile_request == request;
                match result {
                    Ok(profile) => {
                        if !profile.roles.is_empty()
                            && let Some(server) = guild
                                .as_ref()
                                .and_then(|id| self.store.guilds.iter_mut().find(|g| &g.id == id))
                        {
                            server.roles = profile.roles.clone();
                        }
                        if current {
                            self.profile = Some((*profile).clone());
                        }
                        if self.profiles.len() >= 32 {
                            self.profiles.clear();
                        }
                        self.profiles
                            .insert((id, guild), (*profile, std::time::Instant::now()));
                    }
                    Err(error) => {
                        if current {
                            self.profile_error = Some(error);
                        }
                    }
                }
            }
            Event::ProfileEffect {
                id,
                guild,
                request,
                effect,
                frame,
            } => {
                let frame = frame.map(|f| *f);
                let effect = effect.map(|e| *e);
                if self.profile_request == request
                    && self.profile_guild == guild
                    && let Some(profile) = self.profile.as_mut().filter(|p| p.user.id == id)
                {
                    self.profile_started = std::time::Instant::now();
                    profile.effect_asset = effect.clone();
                    profile.frame_asset = frame.clone();
                }
                if let Some((profile, _)) = self.profiles.get_mut(&(id, guild)) {
                    profile.effect_asset = effect;
                    profile.frame_asset = frame;
                }
            }
            Event::Interaction { message, result } => {
                if self.interaction.as_ref() == Some(&message) {
                    self.interaction = None;
                }
                if let Err(error) = result {
                    self.error = Some(error);
                }
            }
            Event::Sent { nonce, result } => {
                if self
                    .sending
                    .as_ref()
                    .is_some_and(|(pending, _, _)| *pending == nonce)
                {
                    let (_, channel, content) = self.sending.take().unwrap();
                    match result {
                        Ok(message) => {
                            let message = *message;
                            let selected = self.selected_channel.as_ref() == Some(&channel);
                            let older = self.pages.get(&channel).is_some_and(|p| p.newer);
                            if older && selected {
                                self.live_tail.push(message);
                                self.load_history(Page::Latest);
                            } else if !older {
                                self.store.insert(message);
                                if selected {
                                    self.scrolled_away = false;
                                    self.refresh_messages();
                                }
                            }
                        }
                        Err(error) => {
                            self.failed_send = Some((channel, content));
                            self.error = Some(format!(
                                "{error} Your unsent message is saved below. Check the channel before retrying."
                            ));
                        }
                    }
                }
            }
            Event::Dispatch(kind, data) => {
                let selected = data["channel_id"].as_str() == self.selected_channel.as_deref();
                if kind == "TYPING_START" && !selected {
                    return;
                }
                let already_unread = data["channel_id"]
                    .as_str()
                    .is_some_and(|id| self.store.unread.contains(id));
                let old_mentions = data["channel_id"]
                    .as_str()
                    .and_then(|id| self.store.mentions.get(id))
                    .copied()
                    .unwrap_or(0);
                let channel = data["channel_id"].as_str().map(str::to_string);
                let message_event = kind.starts_with("MESSAGE_");
                let redraw = kind == "MESSAGE_ACK"
                    || !message_event
                    || selected
                    || (kind == "MESSAGE_CREATE"
                        && (!already_unread
                            || (self.selected_guild.is_none()
                                && data["channel_id"]
                                    .as_str()
                                    .and_then(|id| self.store.channels.get(id))
                                    .is_some_and(|channel| channel.guild_id.is_none()))));
                self.dispatch(&kind, data, window, cx);
                let mentions_changed = channel.as_ref().is_some_and(|id| {
                    self.store.mentions.get(id).copied().unwrap_or(0) != old_mentions
                });
                if !redraw && !mentions_changed {
                    return;
                }
            }
            Event::SavedSession(_) => {}
        }
        cx.notify();
    }

    fn expire_typing_later(&mut self, cx: &mut Context<Self>) {
        let Some(deadline) = self.typing.values().map(|(_, deadline)| *deadline).min() else {
            self.typing_task = None;
            return;
        };
        self.typing_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(deadline.saturating_duration_since(std::time::Instant::now()))
                .await;
            let _ = view.update(cx, |view, cx| {
                let now = std::time::Instant::now();
                view.typing.retain(|_, (_, deadline)| *deadline > now);
                view.typing_styles
                    .retain(|id, _| view.typing.contains_key(id));
                view.expire_typing_later(cx);
                cx.notify();
            });
        }));
    }

    fn typing_label(&self) -> String {
        let mut names: Vec<_> = self
            .typing
            .values()
            .map(|(name, _)| name.as_str())
            .collect();
        names.sort_unstable();
        match names.as_slice() {
            [] => String::new(),
            [name] => format!(
                "{name} is {}…",
                self.typing
                    .keys()
                    .next()
                    .and_then(|id| self.typing_styles.get(id))
                    .map_or("typing", |s| s.verb())
            ),
            [a, b] => format!("{a} and {b} are typing…"),
            [a, b, ..] => format!("{a}, {b} and others are typing…"),
        }
    }

    fn dispatch(&mut self, kind: &str, data: Value, window: &mut Window, cx: &mut Context<Self>) {
        match kind {
            "READY" => {
                self.login_open = false;
                self.qr = QrState::Idle;
                if let Ok(user) = serde_json::from_value(data["user"].clone()) {
                    self.store.user = user;
                }
                self.store.guilds.clear();
                self.store.channels.clear();
                for guild in data["guilds"].as_array().into_iter().flatten() {
                    self.add_guild(guild);
                }
                for channel in data["private_channels"].as_array().into_iter().flatten() {
                    if let Ok(channel) = serde_json::from_value::<Channel>(channel.clone()) {
                        self.store.channels.insert(channel.id.clone(), channel);
                    }
                }
                self.store.unread.clear();
                self.store.mentions.clear();
                self.store.read.clear();
                self.store.users.clear();
                for user in data["users"].as_array().into_iter().flatten().take(2000) {
                    if let Ok(user) = serde_json::from_value::<User>(user.clone()) {
                        self.store.users.insert(user.id.clone(), user);
                    }
                }
                self.store.own_roles.clear();
                self.store.mention_suppression.clear();
                for entry in data["user_guild_settings"]
                    .get("entries")
                    .unwrap_or(&data["user_guild_settings"])
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    self.store.guild_preferences(entry);
                }
                for (index, guild) in data["guilds"].as_array().into_iter().flatten().enumerate() {
                    if let Some(id) = guild["id"].as_str() {
                        for member in data["merged_members"][index]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .chain(guild["members"].as_array().into_iter().flatten())
                        {
                            if member["user_id"].as_str().or(member["user"]["id"].as_str())
                                == Some(&self.store.user.id)
                            {
                                self.store.own_roles.insert(
                                    id.into(),
                                    member["roles"]
                                        .as_array()
                                        .into_iter()
                                        .flatten()
                                        .filter_map(Value::as_str)
                                        .map(str::to_string)
                                        .collect(),
                                );
                            }
                        }
                    }
                }
                self.store.read_states(&data["read_state"]);
                if let Some(layout) = crate::navigation::Layout::parse(&data) {
                    self.store.apply_layout(layout);
                }
                if !self.preview {
                    self.command(Command::Layout);
                }
                self.settings_tab = None;
                if self.selected_channel.is_none() {
                    let newest = self
                        .store
                        .channels
                        .values()
                        .filter(|c| c.guild_id.is_none())
                        .max_by_key(|c| c.activity())
                        .map(|c| c.id.clone());
                    if let Some(id) = newest {
                        self.choose_channel(id, window, cx);
                    }
                } else if let Some(id) = self.selected_channel.clone() {
                    if self.store.channels.contains_key(&id) {
                        self.load_history(Page::Latest);
                    } else {
                        self.selected_channel = None;
                        self.selected_guild = None;
                        self.refresh_messages();
                    }
                }
            }
            "USER_UPDATE" => {
                let Some(id) = data["id"].as_str() else {
                    return;
                };
                let existing = if self.store.user.id == id {
                    Some(&self.store.user)
                } else {
                    self.store.users.get(id)
                };
                let mut merged = existing
                    .and_then(|u| serde_json::to_value(u).ok())
                    .unwrap_or_else(|| serde_json::json!({"id":id}));
                if let Some(fields) = data.as_object() {
                    for (key, value) in fields {
                        merged[key] = value.clone();
                    }
                }
                if let Ok(user) = serde_json::from_value::<User>(merged) {
                    self.profiles.retain(|(id, _), _| id != &user.id);
                    if self.store.user.id == user.id {
                        self.store.user = user.clone();
                    }
                    if let Some(profile) = self.profile.as_mut().filter(|p| p.user.id == user.id) {
                        profile.user = user.clone();
                    }
                    if self.store.users.contains_key(&user.id) {
                        self.store.users.insert(user.id.clone(), user);
                    }
                }
            }
            "USER_SETTINGS_UPDATE" | "USER_SETTINGS_PROTO_UPDATE" => {
                if let Some(mut layout) = crate::navigation::Layout::parse(&data) {
                    if data.get("guild_folders").is_none() && data.get("guild_positions").is_some()
                    {
                        layout.folders = self.store.layout.folders.clone();
                    }
                    if data.get("guild_positions").is_none() && data.get("guild_folders").is_some()
                    {
                        layout.positions = self.store.layout.positions.clone();
                    }
                    self.store.apply_layout(layout);
                }
            }
            "MESSAGE_ACK" => self.store.read_state(&data),
            "USER_GUILD_SETTINGS_UPDATE" => self.store.guild_preferences(&data),
            "GUILD_CREATE" | "GUILD_UPDATE" => self.add_guild(&data),
            "GUILD_DELETE" => {
                if data["unavailable"].as_bool() != Some(true)
                    && let Some(id) = data["id"].as_str()
                {
                    self.store.guilds.retain(|g| g.id != id);
                    self.store
                        .channels
                        .retain(|_, c| c.guild_id.as_deref() != Some(id));
                }
            }
            "CHANNEL_CREATE" | "CHANNEL_UPDATE" => {
                if let Ok(channel) = serde_json::from_value::<Channel>(data) {
                    self.store.channels.insert(channel.id.clone(), channel);
                }
            }
            "CHANNEL_DELETE" => {
                if let Some(id) = data["id"].as_str() {
                    self.store.channels.remove(id);
                }
            }
            "TYPING_START" => {
                if data["channel_id"].as_str() != self.selected_channel.as_deref() {
                    return;
                }
                let Some(user) = data["user_id"]
                    .as_str()
                    .filter(|id| *id != self.store.user.id)
                else {
                    return;
                };
                let author = serde_json::from_value::<crate::model::Member>(data["member"].clone())
                    .ok()
                    .and_then(|m| m.user)
                    .or_else(|| {
                        self.selected_channel.as_deref().and_then(|id| {
                            self.store
                                .channel_messages(id)
                                .iter()
                                .find(|m| m.author.id == user)
                                .map(|m| m.author.clone())
                        })
                    })
                    .or_else(|| self.store.users.get(user).cloned())
                    .or_else(|| {
                        self.selected_channel
                            .as_ref()
                            .and_then(|id| self.store.channels.get(id))
                            .and_then(|c| c.recipients.iter().find(|u| u.id == user))
                            .cloned()
                    });
                if let Some(style) = data
                    .get("typing_indicator_style")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .or_else(|| {
                        author
                            .as_ref()
                            .and_then(|u| u.typing_indicator_style.clone())
                    })
                {
                    self.typing_styles.insert(user.into(), style);
                }
                let name = data["member"]["nick"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| author.map(|u| u.name().to_string()))
                    .unwrap_or_else(|| "Someone".into());
                if self.typing.len() < 32 || self.typing.contains_key(user) {
                    self.typing.insert(
                        user.into(),
                        (
                            name,
                            std::time::Instant::now() + std::time::Duration::from_secs(10),
                        ),
                    );
                    self.expire_typing_later(cx);
                }
            }
            "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" => {
                if let Some(guild) = self
                    .store
                    .guilds
                    .iter_mut()
                    .find(|g| Some(g.id.as_str()) == data["guild_id"].as_str())
                    && let Ok(role) =
                        serde_json::from_value::<crate::model::Role>(data["role"].clone())
                {
                    guild.roles.retain(|r| r.id != role.id);
                    guild.roles.push(role);
                }
            }
            "GUILD_ROLE_DELETE" => {
                if let Some(guild) = self
                    .store
                    .guilds
                    .iter_mut()
                    .find(|g| Some(g.id.as_str()) == data["guild_id"].as_str())
                {
                    guild
                        .roles
                        .retain(|r| Some(r.id.as_str()) != data["role_id"].as_str());
                }
            }
            "GUILD_MEMBER_UPDATE" => {
                let id = data["user"]["id"].as_str();
                let guild = data["guild_id"].as_str();
                if id == Some(&self.store.user.id)
                    && let Some(guild) = guild
                    && let Some(roles) = data["roles"].as_array()
                {
                    self.store.own_roles.insert(
                        guild.into(),
                        roles
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect(),
                    );
                }
                self.profiles.retain(|(user, server), _| {
                    Some(user.as_str()) != id || server.as_deref() != guild
                });
                if let Some(profile) = &mut self.profile
                    && Some(profile.user.id.as_str()) == id
                    && self.profile_guild.as_deref() == guild
                    && let Ok(member) = serde_json::from_value(data.clone())
                {
                    profile.guild_member = Some(member);
                }
            }
            "MESSAGE_CREATE" => {
                if let Ok(message) = serde_json::from_value::<Message>(data) {
                    let fresh = self
                        .store
                        .channels
                        .get(&message.channel_id)
                        .is_none_or(|c| message.id.parse::<u64>().unwrap_or(0) > c.activity());
                    self.store.note_message(&message.channel_id, &message.id);
                    let selected = self.selected_channel.as_ref() == Some(&message.channel_id);
                    let own = message.author.id == self.store.user.id;
                    if selected {
                        self.typing.remove(&message.author.id);
                        self.expire_typing_later(cx);
                    }
                    if !own
                        && !(selected
                            && window.is_window_active()
                            && !self.scrolled_away
                            && self.settings_tab.is_none()
                            && self.profile.is_none()
                            && self.video.is_none())
                    {
                        self.store.unread.insert(message.channel_id.clone());
                        let dm = self
                            .store
                            .channels
                            .get(&message.channel_id)
                            .is_some_and(|c| c.guild_id.is_none());
                        let mentioned = self.store.mentioned(&message);
                        if fresh && (dm || mentioned) {
                            let count = self
                                .store
                                .mentions
                                .entry(message.channel_id.clone())
                                .or_default();
                            *count = count.saturating_add(1);
                        }
                        if fresh && self.settings.notifications && (dm || mentioned) {
                            self.command(Command::Notify {
                                title: format!("{} on Larpcord", message.author.name()),
                                body: if self.settings.notification_preview {
                                    message.content.chars().take(160).collect()
                                } else {
                                    "You have a new message.".into()
                                },
                            });
                        }
                    }
                    // only the viewed conversation receives live bodies. other channels keep an unread bit.
                    if selected {
                        let state = self.pages.entry(message.channel_id.clone()).or_default();
                        state.latest = Some(message.id.clone());
                        if !state.newer {
                            self.store.insert(message);
                            self.refresh_messages();
                            self.mark_view_read(window, cx);
                        } else {
                            self.live_tail.push(message);
                            if self.live_tail.len() > 200 {
                                self.live_tail.remove(0);
                            }
                        }
                    }
                }
            }
            "MESSAGE_UPDATE" => {
                let selected = data["channel_id"].as_str() == self.selected_channel.as_deref();
                for message in &mut self.live_tail {
                    if Some(message.id.as_str()) == data["id"].as_str() {
                        Store::apply_patch(message, &data);
                    }
                }
                self.store.update(&data);
                if selected {
                    self.refresh_messages();
                }
            }
            "MESSAGE_DELETE" => {
                if let (Some(channel), Some(id)) =
                    (data["channel_id"].as_str(), data["id"].as_str())
                {
                    self.live_tail
                        .retain(|m| m.channel_id != channel || m.id != id);
                    if self.store.has_history(channel) {
                        self.store.delete(channel, id);
                    }
                    if self.selected_channel.as_deref() == Some(channel) {
                        self.refresh_messages();
                    }
                }
            }
            "MESSAGE_DELETE_BULK" => {
                if let Some(channel) = data["channel_id"].as_str() {
                    for id in data["ids"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        self.live_tail
                            .retain(|m| m.channel_id != channel || m.id != id);
                        if self.store.has_history(channel) {
                            self.store.delete(channel, id);
                        }
                    }
                    if self.selected_channel.as_deref() == Some(channel) {
                        self.refresh_messages();
                    }
                }
            }
            "CALL_CREATE" => {
                let ringing = data["ringing"].as_array().is_some_and(|users| {
                    users
                        .iter()
                        .any(|id| id.as_str() == Some(self.store.user.id.as_str()))
                });
                if ringing && self.settings.notifications {
                    self.command(Command::Notify { title: "Incoming Discord call".into(), body: "Open a client with voice support to answer. Native voice is coming to Larpcord.".into() });
                }
            }
            _ => {}
        }
    }

    fn add_guild(&mut self, data: &Value) {
        if let Some(mut guild) = guild_from_ready(data) {
            // a later gateway update wins over an older metadata request.
            self.guild_details_attempted.remove(&guild.id);
            if let Some(existing) = self.store.guilds.iter_mut().find(|g| g.id == guild.id) {
                let properties = data.get("properties").unwrap_or(data);
                if properties.get("name").is_none() && data.get("name").is_none() {
                    guild.name = existing.name.clone();
                }
                if properties.get("icon").is_none() && data.get("icon").is_none() {
                    guild.icon = existing.icon.clone();
                }
                if properties.get("banner").is_none() && data.get("banner").is_none() {
                    guild.banner = existing.banner.clone();
                }
                if properties.get("premium_subscription_count").is_none()
                    && data.get("premium_subscription_count").is_none()
                {
                    guild.premium_subscription_count = existing.premium_subscription_count;
                }
                if properties.get("premium_tier").is_none() && data.get("premium_tier").is_none() {
                    guild.premium_tier = existing.premium_tier;
                }
                if data.get("roles").is_none() && properties.get("roles").is_none() {
                    guild.roles = existing.roles.clone();
                }
                *existing = guild;
            } else {
                self.store.guilds.push(guild);
                self.store.apply_layout(self.store.layout.clone());
            }
        }
        for channel in guild_channels(data) {
            self.store.channels.insert(channel.id.clone(), channel);
        }
    }

    fn titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        row()
            .h(px(32.))
            .flex_shrink_0()
            .px_3()
            .gap_2()
            .bg(rgb(PANEL))
            .when(cfg!(target_os = "macos"), |s| s.pl(px(86.)))
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                if event.click_count == 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
            })
            .child(
                img(ImageSource::Resource(Resource::Embedded(
                    "mascot.png".into(),
                )))
                .size(px(20.)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("Larpcord"),
            )
            .child(div().flex_1())
            .when(!cfg!(target_os = "macos"), |s| {
                s.children([
                    button("minimize", "")
                        .child(icon("minus", 16.))
                        .py_0()
                        .on_click(|_, window, _| window.minimize_window())
                        .into_any_element(),
                    button("maximize", "")
                        .child(icon("maximize", 16.))
                        .py_0()
                        .on_click(|_, window, _| window.zoom_window())
                        .into_any_element(),
                    button("close-window", "")
                        .child(icon("close", 16.))
                        .py_0()
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.settings.keep_connected {
                                window.minimize_window();
                            } else {
                                cx.quit();
                            }
                        }))
                        .into_any_element(),
                ])
            })
    }

    fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut dm_ids = self
            .store
            .channels
            .values()
            .filter(|c| {
                c.guild_id.is_none() && self.store.mentions.get(&c.id).is_some_and(|n| *n > 0)
            })
            .collect::<Vec<_>>();
        dm_ids.sort_by_key(|c| std::cmp::Reverse(c.activity()));
        let dm_ids = dm_ids.into_iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        let mut items = (0..dm_ids.len()).map(|i| (2u8, i)).collect::<Vec<_>>();
        let mut seen = HashSet::new();
        for (index, folder) in self.store.layout.folders.iter().enumerate() {
            let grouped = folder.id.is_some();
            if grouped {
                items.push((1, index));
            }
            let expanded = folder
                .id
                .as_ref()
                .is_none_or(|id| self.expanded_folders.contains(id));
            for id in &folder.guild_ids {
                if !seen.insert(id.clone()) {
                    continue;
                }
                if expanded && let Some(index) = self.store.guilds.iter().position(|g| &g.id == id)
                {
                    items.push((0, index));
                }
            }
        }
        for (index, guild) in self.store.guilds.iter().enumerate() {
            if seen.insert(guild.id.clone()) {
                items.push((0, index));
            }
        }
        column()
            .w(px(72.))
            .h_full()
            .flex_shrink_0()
            .items_center()
            .py_4()
            .gap_3()
            .bg(rgb(PANEL))
            .child(
                row()
                    .id("home")
                    .size(px(40.))
                    .justify_center()
                    .rounded_xl()
                    .bg(rgb(if self.selected_guild.is_none() {
                        0x5865f2
                    } else {
                        SURFACE
                    }))
                    .cursor_pointer()
                    .child(svg().path("home.svg").size(px(28.)).text_color(rgb(TEXT)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.selected_guild = None;
                        this.search.update(cx, |input, cx| input.set("", cx));
                        let channel = this
                            .store
                            .channels
                            .values()
                            .filter(|c| c.guild_id.is_none() && c.readable())
                            .max_by_key(|c| c.activity())
                            .map(|c| c.id.clone());
                        if let Some(channel) = channel {
                            this.choose_channel(channel, window, cx);
                        } else {
                            this.selected_channel = None;
                            this.command(Command::ViewGuild(None));
                            this.refresh_messages();
                        }
                        cx.notify();
                    })),
            )
            .child(div().w(px(26.)).h(px(1.)).bg(rgb(LINE)))
            .child(
                div().flex_1().min_h_0().w_full().overflow_hidden().child(
                    uniform_list(
                        "servers",
                        items.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| match items[index].0 {
                                    1 => this.folder_row(items[index].1, cx),
                                    2 => this.dm_rail(&dm_ids[items[index].1], cx),
                                    _ => this.guild_row(items[index].1, cx),
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .size_full(),
                ),
            )
            .child(
                div()
                    .size(px(7.))
                    .rounded_full()
                    .bg(rgb(if self.status == "Connected" {
                        ACCENT
                    } else {
                        MUTED
                    })),
            )
            .child(label(self.status.clone()).text_size(px(9.)))
    }

    fn dm_rail(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some(channel) = self.store.channels.get(id) else {
            return div().into_any_element();
        };
        let id = id.to_string();
        let name = channel.label();
        let count = self.store.mentions.get(&id).copied().unwrap_or(0);
        row()
            .relative()
            .w_full()
            .h(px(48.))
            .justify_center()
            .child(
                div()
                    .id(SharedString::from(format!("unread-dm-{id}")))
                    .cursor_pointer()
                    .child(
                        channel
                            .recipients
                            .first()
                            .map(|user| self.avatar(user, 40.))
                            .unwrap_or_else(|| icon("user", 40.).into_any_element()),
                    )
                    .tooltip(move |_, cx| cx.new(|_| Hint(name.clone())).into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_guild = None;
                        this.choose_channel(id.clone(), window, cx);
                    })),
            )
            .child(
                div()
                    .absolute()
                    .right(px(5.))
                    .bottom_0()
                    .child(ping_badge(count)),
            )
            .into_any_element()
    }

    fn folder_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(folder) = self.store.layout.folders.get(index) else {
            return div().into_any_element();
        };
        let Some(id) = folder.id.clone() else {
            return div().into_any_element();
        };
        let name = folder
            .name
            .clone()
            .unwrap_or_else(|| "Server folder".into());
        let pings = folder
            .guild_ids
            .iter()
            .map(|id| self.store.server_mentions(id))
            .fold(0, u32::saturating_add);
        let color = folder.color.unwrap_or(0x5865f2);
        let unread = folder
            .guild_ids
            .iter()
            .any(|id| self.store.server_unread(id));
        let mut grid = row().flex_wrap().w(px(32.)).h(px(32.)).gap(px(2.));
        for id in folder.guild_ids.iter().take(4) {
            let guild = self.store.guilds.iter().find(|g| &g.id == id);
            let artwork = guild
                .and_then(|g| crate::media::guild_icon(id, g.icon.as_deref()))
                .and_then(|url| self.media.get(&url));
            grid = grid.child(
                div()
                    .size(px(15.))
                    .rounded_sm()
                    .overflow_hidden()
                    .bg(rgb(SURFACE))
                    .when_some(artwork, |s, image| {
                        s.child(img(image).size_full().object_fit(ObjectFit::Cover))
                    }),
            );
        }
        row()
            .relative()
            .h(px(48.))
            .w_full()
            .justify_center()
            .when(unread, |s| {
                s.child(
                    div()
                        .absolute()
                        .left_0()
                        .w(px(4.))
                        .h(px(8.))
                        .rounded_r_md()
                        .bg(rgb(TEXT)),
                )
            })
            .child(
                row()
                    .id(SharedString::from(format!("folder-{id}")))
                    .size(px(40.))
                    .justify_center()
                    .rounded_xl()
                    .bg(rgba((color << 8) | 60))
                    .cursor_pointer()
                    .child(grid)
                    .tooltip(move |_, cx| cx.new(|_| Hint(name.clone())).into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded_folders.remove(&id) {
                            this.expanded_folders.insert(id.clone());
                        }
                        cx.notify();
                    })),
            )
            .when(pings > 0, |s| {
                s.child(
                    div()
                        .absolute()
                        .right(px(5.))
                        .bottom(px(1.))
                        .child(ping_badge(pings)),
                )
            })
            .into_any_element()
    }

    fn guild_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(guild) = self.store.guilds.get(index) else {
            return div().into_any_element();
        };
        let id = guild.id.clone();
        let name = guild.name.clone();
        let selected = self.selected_guild.as_ref() == Some(&id);
        let pings = self.store.server_mentions(&id);
        let unread = self.store.server_unread(&id);
        let artwork = crate::media::guild_icon(&id, guild.icon.as_deref())
            .and_then(|url| self.media.get(&url));
        let initials: String = guild
            .name
            .split_whitespace()
            .take(2)
            .filter_map(|s| s.chars().next())
            .collect();
        row()
            .relative()
            .w_full()
            .h(px(48.))
            .justify_center()
            .when(unread && !selected, |s| {
                s.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(22.))
                        .size(px(4.))
                        .rounded_full()
                        .bg(rgb(TEXT)),
                )
            })
            .when(selected, |s| {
                s.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(14.))
                        .w(px(3.))
                        .h(px(20.))
                        .rounded_full()
                        .bg(rgb(TEXT))
                        .map(|element| {
                            if self.settings.animations {
                                element
                                    .with_animation(
                                        ("selected-server", index),
                                        Animation::new(std::time::Duration::from_millis(140))
                                            .with_easing(ease_out_quint()),
                                        |element, delta| {
                                            element
                                                .h(px(4. + 16. * delta))
                                                .top(px(24. - (4. + 16. * delta) / 2.))
                                        },
                                    )
                                    .into_any_element()
                            } else {
                                element.into_any_element()
                            }
                        }),
                )
            })
            .child(
                row()
                    .id(SharedString::from(format!("guild-{id}")))
                    .size(px(40.))
                    .justify_center()
                    .rounded_xl()
                    .cursor_pointer()
                    .bg(rgb(if selected { 0x5865f2 } else { SURFACE }))
                    .text_color(rgb(if selected { LILAC } else { MUTED }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .hover(|s| s.bg(rgb(LINE)))
                    .overflow_hidden()
                    .map(|s| match artwork {
                        Some(image) => s.child(
                            img(image)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .rounded_xl(),
                        ),
                        None => s.child(initials),
                    })
                    .tooltip(move |_, cx| cx.new(|_| Hint(name.clone())).into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_guild = Some(id.clone());
                        this.search.update(cx, |input, cx| input.set("", cx));
                        let channel = this
                            .store
                            .channels
                            .values()
                            .filter(|c| c.guild_id.as_ref() == Some(&id) && c.readable())
                            .min_by(|a, b| {
                                a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id))
                            })
                            .map(|c| c.id.clone());
                        if let Some(channel) = channel {
                            this.choose_channel(channel, window, cx);
                        }
                        cx.notify();
                    })),
            )
            .when(pings > 0, |s| {
                s.child(
                    div()
                        .absolute()
                        .right(px(5.))
                        .bottom(px(1.))
                        .child(ping_badge(pings)),
                )
            })
            .into_any_element()
    }

    fn dm_row(&self, channel: &Channel, cx: &mut Context<Self>) -> AnyElement {
        let id = channel.id.clone();
        let selected = self.selected_channel.as_ref() == Some(&id);
        let unread = self.store.unread.contains(&id);
        let pings = self.store.mentions.get(&id).copied().unwrap_or(0);
        let content = row()
            .w_full()
            .id(SharedString::from(format!("dm-{id}")))
            .relative()
            .h(px(44.))
            .when_some(
                channel
                    .recipients
                    .first()
                    .and_then(|user| self.nameplate(user, None)),
                |s, image| {
                    s.child(
                        img(image)
                            .absolute()
                            .inset_0()
                            .size_full()
                            .rounded_md()
                            .object_fit(ObjectFit::Cover),
                    )
                },
            )
            .px_2()
            .gap_3()
            .rounded_md()
            .cursor_pointer()
            .bg(rgb(if selected { 0x232428 } else { SIDEBAR }))
            .hover(|s| s.bg(rgb(SURFACE)))
            .child(
                channel
                    .recipients
                    .first()
                    .map(|user| self.avatar(user, 32.))
                    .unwrap_or_else(|| {
                        div()
                            .size(px(28.))
                            .rounded_full()
                            .bg(rgb(SURFACE))
                            .child("@")
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if selected || unread { TEXT } else { MUTED }))
                    .relative()
                    .child(
                        channel
                            .recipients
                            .first()
                            .map(|user| self.display_name(&channel.label(), user, None, false))
                            .unwrap_or_else(|| div().child(channel.label()).into_any_element()),
                    ),
            )
            .when(pings > 0, |s| s.child(ping_badge(pings)))
            .when(unread && pings == 0, |s| {
                s.child(div().size(px(6.)).rounded_full().bg(rgb(TEXT)))
            })
            .on_click(
                cx.listener(move |this, _, window, cx| this.choose_channel(id.clone(), window, cx)),
            )
            .into_any_element();
        div().w_full().px_2().child(content).into_any_element()
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let guild = self
            .selected_guild
            .as_ref()
            .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id));
        let title = guild
            .map_or("Direct Messages", |g| g.name.as_str())
            .to_string();
        let banner = guild.and_then(|g| crate::media::banner(&g.id, None, g.banner.as_deref()?));
        let banner_image = banner.as_ref().and_then(|url| self.media.get(url));
        let query = self.search.read(cx).content.to_lowercase();
        let mut channels: Vec<_> = self
            .store
            .channels
            .values()
            .filter(|channel| {
                channel.guild_id == self.selected_guild && (channel.readable() || channel.kind == 2)
            })
            .filter(|channel| query.is_empty() || channel.label().to_lowercase().contains(&query))
            .cloned()
            .collect();
        channels.sort_by(|a, b| {
            if self.selected_guild.is_none() {
                return b
                    .activity()
                    .cmp(&a.activity())
                    .then_with(|| a.id.cmp(&b.id));
            }
            let category = |channel: &Channel| {
                channel
                    .parent_id
                    .as_ref()
                    .and_then(|id| self.store.channels.get(id))
                    .map_or(-1, |c| c.position)
            };
            category(a)
                .cmp(&category(b))
                .then_with(|| (a.kind == 2).cmp(&(b.kind == 2)))
                .then_with(|| a.position.cmp(&b.position))
                .then_with(|| a.label().cmp(&b.label()))
        });
        let mut navigation = Vec::new();
        let mut last_group = None;
        for channel in &channels {
            if self.selected_guild.is_none() {
                break;
            }
            let group = (
                channel.parent_id.clone(),
                channel.kind == 2 && channel.parent_id.is_none(),
            );
            if last_group.as_ref() != Some(&group) && self.selected_guild.is_some() {
                let title = channel
                    .parent_id
                    .as_ref()
                    .and_then(|id| self.store.channels.get(id))
                    .map_or_else(
                        || {
                            if channel.kind == 2 {
                                "VOICE CHANNELS".into()
                            } else {
                                "TEXT CHANNELS".into()
                            }
                        },
                        Channel::label,
                    );
                navigation.push(
                    label(title.to_uppercase())
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .px_2()
                        .pt_4()
                        .pb_1()
                        .into_any_element(),
                );
                last_group = Some(group);
            }
            let id = channel.id.clone();
            let selected = self.selected_channel.as_ref() == Some(&id);
            navigation.push(
                row()
                    .id(SharedString::from(format!("channel-{id}")))
                    .h(px(if channel.guild_id.is_none() { 40. } else { 32. }))
                    .px_2()
                    .gap_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(if selected { 0x232428 } else { SIDEBAR }))
                    .hover(|s| s.bg(rgb(SURFACE)))
                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                    .child(if channel.guild_id.is_none() {
                        channel
                            .recipients
                            .first()
                            .map(|user| self.avatar(user, 28.))
                            .unwrap_or_else(|| {
                                div()
                                    .size(px(28.))
                                    .rounded_full()
                                    .bg(rgb(SURFACE))
                                    .child("@")
                                    .into_any_element()
                            })
                    } else {
                        div()
                            .w(px(22.))
                            .text_size(px(21.))
                            .text_color(rgb(0x80848e))
                            .child(icon(if channel.kind == 2 { "volume" } else { "hash" }, 18.))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(15.))
                            .child(channel.label()),
                    )
                    .when(
                        self.store.mentions.get(&id).copied().unwrap_or(0) > 0,
                        |s| s.child(ping_badge(self.store.mentions[&id])),
                    )
                    .when(
                        self.store.unread.contains(&id)
                            && self.store.mentions.get(&id).copied().unwrap_or(0) == 0,
                        |s| s.child(div().size(px(6.)).rounded_full().bg(rgb(TEXT))),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_channel(id.clone(), window, cx)
                    }))
                    .into_any_element(),
            );
        }
        column()
            .w(px(280.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(SIDEBAR))
            .child(
                column()
                    .relative()
                    .h(px(if banner.is_some() { 148. } else { 48. }))
                    .flex_shrink_0()
                    .justify_start()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .when_some(banner_image, |s, image| {
                        s.child(
                            img(image)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    })
                    .child(
                        row()
                            .relative()
                            .h(px(48.))
                            .px_4()
                            .bg(if banner.is_some() {
                                rgba(0x00000088)
                            } else {
                                rgba(0x00000000)
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(icon("chevron-down", 18.)),
                    ),
            )
            .when_some(
                guild.and_then(|guild| {
                    guild
                        .premium_subscription_count
                        .map(|count| (count, guild.premium_tier))
                }),
                |s, (count, tier)| {
                    s.child(
                        row()
                            .px_4()
                            .py_2()
                            .gap_2()
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(icon("boost", 16.).text_color(rgb(0xf47fff)))
                            .child(
                                label(format!(
                                    "{count} {}",
                                    if count == 1 { "boost" } else { "boosts" }
                                ))
                                .text_size(px(12.)),
                            )
                            .when_some(tier.filter(|tier| *tier > 0), |s, tier| {
                                s.child(label(format!("· Level {tier}")).text_size(px(12.)))
                            }),
                    )
                },
            )
            .child(
                row()
                    .mx_2()
                    .mt_2()
                    .px_2()
                    .rounded_md()
                    .bg(rgb(PANEL))
                    .gap_2()
                    .child(icon("search", 16.).text_color(rgb(MUTED)))
                    .child(self.search.clone()),
            )
            .child(if self.selected_guild.is_none() {
                let ids: Vec<_> = channels.iter().map(|channel| channel.id.clone()).collect();
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        uniform_list(
                            "direct-messages",
                            ids.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .filter_map(|index| this.store.channels.get(&ids[index]))
                                    .map(|channel| this.dm_row(channel, cx))
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .size_full(),
                    )
                    .into_any_element()
            } else {
                column()
                    .id("channels")
                    .flex_1()
                    .min_h_0()
                    .px_2()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(navigation)
                    .into_any_element()
            })
            .when(self.preview, |s| {
                s.child(
                    row()
                        .px_3()
                        .py_2()
                        .gap_2()
                        .bg(rgb(0x292b30))
                        .child(label("Preview workspace").flex_1().text_size(px(11.)))
                        .child(
                            button("connect-sidebar", "Log in")
                                .py_1()
                                .px_2()
                                .text_size(px(11.))
                                .bg(rgb(0x5865f2))
                                .on_click(cx.listener(|this, _, _, cx| this.open_login(cx))),
                        ),
                )
            })
            .child(
                row()
                    .relative()
                    .h(px(54.))
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
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_settings(1, window, cx)
                                }),
                            ),
                    ),
            )
    }

    fn open_profile(&mut self, user: User, window: &mut Window, cx: &mut Context<Self>) {
        self.previous_focus = window.focused(cx);
        window.focus(&self.modal_focus);
        self.profile_error = None;
        self.profile_request += 1;
        self.profile_started = std::time::Instant::now();
        self.profile_guild = self
            .selected_channel
            .as_ref()
            .and_then(|id| self.store.channels.get(id))
            .and_then(|c| c.guild_id.clone());
        let key = (user.id.clone(), self.profile_guild.clone());
        let cached = self
            .profiles
            .get(&key)
            .filter(|(_, at)| at.elapsed() < std::time::Duration::from_secs(300))
            .map(|(p, _)| p.clone());
        let member = self
            .selected_channel
            .as_deref()
            .and_then(|id| {
                self.store
                    .channel_messages(id)
                    .iter()
                    .rev()
                    .find(|m| m.author.id == user.id)
            })
            .and_then(|m| m.member.clone());
        if !self.preview && cached.is_none() {
            self.command(Command::Profile {
                id: user.id.clone(),
                guild: self.profile_guild.clone(),
                request: self.profile_request,
                roles_needed: self
                    .profile_guild
                    .as_ref()
                    .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id))
                    .is_some_and(|g| g.roles.is_empty()),
            });
        }
        self.profile = Some(cached.unwrap_or(Profile {
            user,
            guild_member: member,
            ..Profile::default()
        }));
        cx.notify();
    }

    fn avatar(&self, user: &User, size: f32) -> AnyElement {
        self.decorated_avatar(user, None, None, size, false)
    }

    fn media_image(&self, url: &str, width: Option<u32>, height: Option<u32>) -> AnyElement {
        let ratio = width
            .zip(height)
            .filter(|(w, h)| *w > 0 && *h > 0)
            .map_or(1.5, |(w, h)| w as f32 / h as f32)
            .clamp(0.25, 8.);
        let w = 400_f32.min(280. * ratio);
        let h = w / ratio;
        let image = self.media.get(url);
        div()
            .w(px(w))
            .max_w_full()
            .h(px(h))
            .rounded_lg()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .map(|s| match image {
                Some(image) => s.child(
                    img(image)
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .rounded_lg(),
                ),
                None => s
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(label("Image")),
            })
            .into_any_element()
    }

    fn message(&self, message: &Message, grouped: bool, cx: &mut Context<Self>) -> AnyElement {
        let time = chrono::DateTime::parse_from_rfc3339(&message.timestamp)
            .ok()
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default();
        let mut content = message.content.clone();
        for user in &message.mentions {
            content = content
                .replace(&format!("<@{}>", user.id), &format!("@{}", user.name()))
                .replace(&format!("<@!{}>", user.id), &format!("@{}", user.name()));
        }
        let user = message.author.clone();
        let avatar_user = user.clone();
        let mut body = column().flex_1().min_w_0().gap_1();
        if !grouped {
            body = body.child(
                row()
                    .gap_3()
                    .child(
                        div()
                            .id("author")
                            .cursor_pointer()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.display_name(
                                message.name(),
                                &message.author,
                                message.member.as_ref(),
                                self.hovered_name.as_deref() == Some(&message.id),
                            ))
                            .on_hover({
                                let id = message.id.clone();
                                cx.listener(move |this, hovered: &bool, _, cx| {
                                    this.hovered_name = hovered.then(|| id.clone());
                                    cx.notify();
                                })
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_profile(user.clone(), window, cx)
                            })),
                    )
                    .when(message.author.bot, |s| s.child(badge("APP")))
                    .child(label(time).text_size(px(12.))),
            );
        }
        if !content.is_empty() {
            body = body.child(self.rich_text(&content));
        }
        if message.edited_timestamp.is_some() {
            body = body.child(label("edited").text_size(px(10.)));
        }
        for (index, attachment) in message
            .attachments
            .iter()
            .filter(|_| message.flags & 32768 == 0)
            .take(10)
            .enumerate()
        {
            let url = attachment.url.clone();
            let source = attachment
                .proxy_url
                .as_deref()
                .filter(|u| crate::media::cdn_url(u))
                .unwrap_or(&url);
            let is_video =
                crate::video::video_type(attachment.content_type.as_deref(), &attachment.filename);
            if is_video
                && crate::video::source(source).is_some()
                && !attachment.filename.starts_with("SPOILER_")
            {
                body = body.child(self.video_card(
                    source,
                    &attachment.filename,
                    None,
                    attachment.width.zip(attachment.height),
                    cx,
                ));
            }
            let is_image = !is_video
                && (attachment
                    .content_type
                    .as_deref()
                    .is_some_and(|t| t.starts_with("image/"))
                    || (attachment.width.is_some() && attachment.height.is_some()));
            if is_image
                && crate::media::cdn_url(source)
                && !attachment.filename.starts_with("SPOILER_")
            {
                body = body.child(self.media_image(source, attachment.width, attachment.height));
            }
            body = body.child(
                button(("attachment", index), "↗")
                    .max_w_full()
                    .text_size(px(11.))
                    .py_1()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(attachment.filename.clone()),
                    )
                    .on_click(move |_, _, cx| {
                        open_https(&url, cx);
                    }),
            );
        }
        for (index, embed) in message.embeds.iter().take(10).enumerate() {
            let playable = embed.video.as_ref().and_then(|video| {
                video
                    .proxy_url
                    .as_deref()
                    .or(video.url.as_deref())
                    .filter(|url| crate::video::source(url).is_some())
                    .map(|url| (url, video))
            });
            if let Some((url, video)) = playable
                && embed.description.is_none()
                && embed.fields.is_empty()
                && embed.author.is_none()
                && embed.footer.is_none()
            {
                body = body.child(self.video_card(
                    url,
                    embed.title.as_deref().unwrap_or_default(),
                    embed.thumbnail.as_ref().or(embed.image.as_ref()),
                    video.width.zip(video.height),
                    cx,
                ));
                continue;
            }
            let image_only = embed.title.is_none()
                && embed.description.is_none()
                && embed.fields.is_empty()
                && embed.author.is_none()
                && embed.footer.is_none();
            let image_width = embed
                .image
                .as_ref()
                .and_then(|m| m.width.zip(m.height))
                .filter(|(w, h)| *h > 0 && *w > 0)
                .map(|(w, h)| 400_f32.min(280. * w as f32 / h as f32) + 28.)
                .unwrap_or(430.);
            let mut card = column()
                .id(("embed", index))
                .w(px(if image_only { image_width } else { 430. }))
                .max_w_full()
                .min_w_0()
                .p_3()
                .gap_2()
                .rounded_md()
                .bg(rgb(SIDEBAR))
                .border_l_4()
                .border_color(rgb(embed.color.unwrap_or(0x5865f2) & 0xffffff));
            if let Some(name) = embed.author.as_ref().and_then(|a| a.name.clone()) {
                card = card.child(label(name));
            }
            if let Some(title) = &embed.title {
                let url = embed.url.clone();
                card = card.child(
                    div()
                        .id(("embed-title", index))
                        .text_color(rgb(0x65b8f7))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .child(title.clone())
                        .on_click(move |_, _, cx| {
                            if let Some(url) = &url {
                                open_https(url, cx);
                            }
                        }),
                );
            }
            if let Some(description) = &embed.description {
                card = card.child(self.rich_text(description));
            }
            for (field_index, field) in embed.fields.iter().take(25).enumerate() {
                card = card.child(
                    column()
                        .id(("embed-field", field_index))
                        .gap_1()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(field.name.clone()),
                        )
                        .child(self.rich_text(&field.value)),
                );
            }
            for media in [embed.image.as_ref(), embed.thumbnail.as_ref()]
                .map(|media| media.filter(|_| playable.is_none()))
                .into_iter()
                .flatten()
            {
                if let Some(url) = media
                    .proxy_url
                    .as_deref()
                    .or(media.url.as_deref())
                    .filter(|u| crate::media::cdn_url(u))
                {
                    card = card.child(self.media_image(url, media.width, media.height));
                }
            }
            if let Some((url, video)) = playable {
                card = card.child(self.video_card(
                    url,
                    "",
                    embed.thumbnail.as_ref().or(embed.image.as_ref()),
                    video.width.zip(video.height),
                    cx,
                ));
            }
            if let Some(footer) = embed.footer.as_ref().and_then(|f| f.text.clone()) {
                card = card.child(label(footer).text_size(px(10.)));
            }
            body = body.child(card);
        }
        for (index, component) in message.components.iter().take(40).enumerate() {
            body = body.child(self.component(message, component, index.to_string(), 0, cx));
        }
        if content.is_empty()
            && message.attachments.is_empty()
            && message.embeds.is_empty()
            && message.components.is_empty()
        {
            body = body.child(label(if message.author.bot {
                "This bot response has no supported content yet."
            } else {
                "Message content is unavailable."
            }));
        }
        row()
            .id(SharedString::from(format!("message-{}", message.id)))
            .hover(|s| s.bg(rgb(0x17181b)))
            .text_size(px(16.))
            .line_height(px(22.))
            .items_start()
            .gap_3()
            .px_4()
            .pt(px(if grouped {
                1.
            } else if self.settings.compact {
                6.
            } else {
                12.
            }))
            .pb(px(if grouped { 1. } else { 4. }))
            .w_full()
            .child(div().w(px(40.)).flex_shrink_0().when(!grouped, |s| {
                s.child(
                    div()
                        .id("avatar")
                        .cursor_pointer()
                        .child(self.decorated_avatar(
                            &message.author,
                            message.member.as_ref(),
                            self.selected_guild.as_deref(),
                            40.,
                            self.hovered_name.as_deref() == Some(&message.id),
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile(avatar_user.clone(), window, cx)
                        })),
                )
            }))
            .child(body)
            .into_any_element()
    }

    fn conversation(&self, cx: &mut Context<Self>) -> AnyElement {
        let channel = self
            .selected_channel
            .as_ref()
            .and_then(|id| self.store.channels.get(id));
        let title = channel.map_or_else(|| "Your little corner".into(), Channel::label);
        let topic = channel.and_then(|c| c.topic.clone()).unwrap_or_default();
        let weak = cx.weak_entity();
        let selected = self.selected_channel.clone();
        column()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                row()
                    .h(px(48.))
                    .flex_shrink_0()
                    .px_4()
                    .gap_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(icon(if channel.is_some_and(|c|c.guild_id.is_none()) {"user"} else {"hash"},24.).text_color(rgb(MUTED)))
                    .child(div().flex_shrink_0().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .when(!topic.is_empty(),|s|s.child(div().w(px(1.)).h(px(24.)).bg(rgb(LINE))).child(label(topic).truncate().text_size(px(14.))))
                    .child(div().flex_1())
                    .when_some(channel.and_then(|c|c.recipients.first()).cloned(),|s,user|s.child(button("conversation-profile", "").p_2().child(icon("user",20.)).tooltip(|_,cx|cx.new(|_|Hint("View profile".into())).into()).on_click(cx.listener(move|this,_,window,cx|this.open_profile(user.clone(),window,cx)))))
                    .child(button("conversation-search", "").p_2().child(icon("search",20.)).tooltip(|_,cx|cx.new(|_|Hint("Find a conversation".into())).into()).on_click(cx.listener(|this,_,window,cx|window.focus(&this.search.focus_handle(cx)))))
                    .when(self.preview, |s| s.child(badge("PREVIEW"))),
            )
            .when(channel.is_none(), |s| {
                s.when_some(self.error.clone(), |s, error| s.child(
                    div().px_6().py_3().bg(rgb(0x3b2c30)).text_color(rgb(0xf0b7c3)).child(error)
                )).child(
                    column()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(
                            img(ImageSource::Resource(Resource::Embedded(
                                "mascot.png".into(),
                            )))
                            .size(px(70.))
                            .text_color(rgb(ACCENT)),
                        )
                        .child(div().text_size(px(24.)).child("A little room to breathe."))
                        .child(label("Choose a conversation from the sidebar.")),
                )
            })
            .when(channel.is_some(), |s| {
                s.child(
                    list(self.messages.clone(), move |index, _, cx| {
                        let Some(view) = weak.upgrade() else {
                            return div().into_any_element();
                        };
                        view.update(cx, |view, cx| {
                            let message = view
                                .selected_channel
                                .as_deref()
                                .and_then(|id| view.store.channel_messages(id).get(index))
                                .cloned();
                            let grouped = view
                                .selected_channel
                                .as_deref()
                                .and_then(|id| {
                                    view.store.channel_messages(id).get(index.wrapping_sub(1))
                                })
                                .zip(message.as_ref())
                                .is_some_and(|(previous, message)| message.continues(previous));
                            message.map_or_else(
                                || div().into_any_element(),
                                |message| view.message(&message, grouped, cx),
                            )
                        })
                    })
                    .flex_1()
                    .min_h_0(),
                )
                .when(
                    self.scrolled_away
                        || selected
                            .as_ref()
                            .and_then(|id| self.pages.get(id))
                            .is_some_and(|p| p.newer),
                    |s| {
                        s.child(
                            row().justify_center().py_1().child(
                                button("jump-latest", "↓ Jump to latest")
                                    .rounded_full()
                                    .bg(rgb(0x5865f2))
                                    .on_click(cx.listener(|this, _, _, cx| this.jump_latest(cx))),
                            ),
                        )
                    },
                )
                .when(
                    selected
                        .as_ref()
                        .is_some_and(|id| self.loading.contains_key(id)),
                    |s| s.child(label("Loading messages…").px_6()),
                )
                .child(
                    column()
                        .p_4()
                        .pt_2()
                        .gap_2()
                        .when_some(self.error.clone(), |s, error| {
                            s.child(
                                row()
                                    .p_3()
                                    .gap_3()
                                    .rounded_lg()
                                    .bg(rgb(0x3b2c30))
                                    .text_color(rgb(0xf0b7c3))
                                    .text_size(px(12.))
                                    .child(div().flex_1().child(error))
                                    .child(button("dismiss-error", "×").py_0().px_2().on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.error = None;
                                            cx.notify();
                                        }),
                                    )),
                            )
                        })
                        .child(self.typing_indicator())
                        .when_some(self.failed_send.clone(), |s, (channel, content)| {
                            s.child(
                                column()
                                    .p_3()
                                    .gap_2()
                                    .rounded_lg()
                                    .bg(rgb(PANEL))
                                    .child(label(format!(
                                        "Unsent: {}",
                                        content.chars().take(160).collect::<String>()
                                    )))
                                    .child(
                                        row()
                                            .gap_2()
                                            .child(
                                                button("recover-send", "Copy unsent text")
                                                    .on_click(move |_, _, cx| {
                                                        cx.write_to_clipboard(
                                                            ClipboardItem::new_string(
                                                                content.clone(),
                                                            ),
                                                        )
                                                    }),
                                            )
                                            .child(button("dismiss-unsent", "Dismiss").on_click(
                                                cx.listener(move |this, _, _, cx| {
                                                    if this
                                                        .failed_send
                                                        .as_ref()
                                                        .is_some_and(|(id, _)| *id == channel)
                                                    {
                                                        this.failed_send = None;
                                                    }
                                                    cx.notify();
                                                }),
                                            )),
                                    ),
                            )
                        })
                        .child(
                            row()
                                .min_h(px(50.))
                                .px_3()
                                .gap_2()
                                .rounded_xl()
                                .border_1()
                                .border_color(rgb(LINE))
                                .bg(rgb(SURFACE))
                                .child(div().flex_1().min_w_0().child(self.composer.clone()))
                                .child(
                                    button(
                                        "send",
                                        if self.sending.is_some() {
                                            "Sending…"
                                        } else {
                                            "Send ↗"
                                        },
                                    )
                                    .bg(rgb(0x5865f2))
                                    .text_color(rgb(ACCENT))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.send(window, cx)),
                                    ),
                                ),
                        )
                        .child(
                            row().justify_between().px_1().child(
                                label(if self.preview {
                                    "Preview messages stay in this window."
                                } else {
                                    "Enter to send · Shift+Enter for a new line · mentions do not ping"
                                })
                                .text_size(px(10.)),
                            ),
                        ),
                )
            })
            .into_any_element()
    }

    fn login_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        let qr_area = match &self.qr {
            QrState::Ready(code) => {
                let code = code.clone();
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        window.paint_quad(fill(bounds, rgb(0xffffff)));
                        let module =
                            px((f32::from(bounds.size.width) / (code.side + 8) as f32).floor());
                        let extent = module * (code.side + 8) as f32;
                        let origin = point(
                            bounds.origin.x + (bounds.size.width - extent) / 2.,
                            bounds.origin.y + (bounds.size.height - extent) / 2.,
                        );
                        for y in 0..code.side {
                            for x in 0..code.side {
                                if code.dark[y * code.side + x] {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(
                                                origin.x + module * (x + 4) as f32,
                                                origin.y + module * (y + 4) as f32,
                                            ),
                                            size(module, module),
                                        ),
                                        rgb(0x000000),
                                    ));
                                }
                            }
                        }
                    },
                )
                .size(px(200.))
                .into_any_element()
            }
            _ => column()
                .size(px(200.))
                .p_4()
                .items_center()
                .justify_center()
                .gap_3()
                .rounded_lg()
                .bg(rgb(SURFACE))
                .child(
                    img(ImageSource::Resource(Resource::Embedded(
                        "mascot.png".into(),
                    )))
                    .size(px(52.)),
                )
                .child(
                    label(match &self.qr {
                        QrState::Confirm => "Confirm on your phone",
                        QrState::Failed(_) => "Code unavailable",
                        QrState::Generating => "Generating a secure code…",
                        _ => "Ready to connect",
                    })
                    .text_center(),
                )
                .into_any_element(),
        };
        let mut instructions = column()
            .w(px(340.))
            .min_w_0()
            .flex_shrink_0()
            .gap_4()
            .child(div().font_weight(FontWeight::BOLD).text_size(px(26.)).child("Welcome back!"))
            .child(
                div().text_size(px(15.)).text_color(rgb(MUTED)).line_height(px(24.))
                    .child("Sign in with the Discord mobile app. Open your settings, choose Scan QR Code, and confirm the login on your phone."),
            )
            .child(
                label("The code creates a session for this client. It expires automatically and is cleared when you leave this screen.")
                    .line_height(px(20.)),
            );
        if let Some(error) = &self.error {
            instructions = instructions.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xf0b7c3))
                    .child(error.clone()),
            );
        }
        if matches!(self.qr, QrState::Failed(_)) {
            instructions = instructions.child(
                button("retry-qr", "Generate a new code")
                    .on_click(cx.listener(|this, _, _, cx| this.open_login(cx))),
            );
        }
        instructions = instructions
            .child(
                button("explore-preview", "Explore the preview")
                    .on_click(cx.listener(|this, _, _, cx| this.close_login(cx))),
            )
            .child(
                div()
                    .id("advanced-login")
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .cursor_pointer()
                    .child("Advanced sign-in")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_login(cx);
                        this.open_settings(0, window, cx);
                    })),
            );
        let mut qr_panel = column()
            .w(px(220.))
            .min_w_0()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .child(qr_area)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(17.))
                    .child("Log in with QR Code"),
            )
            .child(
                label(match &self.qr {
                    QrState::Confirm => "Scanned. Finish signing in on your phone.",
                    _ => "Scan with the Discord mobile app.",
                })
                .text_center(),
            );
        if let QrState::Failed(error) = &self.qr {
            qr_panel = qr_panel.child(label(error.clone()).text_center().text_color(rgb(0xf0b7c3)));
        }
        column()
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgb(PANEL))
            .items_center()
            .justify_center()
            .gap_6()
            .child(
                row()
                    .gap_3()
                    .child(
                        img(ImageSource::Resource(Resource::Embedded(
                            "mascot.png".into(),
                        )))
                        .size(px(42.)),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(22.))
                            .child("larpcord"),
                    ),
            )
            .child(
                row()
                    .w(px(700.))
                    .p_8()
                    .gap_8()
                    .rounded_lg()
                    .bg(rgb(BG))
                    .shadow_lg()
                    .child(instructions)
                    .child(qr_panel),
            )
            .into_any_element()
    }

    fn setting_toggle(
        &self,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        enabled: bool,
    ) -> Stateful<Div> {
        row()
            .id(id)
            .py_4()
            .gap_4()
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor_pointer()
            .child(
                column()
                    .flex_1()
                    .gap_1()
                    .child(div().text_size(px(14.)).child(title))
                    .child(label(description).line_height(px(18.))),
            )
            .child(
                row()
                    .w(px(36.))
                    .h(px(22.))
                    .flex_shrink_0()
                    .rounded_full()
                    .px(px(3.))
                    .bg(rgb(if enabled { 0x5865f2 } else { LINE }))
                    .when(enabled, |s| s.justify_end())
                    .child(div().size(px(16.)).rounded_full().bg(rgb(if enabled {
                        ACCENT
                    } else {
                        MUTED
                    }))),
            )
    }

    fn settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.settings_tab.unwrap_or(0);
        let mut body = column().gap_4();
        match tab {
            0 => {
                body = body
                    .child(div().text_size(px(26.)).child("Your account"))
                    .child(label("Connect directly to your Discord conversations."));
                if self.preview || self.status == "Disconnected" {
                    body = body.child(column().mt_4().p_4().rounded_xl().border_1().border_color(rgb(LINE)).bg(rgb(SURFACE)).gap_3()
                        .child(div().text_size(px(13.)).child("Discord session token"))
                        .child(row().px_2().rounded_lg().bg(rgb(PANEL)).child(self.token.clone()))
                        .child(label("Your token is a password. It is sent only to Discord. Larpcord stores remembered sessions in your OS credential store.").line_height(px(19.)))
                        .child(button("login", "Connect account").bg(rgb(0x5865f2)).text_color(rgb(ACCENT)).on_click(cx.listener(|this, _, window, cx| this.login(window, cx)))))
                        .child(label("This is an early, independent Discord client. Voice, quests and existing JavaScript mods are still pending."));
                } else {
                    body = body.child(
                        column()
                            .p_4()
                            .rounded_xl()
                            .bg(rgb(SURFACE))
                            .gap_3()
                            .child(div().child(self.store.user.name().to_string()))
                            .child(label(self.status.clone()))
                            .child(button("own-profile", "View profile").on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.close_settings(window, cx);
                                    this.open_profile(this.store.user.clone(), window, cx);
                                },
                            )))
                            .child(
                                button("sign-out", "Disconnect and forget session")
                                    .on_click(cx.listener(|this, _, _, cx| this.disconnect(cx))),
                            ),
                    );
                }
                body = body.child(
                    self.setting_toggle(
                        "remember",
                        "Remember this session",
                        "Save future logins in the operating system credential store.",
                        self.settings.remember_session,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings.remember_session = !this.settings.remember_session;
                        if !this.settings.remember_session {
                            this.command(Command::Forget);
                        }
                        this.persist(cx);
                    })),
                );
            }
            1 => {
                body = body
                    .child(div().text_size(px(26.)).child("Make it yours"))
                    .child(label("A few useful choices. Sensible defaults."))
                    .child(
                        self.setting_toggle(
                            "notifications",
                            "Desktop notifications",
                            "Notify for direct messages, mentions and incoming call signals.",
                            self.settings.notifications,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.notifications = !this.settings.notifications;
                            this.persist(cx);
                        })),
                    )
                    .child(
                        self.setting_toggle(
                            "preview-text",
                            "Show message previews",
                            "Include message text in notifications. Off by default for privacy.",
                            self.settings.notification_preview,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.notification_preview =
                                !this.settings.notification_preview;
                            this.persist(cx);
                        })),
                    )
                    .child(
                        self.setting_toggle(
                            "keep-connected",
                            "Keep connected when closing",
                            "Minimize the window and keep receiving messages. Use Quit to exit.",
                            self.settings.keep_connected,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.keep_connected = !this.settings.keep_connected;
                            this.persist(cx);
                        })),
                    )
                    .child(
                        self.setting_toggle(
                            "compact",
                            "Compact conversations",
                            "Give each message a little less space.",
                            self.settings.compact,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.compact = !this.settings.compact;
                            this.refresh_messages();
                            this.persist(cx);
                        })),
                    ).child(self.setting_toggle("animations","Interface animations","Short transitions for panels and selection. Turn off to reduce motion.",self.settings.animations).on_click(cx.listener(|this,_,_,cx|{this.settings.animations = !this.settings.animations;this.persist(cx);})));
            }
            _ => {
                body = body.child(img(ImageSource::Resource(Resource::Embedded("mascot.png".into()))).size(px(58.)).text_color(rgb(ACCENT)))
                    .child(div().text_size(px(30.)).child("larpcord"))
                    .child(label(format!("{} • development preview", env!("CARGO_PKG_VERSION"))))
                    .child(div().text_size(px(16.)).child("A little room to breathe."))
                    .child(label("Built in Rust with GPUI. Inspired by Kawaicord's small-footprint approach."))
                    .child(column().mt_4().p_4().rounded_xl().bg(rgb(SURFACE)).gap_3()
                        .child(div().child(format!("{} messages cached", self.store.cached_messages())))
                        .child(label("Up to 12 conversations and 200 messages per conversation. Images load as you scroll. The connection runs independently of rendering."))
                        .child(button("clear-cache", "Clear message cache").on_click(cx.listener(|this, _, _, cx| { this.store.clear_history(); this.refresh_messages(); cx.notify(); }))))
                    .child(button("github", "Open GitHub ↗").on_click(|_, _, cx| cx.open_url("https://github.com/evenafterallisaid/larpcord")))
                    .child(label("Native voice and plugin adapters are future milestones. Video codec support depends on your platform and installed graphics drivers."));
                if self.preview_only {
                    body = body.child(button("sample-video", "Try sample video").on_click(
                        cx.listener(|this, _, window, cx| this.sample_video(window, cx)),
                    ));
                }
            }
        }
        column()
            .id("settings-modal")
            .track_focus(&self.modal_focus)
            .occlude()
            .absolute()
            .inset_0()
            .top(px(32.))
            .bg(rgba(0x00000099))
            .items_center()
            .justify_center()
            .child(
                row()
                    .w(relative(0.9))
                    .max_w(px(960.))
                    .h(relative(0.9))
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(0x46474f))
                    .shadow_lg()
                    .overflow_hidden()
                    .bg(rgb(BG))
                    .child(
                        column()
                            .w(px(240.))
                            .h_full()
                            .p_4()
                            .gap_2()
                            .bg(rgb(PANEL))
                            .child(label("LARPCORD").mt_2().mb_4().text_size(px(10.)))
                            .children(
                                ["Account", "Preferences", "About"]
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, name)| {
                                        row()
                                            .id(("settings-tab", index))
                                            .px_3()
                                            .py_2()
                                            .rounded_lg()
                                            .cursor_pointer()
                                            .bg(rgb(if tab == index { SURFACE } else { PANEL }))
                                            .text_color(rgb(if tab == index {
                                                ACCENT
                                            } else {
                                                MUTED
                                            }))
                                            .child(name)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.settings_tab = Some(index);
                                                cx.notify();
                                            }))
                                    }),
                            )
                            .child(div().flex_1())
                            .child(label("small client,\nsoft landing.").line_height(px(20.))),
                    )
                    .child(
                        column()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .child(row().px_6().pt_4().justify_end().child(
                                button("close-settings", "×").px_2().on_click(cx.listener(
                                    |this, _, window, cx| this.close_settings(window, cx),
                                )),
                            ))
                            .child(
                                column()
                                    .id("settings-body")
                                    .flex_1()
                                    .min_h_0()
                                    .px_6()
                                    .pb_6()
                                    .overflow_y_scroll()
                                    .child(body)
                                    .when_some(self.error.clone(), |s, error| {
                                        s.child(
                                            div()
                                                .mt_3()
                                                .text_size(px(12.))
                                                .text_color(rgb(0xf0b7c3))
                                                .child(error),
                                        )
                                    }),
                            ),
                    ),
            )
            .map(|element| fade_in(element, "settings-enter", self.settings.animations))
    }
}

impl Render for Larpcord {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.media.begin_frame();
        column()
            .relative()
            .track_focus(&self.root_focus)
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(14.))
            .font_family("Inter")
            .on_action(|_: &Quit, _, cx| cx.quit())
            .on_action(
                cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ClosePanel, window, cx| {
                if this.video.is_some() {
                    this.close_video(window, cx);
                } else if this.component_menu.take().is_some() || this.profile.take().is_some() {
                    if let Some(focus) = this.previous_focus.take() {
                        window.focus(&focus);
                    }
                    cx.notify();
                } else if this.login_open {
                    this.close_login(cx);
                } else if this.settings_tab.is_some() {
                    this.close_settings(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                if this.settings_tab.is_none()
                    && !this.login_open
                    && this.profile.is_none()
                    && this.component_menu.is_none()
                    && this.video.is_none()
                {
                    window.focus(&this.search.focus_handle(cx));
                }
            }))
            .child(self.titlebar(cx))
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(self.rail(cx))
                    .child(self.sidebar(cx))
                    .child(self.conversation(cx)),
            )
            .when(self.settings_tab.is_some(), |s| s.child(self.settings(cx)))
            .when(self.login_open, |s| s.child(self.login_screen(cx)))
            .when_some(self.profile.clone(), |s, user| {
                s.child(self.profile_panel(&user, cx))
            })
            .when(self.video.is_some(), |s| s.child(self.video_panel(cx)))
            .when_some(self.component_menu.clone(), |s, (message, component)| {
                s.child(self.component_menu_panel(&message, &component, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Larpcord, Message, Page};
    use crate::network::{Envelope, Event};
    use gpui::{Context, Focusable, TestAppContext, Window};

    #[gpui::test]
    fn partial_user_update_preserves_the_avatar_and_other_cosmetics(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window.update(cx,|view,window,cx| {
            view.store.user.avatar=Some("abc".into());
            let id=view.store.user.id.clone();let name=view.store.user.username.clone();
            view.dispatch("USER_UPDATE",serde_json::json!({"id":id,"display_name_styles":{"font_id":3,"colors":[16711680]}}),window,cx);
            assert_eq!(view.store.user.avatar.as_deref(),Some("abc"));assert_eq!(view.store.user.username,name);
            assert_eq!(view.store.user.display_name_styles.as_ref().unwrap().font(),"Cherry Bomb One");
        }).unwrap();
    }

    #[gpui::test]
    fn profile_avatar_overlaps_the_banner_without_collapsing_the_card(cx: &mut TestAppContext) {
        use gpui::{point, px, size};
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |view, cx| {
            view.profile = Some(crate::model::Profile {
                user: view.store.user.clone(),
                ..Default::default()
            });
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let card = cx.debug_bounds("profile-card").unwrap();
        let banner = cx.debug_bounds("profile-banner").unwrap();
        let avatar = cx.debug_bounds("profile-avatar").unwrap();
        assert!(card.size.height >= px(450.), "card collapsed: {card:?}");
        assert!(
            avatar.origin.y < banner.bottom() && avatar.bottom() > banner.bottom(),
            "avatar must overlap the banner boundary"
        );
        assert_eq!(avatar.size.width, px(96.));
    }

    #[gpui::test]
    fn clicking_a_video_tile_opens_the_native_player(cx: &mut TestAppContext) {
        use gpui::{Modifiers, point, px, size};
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let bounds = cx
            .debug_bounds("video-tile")
            .expect("sample tile was painted");
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.update(|_, cx| {
            assert!(
                view.read(cx).video.is_some(),
                "clicking the media tile must open playback"
            )
        });
    }

    #[gpui::test]
    fn server_metadata_cannot_overwrite_a_newer_gateway_update(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window.update(cx,|view,window,cx| {
            let started=std::time::Instant::now();
            view.guild_details_attempted.insert("larp".into(),started);
            view.add_guild(&serde_json::json!({"id":"larp","banner":"abc","premium_subscription_count":14,"premium_tier":3}));
            view.receive(Envelope {generation:0,event:Event::GuildDetails {id:"larp".into(),started,result:Ok(crate::model::Guild {id:"larp".into(),banner:Some("def".into()),premium_subscription_count:Some(8),..Default::default()})}},window,cx);
            let guild=&view.store.guilds[0];
            assert_eq!(guild.banner.as_deref(),Some("abc"));
            assert_eq!(guild.premium_subscription_count,Some(14));
            assert_eq!(guild.premium_tier,Some(3));
        }).unwrap();
    }

    #[gpui::test]
    fn nested_help_text_keeps_a_readable_layout_width(cx: &mut TestAppContext) {
        use gpui::{point, px, size};
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let bounds = cx.debug_bounds("rich-text").expect("help text was painted");
        assert!(
            bounds.size.width > px(150.),
            "nested help text collapsed to {:?}",
            bounds
        );
    }

    #[gpui::test]
    fn typing_after_submission_survives_delayed_acknowledgement(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.preview = false;
                view.status = "Connected".into();
                view.composer.update(cx, |input, cx| input.set("thats", cx));
                view.send(window, cx);
                let nonce = view.sending.as_ref().unwrap().0.clone();
                assert!(view.composer.read(cx).content.is_empty());
                view.composer.update(cx, |input, cx| input.set("crazy", cx));
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::Sent {
                            nonce,
                            result: Ok(Box::new(Message {
                                id: "100".into(),
                                channel_id: "campfire".into(),
                                content: "thats".into(),
                                ..Message::default()
                            })),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(view.composer.read(cx).content.as_ref(), "crazy");
                assert!(view.sending.is_none());
            })
            .unwrap();
    }

    #[gpui::test]
    fn send_failure_preserves_both_submitted_text_and_followup_draft(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.preview = false;
                view.status = "Connected".into();
                view.composer.update(cx, |input, cx| input.set("first", cx));
                view.send(window, cx);
                let nonce = view.sending.as_ref().unwrap().0.clone();
                view.composer
                    .update(cx, |input, cx| input.set("second", cx));
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::Sent {
                            nonce,
                            result: Err("connection interrupted".into()),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(view.composer.read(cx).content.as_ref(), "second");
                assert_eq!(view.failed_send.as_ref().unwrap().1, "first");
            })
            .unwrap();
    }

    #[gpui::test]
    fn paging_forward_reenables_older_history_and_jump_ignores_a_stale_page(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.preview = false;
                view.store.clear_history();
                let respond = |view: &mut Larpcord,
                               page,
                               range: std::ops::Range<usize>,
                               window: &mut Window,
                               cx: &mut Context<Larpcord>| {
                    view.load_history(page);
                    let request = view.loading["campfire"].0;
                    let messages = range
                        .map(|id| Message {
                            id: id.to_string(),
                            channel_id: "campfire".into(),
                            ..Message::default()
                        })
                        .collect();
                    view.receive(
                        Envelope {
                            generation: 0,
                            event: Event::History {
                                channel: "campfire".into(),
                                started: 0,
                                request,
                                page,
                                result: Ok(messages),
                            },
                        },
                        window,
                        cx,
                    );
                };
                respond(view, Page::Latest, 200..300, window, cx);
                respond(view, Page::Before, 100..200, window, cx);
                respond(view, Page::Before, 0..100, window, cx);
                respond(view, Page::Before, 0..0, window, cx);
                assert!(view.pages["campfire"].no_older);
                respond(view, Page::After, 200..300, window, cx);
                assert!(!view.pages["campfire"].no_older);
                view.load_history(Page::Before);
                let stale = view.loading["campfire"].0;
                view.jump_latest(cx);
                assert_eq!(view.loading["campfire"].1, Page::Latest);
                assert_ne!(view.loading["campfire"].0, stale);
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::History {
                            channel: "campfire".into(),
                            started: 0,
                            request: stale,
                            page: Page::Before,
                            result: Ok(vec![Message {
                                id: "0".into(),
                                channel_id: "campfire".into(),
                                ..Message::default()
                            }]),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    view.store.channel_messages("campfire").first().unwrap().id,
                    "100"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn late_profile_responses_cannot_replace_a_different_person_or_guild(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.open_profile(
                    crate::model::User {
                        id: "1".into(),
                        username: "first".into(),
                        ..crate::model::User::default()
                    },
                    window,
                    cx,
                );
                let first = view.profile_request;
                let guild = view.profile_guild.clone();
                view.open_profile(
                    crate::model::User {
                        id: "2".into(),
                        username: "second".into(),
                        ..crate::model::User::default()
                    },
                    window,
                    cx,
                );
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::Profile {
                            id: "1".into(),
                            guild,
                            request: first,
                            result: Ok(Box::new(crate::model::Profile {
                                user: crate::model::User {
                                    id: "1".into(),
                                    username: "first".into(),
                                    ..crate::model::User::default()
                                },
                                ..crate::model::Profile::default()
                            })),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(view.profile.as_ref().unwrap().user.id, "2");
                let request = view.profile_request;
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::Profile {
                            id: "2".into(),
                            guild: Some("another-guild".into()),
                            request,
                            result: Ok(Box::new(crate::model::Profile {
                                user: crate::model::User {
                                    id: "2".into(),
                                    username: "wrong context".into(),
                                    ..crate::model::User::default()
                                },
                                ..crate::model::Profile::default()
                            })),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(view.profile.as_ref().unwrap().user.username, "second");
            })
            .unwrap();
    }

    #[gpui::test]
    fn typing_is_scoped_to_the_view_and_stops_when_a_message_arrives(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window.update(cx,|view,window,cx| {
            view.dispatch("TYPING_START",serde_json::json!({"channel_id":"other","user_id":"1","member":{"user":{"id":"1","username":"friend"}}}),window,cx);
            assert!(view.typing.is_empty());
            view.dispatch("TYPING_START",serde_json::json!({"channel_id":"campfire","user_id":"1","member":{"user":{"id":"1","username":"friend"}}}),window,cx);
            assert_eq!(view.typing_label(),"friend is typing…");
            view.dispatch("MESSAGE_CREATE",serde_json::json!({"id":"900","channel_id":"campfire","author":{"id":"1","username":"friend"},"content":"hello"}),window,cx);
            assert!(view.typing.is_empty());
            view.choose_channel("build-log".into(),window,cx);
            assert!(view.typing_task.is_none());
        }).unwrap();
    }

    #[gpui::test]
    fn settings_moves_focus_blocks_submission_and_restores_the_draft(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.composer
                    .update(cx, |input, cx| input.set("keep me", cx));
                window.focus(&view.composer.focus_handle(cx));
                view.open_settings(1, window, cx);
                assert!(view.modal_focus.is_focused(window));
                let before = view.store.cached_messages();
                view.send(window, cx);
                assert_eq!(view.store.cached_messages(), before);
                assert_eq!(view.composer.read(cx).content.as_ref(), "keep me");
                view.close_settings(window, cx);
                assert!(view.composer.focus_handle(cx).is_focused(window));
                assert_eq!(view.composer.read(cx).content.as_ref(), "keep me");
            })
            .unwrap();
    }
}
