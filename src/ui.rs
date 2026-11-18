mod burst;
mod chrome;
mod components;
mod composer;
mod conversation;
mod cosmetics;
mod friends;
mod gif;
mod members;
mod modules;
mod panes;
mod picker;
mod profile;
mod profile_edit;
mod text;
mod video;

use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::icons::icon;
use chrome::toolbar_button;
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

const BG: u32 = 0x1b1c21;
const PANEL: u32 = 0x111216;
const SIDEBAR: u32 = 0x15161b;
const SURFACE: u32 = 0x282a32;
const LINE: u32 = 0x2d2f38;
const TEXT: u32 = 0xf2f3f5;
const MUTED: u32 = 0x9a9da7;
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
    modules: crate::modules::Registry,
    modules_loading: bool,
    modules_task: Option<Task<()>>,
    catalog_task: Option<Task<()>>,
    catalog: crate::modules::Catalog,
    catalog_notice: Option<(String, bool)>,
    catalog_discover: bool,
    catalog_search: Entity<TextInput>,
    generation: u64,
    preview: bool,
    preview_only: bool,
    window_active: bool,
    status: String,
    selected_guild: Option<String>,
    home_page: bool,
    friends_online: bool,
    friend_search: Entity<TextInput>,
    opening_dm: Option<String>,
    expanded_folders: HashSet<String>,
    guild_details_attempted: HashMap<String, std::time::Instant>,
    selected_channel: Option<String>,
    members: crate::members::Members,
    members_open: bool,
    member_views: crate::members::Views,
    navigation: crate::navigation::History,
    last_channels: HashMap<String, String>,
    reaction_replays: HashMap<String, u64>,
    collapsed_categories: HashSet<String>,
    composer: Entity<TextInput>,
    pane_resize_dirty: bool,
    emoji_search: Entity<TextInput>,
    emoji_open: bool,
    picker: crate::expressions::Picker,
    picker_debounce: Option<Task<()>>,
    picker_scroll: UniformListScrollHandle,
    picker_sections: ListState,
    selected_sticker: Option<crate::expressions::Sticker>,
    pending_sticker: Option<crate::expressions::Sticker>,
    failed_sticker: Option<crate::expressions::Sticker>,
    sticker_drafts: HashMap<String, crate::expressions::Sticker>,
    expression_attempts: HashSet<String>,
    reply_to: Option<composer::Reply>,
    pending_reply: Option<composer::Reply>,
    hovered_message: Option<String>,
    message_tools_top: Pixels,
    reaction_target: Option<Message>,
    reaction_pending: HashSet<(String, String)>,
    message_menu: Option<(Message, Point<Pixels>)>,
    search: Entity<TextInput>,
    token: Entity<TextInput>,
    messages: ListState,
    rendered_ids: Vec<String>,
    settings_tab: Option<usize>,
    profile_editor: Option<profile_edit::Editor>,
    error: Option<String>,
    loading: HashMap<String, (u64, Page)>,
    pages: HashMap<String, Paging>,
    page_request: u64,
    typing: HashMap<String, (String, std::time::Instant)>,
    typing_task: Option<Task<()>>,
    typing_styles: HashMap<String, crate::cosmetics::TypingStyle>,
    hovered_name: Option<String>,
    name_motion_origin: std::time::Instant,
    name_motion_task: std::cell::RefCell<Option<Task<()>>>,
    read_task: Option<Task<()>>,
    typing_sent: Option<(String, std::time::Instant)>,
    scrolled_away: bool,
    jump_task: Option<Task<()>>,
    jumping: bool,
    jump_after_latest: Option<String>,
    latest_opacity: f32,
    latest_target: bool,
    latest_fade: Option<Task<()>>,
    unread_marker: Option<(String, String)>,
    gif_pool: gif::SharedPool,
    banner_pool: gif::SharedPool,
    media: crate::media::Media,
    media_retry: Option<Task<()>>,
    profile: Option<Profile>,
    profile_inspector: bool,
    profile_tab: u8,
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
        .px_3()
        .py(px(6.))
        .rounded_md()
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
                Animation::new(std::time::Duration::from_millis(180)).with_easing(ease_out_quint()),
                |element, delta| element.opacity(delta),
            )
            .into_any_element()
    } else {
        element.into_any_element()
    }
}

fn snowflake_after(a: &str, b: &str) -> bool {
    a.len() > b.len() || (a.len() == b.len() && a > b)
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
    let (w, h) = dimensions
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((320, 180));
    let (w, h) = (w as f32, h as f32);
    let scale = (max_width / w).min(max_height / h).min(1.);
    ((w * scale).max(1.), (h * scale).max(1.))
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
    fn motion(&self) -> bool {
        self.settings.animations && self.window_active
    }
    fn color(&self, value: u32) -> Rgba {
        rgb(self
            .modules
            .tokens(self.settings.theme.tokens(self.settings.custom_theme))
            .resolve(value))
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_preview(
            window,
            cx,
            std::env::args().any(|arg| arg == "--preview")
                || std::env::var_os("LARPCORD_PREVIEW").is_some_and(|value| value == "1"),
        )
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
                        view.media_retry = view.media.retry_after().map(|delay| {
                            cx.spawn_in(window, async move |view, cx| {
                                cx.background_executor().timer(delay).await;
                                let _ = view.update(cx, |_, cx| cx.notify());
                            })
                        });
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
                        || view.profile_is_modal()
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
        let composer = cx.new(|cx| {
            let mut input = TextInput::new("Message the campfire", false, cx);
            input.inline_emojis = true;
            input
        });
        let emoji_search = cx.new(|cx| TextInput::new("Find the perfect emoji", false, cx));
        let search = cx.new(|cx| TextInput::new("Find a conversation", false, cx));
        let token = cx.new(|cx| TextInput::new("Paste a session token", true, cx));
        let catalog_search = cx.new(|cx| TextInput::new("Search the catalog", false, cx));
        let friend_search = cx.new(|cx| TextInput::new("Search friends", false, cx));
        let subscriptions = vec![
            cx.observe_window_activation(window, |this, window, cx| {
                gif::Pool::pause(&this.gif_pool, cx);
                gif::Pool::pause(&this.banner_pool, cx);
                this.mark_view_read(window, cx);
                cx.notify();
            }),
            cx.subscribe_in(&composer, window, |this, _, _: &Submit, window, cx| {
                this.send(window, cx)
            }),
            cx.subscribe_in(&token, window, |this, _, _: &Submit, window, cx| {
                this.login(window, cx)
            }),
            cx.subscribe(&emoji_search, |this, _, _: &Changed, cx| {
                this.picker_changed(cx);
                cx.notify();
            }),
            cx.subscribe(&catalog_search, |_, _, _: &Changed, cx| cx.notify()),
            cx.subscribe(&search, |_, _, _: &Changed, cx| cx.notify()),
            cx.subscribe(&friend_search, |_, _, _: &Changed, cx| cx.notify()),
            cx.subscribe_in(&composer, window, |this, _, _: &Changed, window, cx| {
                let input = this.composer.read(cx);
                if input.content.contains("<:")
                    || input.content.contains("<a:")
                    || !input.emoji_images.is_empty()
                {
                    cx.notify();
                }
                if this.composer.read(cx).content.is_empty()
                    || this.home_page
                    || this.preview
                    || this.settings_tab.is_some()
                    || this.login_open
                    || this.profile_is_modal()
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
        let settings = if preview_only {
            Settings::default()
        } else {
            Settings::load()
        };
        let picker = crate::expressions::Picker {
            recent: settings.recent_emoji.clone(),
            ..Default::default()
        };
        let modules = crate::modules::Registry::load_with_catalog(
            if preview_only {
                None
            } else {
                crate::modules::directory()
            }
            .as_deref(),
            &settings.module_grants,
            &settings.catalog_installed,
        );
        Self {
            modules,
            modules_loading: false,
            modules_task: None,
            catalog: crate::modules::Catalog::bundled(),
            catalog_task: None,
            catalog_notice: None,
            catalog_discover: true,
            catalog_search,
            root_focus,
            modal_focus: cx.focus_handle(),
            previous_focus: None,
            backend,
            store,
            settings,
            generation: 0,
            preview: true,
            preview_only,
            window_active: window.is_window_active(),
            status: "Preview".into(),
            home_page: false,
            friends_online: true,
            friend_search,
            opening_dm: None,
            selected_guild: Some("larp".into()),
            expanded_folders: HashSet::new(),
            guild_details_attempted: HashMap::new(),
            selected_channel: Some("campfire".into()),
            members: crate::members::Members::demo(),
            members_open: true,
            member_views: crate::members::Views::default(),
            navigation: crate::navigation::History::default(),
            last_channels: HashMap::new(),
            reaction_replays: HashMap::new(),
            collapsed_categories: HashSet::new(),
            composer,
            pane_resize_dirty: false,
            emoji_search,
            emoji_open: false,
            picker,
            picker_debounce: None,
            picker_scroll: UniformListScrollHandle::new(),
            picker_sections: ListState::new(0, ListAlignment::Top, px(80.)),
            selected_sticker: None,
            pending_sticker: None,
            failed_sticker: None,
            sticker_drafts: HashMap::new(),
            expression_attempts: HashSet::new(),
            reply_to: None,
            pending_reply: None,
            hovered_message: None,
            message_tools_top: px(0.),
            reaction_target: None,
            reaction_pending: HashSet::new(),
            message_menu: None,
            search,
            token,
            messages,
            rendered_ids,
            settings_tab: None,
            profile_editor: None,
            error: None,
            loading: HashMap::new(),
            pages: HashMap::new(),
            page_request: 0,
            typing: HashMap::new(),
            typing_task: None,
            typing_styles: HashMap::new(),
            hovered_name: None,
            name_motion_origin: std::time::Instant::now(),
            name_motion_task: Default::default(),
            read_task: None,
            typing_sent: None,
            scrolled_away: false,
            jump_task: None,
            jumping: false,
            jump_after_latest: None,
            latest_opacity: 0.,
            latest_target: false,
            latest_fade: None,
            unread_marker: None,
            gif_pool: Default::default(),
            banner_pool: Default::default(),
            media,
            media_retry: None,
            profile: None,
            profile_inspector: false,
            profile_tab: 0,
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
        self.close_picker();
        if self.login_open
            || self.profile_is_modal()
            || self.component_menu.is_some()
            || self.video.is_some()
        {
            return;
        }
        if self.settings_tab.is_none() {
            self.previous_focus = window.focused(cx);
        }
        self.select_settings_tab(tab, cx);
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
        self.close_picker();
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
        self.picker = crate::expressions::Picker {
            recent: self.settings.recent_emoji.clone(),
            ..Default::default()
        };
        self.selected_sticker = None;
        self.pending_sticker = None;
        self.failed_sticker = None;
        self.sticker_drafts.clear();
        self.expression_attempts.clear();
        self.reaction_target = None;
        self.reaction_pending.clear();
        self.profile_editor = None;
        self.generation += 1;
        self.home_page = false;
        self.profile_inspector = false;
        self.opening_dm = None;
        self.preview = false;
        self.store = Store::default();
        self.member_views = crate::members::Views::default();
        self.navigation = crate::navigation::History::default();
        self.last_channels.clear();
        self.reaction_replays.clear();
        self.members = crate::members::Members::default();
        self.selected_guild = None;
        self.selected_channel = None;
        self.loading.clear();
        self.pages.clear();
        self.live_tail.clear();
        self.close_picker();
        self.reply_to = None;
        self.pending_reply = None;
        self.read_task = None;
        self.picker.favorites.clear();
        self.picker.favorites_loaded = false;
        self.picker.favorites_loading = false;
        self.picker.favorites_error = None;
        self.message_menu = None;
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
        self.picker = crate::expressions::Picker {
            recent: self.settings.recent_emoji.clone(),
            ..Default::default()
        };
        self.selected_sticker = None;
        self.pending_sticker = None;
        self.failed_sticker = None;
        self.sticker_drafts.clear();
        self.expression_attempts.clear();
        self.reaction_target = None;
        self.reaction_pending.clear();
        self.profile_editor = None;
        self.generation += 1;
        self.home_page = false;
        self.profile_inspector = false;
        self.opening_dm = None;
        self.command(Command::Disconnect);
        self.command(Command::Forget);
        self.store = Store::demo();
        self.member_views = crate::members::Views::default();
        self.navigation = crate::navigation::History::default();
        self.last_channels.clear();
        self.reaction_replays.clear();
        self.members = crate::members::Members::demo();
        self.preview = true;
        self.status = "Preview".into();
        self.selected_guild = Some("larp".into());
        self.selected_channel = Some("campfire".into());
        self.sending = None;
        self.loading.clear();
        self.pages.clear();
        self.live_tail.clear();
        self.close_picker();
        self.reply_to = None;
        self.pending_reply = None;
        self.read_task = None;
        self.picker.favorites.clear();
        self.picker.favorites_loaded = false;
        self.picker.favorites_loading = false;
        self.picker.favorites_error = None;
        self.message_menu = None;
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
        self.jump_task = None;
        self.jumping = false;
        self.jump_after_latest = None;
        self.latest_fade = None;
        self.latest_opacity = 0.;
        self.latest_target = false;
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
        let was_home = self.home_page;
        self.home_page = false;
        self.opening_dm = None;
        if !was_home
            && self.selected_channel.as_ref() == Some(&id)
            && self.members.channel.as_ref() == Some(&id)
        {
            window.focus(&self.composer.focus_handle(cx));
            return;
        }
        if self.profile_inspector {
            self.profile = None;
            self.profile_inspector = false;
        }
        self.home_page = false;
        self.navigation.visit(id.clone());
        self.selected_guild = self
            .store
            .channels
            .get(&id)
            .and_then(|c| c.guild_id.clone());
        if let Some(guild) = &self.selected_guild {
            self.last_channels.insert(guild.clone(), id.clone());
        }
        if let Some(old) = &self.selected_channel {
            if let Some(sticker) = self.selected_sticker.take() {
                self.sticker_drafts.insert(old.clone(), sticker);
                if self.sticker_drafts.len() > 32 {
                    self.sticker_drafts.retain(|id, _| id == old);
                }
            }
            self.drafts
                .insert(old.clone(), self.composer.read(cx).content.to_string());
            if self.drafts.len() > 32 {
                self.drafts.retain(|id, _| id == old);
            }
        }
        self.live_tail.clear();
        self.close_picker();
        self.reply_to = None;
        self.read_task = None;
        self.picker.favorites.clear();
        self.picker.favorites_loaded = false;
        self.picker.favorites_loading = false;
        self.picker.favorites_error = None;
        self.message_menu = None;
        self.typing.clear();
        self.typing_styles.clear();
        self.typing_task = None;
        self.typing_sent = None;
        self.selected_channel = Some(id.clone());
        self.selected_sticker = self.sticker_drafts.remove(&id);
        let list_id = self.store.channels.get(&id).and_then(|channel| {
            let roles = self
                .store
                .guilds
                .iter()
                .find(|g| Some(&g.id) == channel.guild_id.as_ref())
                .map(|g| g.roles.as_slice())
                .unwrap_or_default();
            crate::members::list_identity(channel, roles)
        });
        self.member_views.select(
            &mut self.members,
            self.store
                .channels
                .get(&id)
                .and_then(|c| c.guild_id.clone()),
            Some(id.clone()),
            list_id,
        );
        self.subscribe_members();
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
        self.unread_marker = self
            .store
            .read
            .get(&id)
            .filter(|_| self.store.unread.contains(&id))
            .map(|read| (id.clone(), read.clone()));
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
        if self.home_page
            || self.preview
            || self.status != "Connected"
            || !window.is_window_active()
            || self.scrolled_away
            || self.settings_tab.is_some()
            || self.profile_is_modal()
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
                    && !this.home_page
                    && window.is_window_active()
                    && this.selected_channel.as_ref() == Some(&channel)
                    && !this.scrolled_away
                    && !this.profile_is_modal()
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

    fn jump_latest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.jumping {
            return;
        }
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
            if self.motion() {
                self.jump_after_latest = self.selected_channel.clone();
                self.jumping = true;
                self.scrolled_away = true;
                cx.notify();
                return;
            }
        }
        if self.motion() && !newer && !pending {
            let start = -self.messages.scroll_px_offset_for_scrollbar().y;
            self.scrolled_away = true;
            self.jumping = true;
            let channel = self.selected_channel.clone();
            self.jump_task = Some(cx.spawn_in(window, async move |view, cx| {
                for step in 1..=24 {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(12))
                        .await;
                    let keep = view
                        .update_in(cx, |this, window, cx| {
                            if this.selected_channel != channel || !this.jumping {
                                return false;
                            }
                            let t = step as f32 / 24.;
                            let eased = 1. - (1. - t).powi(4);
                            let target = this.messages.max_offset_for_scrollbar().height;
                            this.messages.set_offset_from_scrollbar(point(
                                px(0.),
                                -(start + (target - start) * eased),
                            ));
                            if step == 24 {
                                this.scrolled_away = false;
                                this.jumping = false;
                                this.messages.reset(this.rendered_ids.len());
                                this.mark_view_read(window, cx);
                            }
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    if !keep {
                        break;
                    }
                }
            }));
        } else {
            self.messages.reset(self.rendered_ids.len());
            self.mark_view_read(window, cx);
        }
        cx.notify();
    }
    fn update_latest_visibility(&mut self, cx: &mut Context<Self>) {
        let target = !self.jumping
            && (self.scrolled_away
                || self
                    .selected_channel
                    .as_ref()
                    .and_then(|id| self.pages.get(id))
                    .is_some_and(|p| p.newer));
        if target == self.latest_target {
            return;
        }
        self.latest_target = target;
        self.latest_fade = None;
        let end = if target { 1. } else { 0. };
        if !self.motion() {
            self.latest_opacity = end;
            return;
        }
        let start = self.latest_opacity;
        self.latest_fade = Some(cx.spawn(async move |view, cx| {
            for step in 1..=9 {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(14))
                    .await;
                let _ = view.update(cx, |this, cx| {
                    let t = step as f32 / 9.;
                    this.latest_opacity = start + (end - start) * (1. - (1. - t).powi(3));
                    cx.notify();
                });
            }
        }));
    }

    fn send(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.home_page
            || self.sending.is_some()
            || self.settings_tab.is_some()
            || self.login_open
            || self.profile_is_modal()
            || self.component_menu.is_some()
            || self.video.is_some()
            || self.emoji_open
        {
            return;
        }
        if self.failed_send.is_some() {
            self.error = Some("Copy or dismiss the saved unsent message before sending another. Your new draft is safe.".into());
            cx.notify();
            return;
        }
        let content = self.composer.read(cx).content.trim().to_string();
        if content.is_empty() && self.selected_sticker.is_none() {
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
                message_reference: self
                    .reply_to
                    .take()
                    .map(|r| serde_json::json!({"message_id":r.id,"channel_id":r.channel})),
                timestamp: chrono::Utc::now().to_rfc3339(),
                sticker_items: self
                    .selected_sticker
                    .take()
                    .map(|s| vec![s.message()])
                    .unwrap_or_default(),
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
                reply: self
                    .reply_to
                    .as_ref()
                    .filter(|r| r.channel == channel)
                    .map(|r| r.id.clone()),
                sticker: self.selected_sticker.as_ref().map(|s| s.id.clone()),
            }) {
                Ok(()) => {
                    self.drafts.remove(&channel);
                    self.pending_reply = self.reply_to.take();
                    self.pending_sticker = self.selected_sticker.take();
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
            Event::ProfileBadges {
                id,
                guild,
                request,
                badges,
            } => {
                let enrich = |profile: &mut Profile| {
                    for badge in &badges {
                        if profile.badges.len() < 40
                            && !profile.badges.iter().any(|b| b.same_as(badge))
                        {
                            profile.badges.push(badge.clone());
                        }
                    }
                };
                if self.profile_request == request
                    && self.profile_guild == guild
                    && let Some(profile) = self.profile.as_mut().filter(|p| p.user.id == id)
                {
                    enrich(profile);
                }
                if let Some((profile, _)) = self.profiles.get_mut(&(id, guild)) {
                    enrich(profile);
                }
            }
            Event::Reacted {
                channel,
                message,
                result,
            } => {
                self.reaction_pending.remove(&(channel, message));
                if let Err(error) = result {
                    self.error = Some(error);
                }
            }
            Event::StickerDetails(stickers) => {
                for sticker in stickers.into_iter().take(32) {
                    if self.picker.account_stickers.contains(&sticker.id) && sticker.valid() {
                        self.picker
                            .sticker_details
                            .insert(sticker.id.clone(), sticker);
                    }
                }
                self.picker_catalog_ready(cx);
            }
            Event::EmojiRecents(result) => {
                self.picker.recents_loading = false;
                match result {
                    Ok(recents) => {
                        self.picker.account_recent = recents.messages;
                        self.picker.reaction_recent = recents.reactions;
                        self.picker.account_stickers = recents.stickers;
                        self.picker
                            .sticker_details
                            .retain(|id, _| self.picker.account_stickers.contains(id));
                        self.load_recent_stickers();
                        self.picker.recents_error = None;
                    }
                    Err(error) => self.picker.recents_error = Some(error),
                }
                self.picker_catalog_ready(cx);
            }
            Event::GifFavorites(result) => {
                self.picker.favorites_loading = false;
                match result {
                    Ok(gifs) => {
                        self.picker.favorites = gifs;
                        self.picker.favorites_loaded = true;
                        self.picker.favorites_error = None;
                    }
                    Err(error) => self.picker.favorites_error = Some(error),
                }
            }
            Event::Gifs { request, result } => {
                if self.emoji_open
                    && self.picker.tab == crate::expressions::Tab::Gifs
                    && self.picker.request == request
                {
                    self.picker.loading = false;
                    match result {
                        Ok(gifs) => {
                            self.picker.gifs = gifs;
                            self.picker.error = None;
                        }
                        Err(error) => self.picker.error = Some(error),
                    }
                }
            }
            Event::StickerPacks(result) => {
                self.picker.packs_loading = false;
                match result {
                    Ok(packs) => {
                        self.picker.packs = packs;
                        self.picker.packs_loaded = true;
                        self.picker.packs_error = None;
                    }
                    Err(error) => self.picker.packs_error = Some(error),
                }
                self.picker_catalog_ready(cx);
            }
            Event::GuildExpressions {
                guild,
                mut emojis,
                mut stickers,
            } => {
                if let Some(g) = self.store.guilds.iter_mut().find(|g| g.id == guild) {
                    if g.emojis.is_none() {
                        if let Some(items) = emojis.as_mut() {
                            items.retain(crate::expressions::Emoji::valid);
                            items.truncate(1000);
                        }
                        g.emojis = emojis;
                    }
                    if g.stickers.is_none() {
                        if let Some(items) = stickers.as_mut() {
                            items.retain(crate::expressions::Sticker::valid);
                            items.truncate(500);
                        }
                        g.stickers = stickers;
                    }
                }
                self.picker_catalog_ready(cx);
            }
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
            Event::OpenedDm { user, result } => {
                if self.opening_dm.as_ref() != Some(&user) {
                    return;
                }
                self.opening_dm = None;
                match result {
                    Ok(channel) => {
                        let id = channel.id.clone();
                        self.store.channels.insert(id.clone(), channel);
                        self.selected_guild = None;
                        self.choose_channel(id, window, cx);
                    }
                    Err(error) => self.error = Some(error),
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
                } else {
                    self.opening_dm = None;
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
                            if page == Page::Latest
                                && self.jump_after_latest.as_ref() == Some(&channel)
                            {
                                self.jump_after_latest = None;
                                self.jumping = false;
                                if self.motion() {
                                    self.scrolled_away = true;
                                    self.messages.scroll_to(ListOffset {
                                        item_ix: self.rendered_ids.len().saturating_sub(8),
                                        offset_in_item: px(0.),
                                    });
                                    let weak = cx.weak_entity();
                                    let selected = channel.clone();
                                    window.on_next_frame(move |window, cx| {
                                        if let Some(view) = weak.upgrade() {
                                            view.update(cx, |this, cx| {
                                                if this.selected_channel.as_ref() == Some(&selected)
                                                {
                                                    this.jump_latest(window, cx);
                                                }
                                            });
                                        }
                                    });
                                }
                            }
                            self.mark_view_read(window, cx);
                        }
                    }
                    Err(error) => {
                        self.jump_after_latest = None;
                        self.jumping = false;
                        self.error = Some(error);
                    }
                }
            }
            Event::EditProfile { id, saved, result } => {
                self.edited_profile(id, saved, result, window, cx);
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
                            self.pending_sticker = None;
                            self.pending_reply = None;
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
                            self.failed_sticker = self.pending_sticker.take();
                            if let Some(sticker) = self.failed_sticker.clone() {
                                if self.selected_channel.as_ref() == Some(&channel)
                                    && self.selected_sticker.is_none()
                                {
                                    self.selected_sticker = Some(sticker);
                                } else {
                                    self.sticker_drafts
                                        .entry(channel.clone())
                                        .or_insert(sticker);
                                }
                            }
                            if self.reply_to.is_none()
                                && self.selected_channel.as_ref() == Some(&channel)
                            {
                                self.reply_to = self.pending_reply.take();
                            } else {
                                self.pending_reply = None;
                            }
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
                self.store.presences.ready(&data);
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
                self.store.relationships.clear();
                for relation in data["relationships"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(2000)
                {
                    self.store.relationship(relation);
                }
                // friends get the bounded identity cache before unrelated bootstrapped users.
                for friends_first in [true, false] {
                    for user in data["users"].as_array().into_iter().flatten().take(10000) {
                        let friend = user["id"]
                            .as_str()
                            .is_some_and(|id| self.store.relationships.get(id) == Some(&1));
                        if friend == friends_first
                            && self.store.users.len() < 2000
                            && let Ok(user) = serde_json::from_value::<User>(user.clone())
                        {
                            self.store.users.insert(user.id.clone(), user);
                        }
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
            "RELATIONSHIP_ADD" => self.store.relationship(&data),
            "READY_SUPPLEMENTAL" => self.store.presences.merge(&data),
            "GUILD_EMOJIS_UPDATE" => {
                if let Some(g) = self
                    .store
                    .guilds
                    .iter_mut()
                    .find(|g| data["guild_id"].as_str() == Some(&g.id))
                {
                    g.emojis = serde_json::from_value::<Vec<crate::expressions::Emoji>>(
                        data["emojis"].clone(),
                    )
                    .ok()
                    .map(|v| {
                        v.into_iter()
                            .filter(crate::expressions::Emoji::valid)
                            .take(1000)
                            .collect()
                    });
                }
            }
            "GUILD_STICKERS_UPDATE" => {
                if let Some(g) = self
                    .store
                    .guilds
                    .iter_mut()
                    .find(|g| data["guild_id"].as_str() == Some(&g.id))
                {
                    g.stickers = serde_json::from_value::<Vec<crate::expressions::Sticker>>(
                        data["stickers"].clone(),
                    )
                    .ok()
                    .map(|v| {
                        v.into_iter()
                            .filter(crate::expressions::Sticker::valid)
                            .take(500)
                            .collect()
                    });
                }
            }
            "RELATIONSHIP_REMOVE" => {
                if let Some(id) = data["id"].as_str() {
                    self.store.relationships.remove(id);
                }
            }
            "GUILD_MEMBER_LIST_UPDATE" => {
                self.members.apply(&data);
                self.store.presences.members(&data);
            }
            "PRESENCE_UPDATE" => {
                self.members.presence(&data);
                self.store.presences.update(&data);
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
                self.picker.recents_checked = None;
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
                let mut merged = data["id"]
                    .as_str()
                    .and_then(|id| self.store.channels.get(id))
                    .and_then(|channel| serde_json::to_value(channel).ok())
                    .unwrap_or_else(|| serde_json::json!({}));
                if let Some(fields) = data.as_object() {
                    for (key, value) in fields {
                        merged[key] = value.clone();
                    }
                }
                if let Ok(channel) = serde_json::from_value::<Channel>(merged) {
                    self.store.channels.insert(channel.id.clone(), channel);
                }
            }
            "CHANNEL_RECIPIENT_ADD" => {
                if let Some(channel) = data["channel_id"]
                    .as_str()
                    .and_then(|id| self.store.channels.get_mut(id))
                    && channel.kind == 3
                    && channel.recipients.len() < 100
                    && let Ok(user) = serde_json::from_value::<User>(data["user"].clone())
                    && !channel.recipients.iter().any(|old| old.id == user.id)
                {
                    channel.recipients.push(user);
                }
            }
            "CHANNEL_RECIPIENT_REMOVE" => {
                if let Some(channel) = data["channel_id"]
                    .as_str()
                    .and_then(|id| self.store.channels.get_mut(id))
                    && channel.kind == 3
                    && let Some(user) = data["user"]["id"].as_str()
                {
                    channel.recipients.retain(|old| old.id != user);
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
                    let selected = !self.home_page
                        && self.selected_channel.as_ref() == Some(&message.channel_id);
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
                            && !self.profile_is_modal()
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
            "MESSAGE_REACTION_ADD"
            | "MESSAGE_REACTION_REMOVE"
            | "MESSAGE_REACTION_REMOVE_ALL"
            | "MESSAGE_REACTION_REMOVE_EMOJI" => {
                let event = crate::reactions::event(kind, &data, &self.store.user.id);
                for message in &mut self.live_tail {
                    if Some(message.id.as_str()) == data["message_id"].as_str()
                        && Some(message.channel_id.as_str()) == data["channel_id"].as_str()
                    {
                        crate::reactions::apply(&mut message.reactions, &event);
                    }
                }
                self.store.reaction(kind, &data);
                if data["channel_id"].as_str() == self.selected_channel.as_deref() {
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
            if let Some(items) = guild.emojis.as_mut() {
                items.retain(crate::expressions::Emoji::valid);
                items.truncate(1000);
            }
            if let Some(items) = guild.stickers.as_mut() {
                items.retain(crate::expressions::Sticker::valid);
                items.truncate(500);
            }
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
                if data.get("emojis").is_none() && properties.get("emojis").is_none() {
                    guild.emojis = existing.emojis.clone();
                }
                if data.get("stickers").is_none() && properties.get("stickers").is_none() {
                    guild.stickers = existing.stickers.clone();
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
            .bg(self.color(PANEL))
            .child(
                row()
                    .id("home")
                    .size(px(40.))
                    .justify_center()
                    .rounded_xl()
                    .bg(self.color(if self.selected_guild.is_none() {
                        0x5865f2
                    } else {
                        SURFACE
                    }))
                    .cursor_pointer()
                    .child(
                        svg()
                            .path("home.svg")
                            .size(px(28.))
                            .text_color(self.color(TEXT)),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_friends(window, cx);
                        cx.notify();
                    })),
            )
            .child(div().w(px(26.)).h(px(1.)).bg(self.color(LINE)))
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
                    .child(self.channel_avatar(channel, 40.))
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
                    .bg(self.color(SURFACE))
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
                        .bg(self.color(TEXT)),
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
                        .top(px(20.))
                        .w(px(4.))
                        .h(px(8.))
                        .rounded_full()
                        .bg(self.color(TEXT)),
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
                        .bg(self.color(TEXT))
                        .map(|element| {
                            if self.motion() {
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
                    .bg(self.color(if selected { 0x5865f2 } else { SURFACE }))
                    .text_color(self.color(if selected { LILAC } else { MUTED }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .hover(|s| s.bg(self.color(LINE)))
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
                        let remembered = this
                            .last_channels
                            .get(&id)
                            .filter(|channel| {
                                this.store.channels.get(*channel).is_some_and(|c| {
                                    c.guild_id.as_ref() == Some(&id) && c.readable()
                                })
                            })
                            .cloned();
                        let channel = remembered.or_else(|| {
                            this.store
                                .channels
                                .values()
                                .filter(|c| c.guild_id.as_ref() == Some(&id) && c.readable())
                                .min_by(|a, b| {
                                    a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id))
                                })
                                .map(|c| c.id.clone())
                        });
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
        let selected = !self.home_page && self.selected_channel.as_ref() == Some(&id);
        let unread = self.store.unread.contains(&id);
        let pings = self.store.mentions.get(&id).copied().unwrap_or(0);
        let content = row()
            .w_full()
            .id(SharedString::from(format!("dm-{id}")))
            .on_hover({
                let key = channel
                    .recipients
                    .first()
                    .map(|u| format!("dm-user-{}", u.id));
                cx.listener(move |this, hovered: &bool, _, cx| {
                    this.hovered_name = if *hovered { key.clone() } else { None };
                    cx.notify();
                })
            })
            .relative()
            .h(px(44.))
            .when(channel.kind == 1, |s| {
                s.when_some(channel.recipients.first(), |s, user| {
                    s.child(self.nameplate_art(user, None, 44.))
                })
            })
            .px_2()
            .gap_3()
            .rounded_md()
            .cursor_pointer()
            .bg(self.color(if selected { SURFACE } else { SIDEBAR }))
            .hover(|s| s.bg(self.color(SURFACE)))
            .when(unread && !selected, |s| {
                s.child(
                    div()
                        .absolute()
                        .left(px(-4.))
                        .top(px(18.))
                        .size(px(6.))
                        .rounded_full()
                        .bg(self.color(TEXT)),
                )
            })
            .child(self.channel_avatar(channel, 32.))
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .text_color(self.color(if selected || unread { TEXT } else { MUTED }))
                    .relative()
                    .child(
                        div()
                            .truncate()
                            .font_weight(if unread {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(
                                channel
                                    .recipients
                                    .first()
                                    .filter(|_| channel.kind == 1)
                                    .map(|user| self.display_name(&channel.label(), user, None, cx))
                                    .unwrap_or_else(|| {
                                        div().child(channel.label()).into_any_element()
                                    }),
                            ),
                    )
                    .when(channel.kind == 3, |s| {
                        s.child(
                            label(format!("{} members", channel.recipients.len() + 1))
                                .text_size(px(11.)),
                        )
                    })
                    .when_some(
                        channel
                            .recipients
                            .first()
                            .filter(|_| channel.kind == 1)
                            .and_then(|u| self.store.presences.get(&u.id))
                            .and_then(crate::presence::Presence::summary),
                        |s, text| s.child(label(text.clone()).text_size(px(11.)).truncate()),
                    ),
            )
            .when(pings > 0, |s| s.child(ping_badge(pings)))
            .when(unread && pings == 0, |s| {
                s.child(div().size(px(6.)).rounded_full().bg(self.color(TEXT)))
            })
            .on_click(
                cx.listener(move |this, _, window, cx| this.choose_channel(id.clone(), window, cx)),
            )
            .into_any_element();
        div().w_full().px_2().child(content).into_any_element()
    }

    fn sidebar(&self, width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let guild = self
            .selected_guild
            .as_ref()
            .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id));
        let title = guild
            .map_or("Direct Messages", |g| g.name.as_str())
            .to_string();
        let banner = guild.and_then(|g| crate::media::banner(&g.id, None, g.banner.as_deref()?));
        let banner_image = banner
            .as_deref()
            .and_then(|url| self.banner_media(url, !self.profile_is_modal(), cx));
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
                let category = channel
                    .parent_id
                    .clone()
                    .unwrap_or_else(|| format!("root-{}", channel.kind == 2));
                let collapsed = self.collapsed_categories.contains(&category);
                let clicked = category.clone();
                navigation.push(
                    row()
                        .id(SharedString::from(format!("category-{category}")))
                        .h(px(32.))
                        .pt_2()
                        .px_1()
                        .gap_1()
                        .cursor_pointer()
                        .child(
                            icon(
                                if collapsed {
                                    "chevron-right"
                                } else {
                                    "chevron-down"
                                },
                                12.,
                            )
                            .text_color(self.color(MUTED)),
                        )
                        .child(
                            label(title.to_uppercase())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(11.)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.collapsed_categories.remove(&clicked) {
                                this.collapsed_categories.insert(clicked.clone());
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                last_group = Some(group);
            }
            let id = channel.id.clone();
            let selected = self.selected_channel.as_ref() == Some(&id);
            let category = channel
                .parent_id
                .clone()
                .unwrap_or_else(|| format!("root-{}", channel.kind == 2));
            if self.collapsed_categories.contains(&category) && !selected {
                continue;
            }
            navigation.push(
                row()
                    .id(SharedString::from(format!("channel-{id}")))
                    .h(px(if channel.guild_id.is_none() { 40. } else { 32. }))
                    .px_2()
                    .gap_2()
                    .relative()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(self.color(if selected { SURFACE } else { SIDEBAR }))
                    .hover(|s| s.bg(self.color(SURFACE)))
                    .text_color(self.color(if selected { TEXT } else { MUTED }))
                    .child(if channel.guild_id.is_none() {
                        channel
                            .recipients
                            .first()
                            .map(|user| self.avatar(user, 28.))
                            .unwrap_or_else(|| {
                                div()
                                    .size(px(28.))
                                    .rounded_full()
                                    .bg(self.color(SURFACE))
                                    .child("@")
                                    .into_any_element()
                            })
                    } else {
                        div()
                            .w(px(22.))
                            .text_size(px(21.))
                            .text_color(rgb(0x80848e))
                            .child(icon(crate::icons::channel_icon(channel.kind), 18.))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(15.))
                            .font_weight(if selected || self.store.unread.contains(&id) {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(channel.label()),
                    )
                    .when(
                        self.store.mentions.get(&id).copied().unwrap_or(0) > 0,
                        |s| s.child(ping_badge(self.store.mentions[&id])),
                    )
                    .when(
                        self.store.unread.contains(&id)
                            && self.store.mentions.get(&id).copied().unwrap_or(0) == 0,
                        |s| {
                            s.child(
                                div()
                                    .absolute()
                                    .left(px(-8.))
                                    .w(px(4.))
                                    .h(px(8.))
                                    .rounded_r_full()
                                    .bg(self.color(TEXT)),
                            )
                        },
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_channel(id.clone(), window, cx)
                    }))
                    .into_any_element(),
            );
        }
        column()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .bg(self.color(SIDEBAR))
            .child(if self.selected_guild.is_none() {
                row()
                    .h(px(48.))
                    .px_2()
                    .border_b_1()
                    .border_color(self.color(LINE))
                    .child(
                        row()
                            .w_full()
                            .px_2()
                            .rounded_md()
                            .bg(self.color(PANEL))
                            .border_1()
                            .border_color(self.color(LINE))
                            .child(self.search.clone()),
                    )
                    .into_any_element()
            } else {
                column()
                    .relative()
                    .h(px(if banner.is_some() { 148. } else { 48. }))
                    .flex_shrink_0()
                    .justify_start()
                    .border_b_1()
                    .border_color(self.color(LINE))
                    .when_some(banner_image, |s, image| {
                        s.child(div().absolute().inset_0().overflow_hidden().child(image))
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
                    )
                    .into_any_element()
            })
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
                            .border_color(self.color(LINE))
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
            .when(self.selected_guild.is_some(), |s| {
                s.child(
                    row()
                        .mx_2()
                        .mt_2()
                        .px_2()
                        .rounded_md()
                        .bg(self.color(PANEL))
                        .gap_2()
                        .child(icon("search", 16.).text_color(self.color(MUTED)))
                        .child(self.search.clone()),
                )
            })
            .when(self.selected_guild.is_none(), |s| {
                s.child(
                    column().child(self.friends_navigation(cx)).child(
                        row().px_4().pt_4().pb_2().child(
                            label("DIRECT MESSAGES")
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD),
                        ),
                    ),
                )
            })
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
                    .gap(px(2.))
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
    }

    fn profile_is_modal(&self) -> bool {
        self.profile.is_some() && !self.profile_inspector
    }

    fn open_profile(&mut self, user: User, window: &mut Window, cx: &mut Context<Self>) {
        self.profile_inspector = false;
        self.profile_tab = 0;
        self.close_picker();
        self.previous_focus = window.focused(cx);
        window.focus(&self.modal_focus);
        self.profile_error = None;
        self.profile_request += 1;
        self.profile_started = std::time::Instant::now();
        self.profile_guild = if self.home_page {
            None
        } else {
            self.selected_channel
                .as_ref()
                .and_then(|id| self.store.channels.get(id))
                .and_then(|c| c.guild_id.clone())
        };
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
                own: user.id == self.store.user.id,
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
        self.media_image_bounded(url, width, height, (400., 280.))
    }

    fn media_image_bounded(
        &self,
        url: &str,
        width: Option<u32>,
        height: Option<u32>,
        limit: (f32, f32),
    ) -> AnyElement {
        let moving = self.motion()
            && reqwest::Url::parse(url).ok().is_some_and(|u| {
                [".gif", ".apng"]
                    .iter()
                    .any(|ext| u.path().to_ascii_lowercase().ends_with(ext))
            });
        let image = if moving {
            self.media
                .get(&format!("{url}#motion"))
                .or_else(|| self.media.get(url))
        } else {
            self.media.get(url)
        };
        let (w, h) = media_size(
            width.zip(height).or_else(|| {
                image.as_ref().map(|image| {
                    let size = image.size(0);
                    (size.width.0 as u32, size.height.0 as u32)
                })
            }),
            limit.0,
            limit.1,
        );
        div()
            .w(px(w))
            .max_w_full()
            .h(px(h))
            .rounded_lg()
            .overflow_hidden()
            .bg(self.color(PANEL))
            .map(|s| match image {
                Some(image) => s.child(
                    img(image.clone())
                        .id(("media-image", image.id.0))
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
        let reply = message.kind == 19 || message.message_reference.is_some();
        if reply {
            body = body.child(self.reply_preview(message));
        }
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
                                cx,
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
                    .child(label(time.clone()).text_size(px(12.))),
            );
        }
        let decorations = self.modules.decorate(crate::modules::MessageView {
            author_bot: message.author.bot,
            has_attachments: !message.attachments.is_empty(),
            content: &message.content,
        });
        if !decorations.is_empty() {
            body = body.child(
                row()
                    .gap_2()
                    .children(decorations.into_iter().enumerate().map(|(index, label)| {
                        let hint = format!("Added by {}", label.module);
                        div()
                            .id(("module-label", index))
                            .px_2()
                            .rounded_md()
                            .text_size(px(10.))
                            .text_color(rgb(label.color))
                            .border_1()
                            .border_color(rgba((label.color << 8) | 0x66))
                            .child(label.text.to_string())
                            .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
                    })),
            );
        }
        if !content.is_empty() && !gif::media_only(message) {
            body = body.child(
                div()
                    .debug_selector(|| "message-content".into())
                    .child(self.rich_text_at(&content, 16.)),
            );
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
                if crate::media::animation::image_url(source) {
                    let poster = crate::model::EmbedMedia {
                        url: Some(source.to_string()),
                        width: attachment.width,
                        height: attachment.height,
                        ..Default::default()
                    };
                    body = body.child(self.gif_card(
                        source,
                        Some(&poster),
                        attachment.width.zip(attachment.height),
                        cx,
                    ));
                } else {
                    body =
                        body.child(self.media_image(source, attachment.width, attachment.height));
                }
            }
            body = body.child(
                button(("attachment", index), "")
                    .child(icon("external-link", 16.))
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
        for (index, embed) in message
            .embeds
            .iter()
            .filter(|_| message.flags & 4 == 0)
            .take(10)
            .enumerate()
        {
            let playable = embed.video.as_ref().and_then(|video| {
                video
                    .proxy_url
                    .as_deref()
                    .or(video.url.as_deref())
                    .filter(|url| crate::video::source(url).is_some())
                    .map(|url| (url, video))
            });
            if let Some((url, poster, dimensions)) = gif::embed_gif(embed) {
                body = body.child(self.gif_card(url, poster, dimensions, cx));
                continue;
            }
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
            if message.flags & 4 == 0
                && let Some(image) = gif::embed_image(embed)
                && let Some(url) = gif::image_source(image)
            {
                body = body.child(self.media_image(url, image.width, image.height));
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
                .bg(self.color(SIDEBAR))
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
            if playable.is_none() {
                if let Some(media) = embed.image.as_ref()
                    && let Some(url) = gif::image_source(media)
                {
                    card = card.child(self.media_image(url, media.width, media.height));
                } else if let Some(thumbnail) = embed.thumbnail.as_ref()
                    && let Some(url) = gif::image_source(thumbnail)
                {
                    card = card.child(self.media_image_bounded(
                        url,
                        thumbnail.width,
                        thumbnail.height,
                        (84., 84.),
                    ));
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
        if !message.sticker_items.is_empty() {
            body = body.child(self.stickers(message));
        }
        if !message.reactions.is_empty() {
            body = body.child(self.reaction_bar(message, cx));
        }
        for (index, component) in message.components.iter().take(40).enumerate() {
            body = body.child(self.component(message, component, index.to_string(), 0, cx));
        }
        if content.is_empty()
            && message.attachments.is_empty()
            && message.embeds.is_empty()
            && message.components.is_empty()
            && message.sticker_items.is_empty()
        {
            body = body.child(label(if message.author.bot {
                "This bot response has no supported content yet."
            } else {
                "Message content is unavailable."
            }));
        }
        row()
            .id(SharedString::from(format!("message-{}", message.id)))
            .debug_selector({
                let id = message.id.clone();
                move || format!("message-row-{id}")
            })
            .min_h(px(34.))
            .on_mouse_down(MouseButton::Right, {
                let message = message.clone();
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_message_menu(message.clone(), event.position, window, cx);
                })
            })
            .relative()
            .on_hover({
                let id = message.id.clone();
                cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.hovered_message = Some(id.clone());
                    } else if this.hovered_message.as_deref() == Some(&id) {
                        this.hovered_message = None;
                    }
                    cx.notify();
                })
            })
            .hover(|s| s.bg(rgba(0xffffff06)))
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
            .child(
                div()
                    .w(px(40.))
                    .mt(px(if reply { 24. } else { 0. }))
                    .flex_shrink_0()
                    .when(
                        grouped && self.hovered_message.as_deref() == Some(&message.id),
                        |s| s.child(label(time.clone()).text_size(px(9.)).text_center().w_full()),
                    )
                    .when(!grouped, |s| {
                        s.child(
                            div()
                                .id("avatar")
                                .cursor_pointer()
                                .child(self.decorated_avatar(
                                    &message.author,
                                    message.member.as_ref(),
                                    self.selected_guild.as_deref(),
                                    40.,
                                    self.hovered_message.as_deref() == Some(&message.id),
                                ))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_profile(avatar_user.clone(), window, cx)
                                })),
                        )
                    }),
            )
            .child(body)
            .when(
                self.hovered_message.as_deref() == Some(&message.id)
                    || self
                        .message_menu
                        .as_ref()
                        .is_some_and(|(m, _)| m.id == message.id)
                    || self
                        .reaction_target
                        .as_ref()
                        .is_some_and(|m| m.id == message.id),
                |s| s.child(self.message_tools(message, cx)),
            )
            .into_any_element()
    }

    fn conversation(&self, widths: panes::PaneWidths, cx: &mut Context<Self>) -> AnyElement {
        let channel = self
            .selected_channel
            .as_ref()
            .and_then(|id| self.store.channels.get(id));
        let title = channel.map_or_else(|| "Your little corner".into(), Channel::label);
        let topic = channel.and_then(|c| c.topic.clone()).unwrap_or_default();
        let has_topic = !topic.is_empty();
        let weak = cx.weak_entity();
        let selected = self.selected_channel.clone();
        let header = row()
            .debug_selector(|| "channel-header".into())
            .h(px(48.))
            .flex_shrink_0()
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(self.color(LINE))
            .child(
                channel
                    .filter(|c| c.guild_id.is_none())
                    .map(|c| self.channel_avatar(c, 24.))
                    .unwrap_or_else(|| {
                        icon(
                            crate::icons::channel_icon(channel.map_or(0, |c| c.kind)),
                            22.,
                        )
                        .text_color(self.color(MUTED))
                        .into_any_element()
                    }),
            )
            .child(
                div()
                    .id("channel-title")
                    .min_w_0()
                    .max_w(px(240.))
                    .truncate()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .tooltip({
                        let title = title.clone();
                        move |_, cx| cx.new(|_| Hint(title.clone())).into()
                    })
                    .child(
                        channel
                            .filter(|c| c.kind == 1)
                            .and_then(|c| c.recipients.first())
                            .map(|user| self.display_name(&title, user, None, cx))
                            .unwrap_or_else(|| div().child(title).into_any_element()),
                    ),
            )
            .when(has_topic, |s| {
                s.child(div().w(px(1.)).h(px(24.)).bg(self.color(LINE)))
                    .child(
                        label(topic)
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.)),
                    )
            })
            .when(!has_topic, |s| s.child(div().flex_1()))
            .when(
                channel.is_some_and(|c| c.guild_id.is_some() || c.kind == 3),
                |s| {
                    s.child(
                        toolbar_button("toggle-members", "users", "Toggle member list").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.members_open = !this.members_open;
                                this.subscribe_members();
                                cx.notify();
                            }),
                        ),
                    )
                },
            )
            .when_some(
                channel
                    .filter(|c| c.kind == 1)
                    .and_then(|c| c.recipients.first())
                    .cloned(),
                |s, user| {
                    s.child(
                        toolbar_button("conversation-profile", "user", "Toggle profile panel")
                            .when(self.profile_inspector, |s| {
                                s.bg(self.color(SURFACE)).text_color(self.color(TEXT))
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.profile_inspector {
                                    this.profile = None;
                                    this.profile_inspector = false;
                                } else {
                                    this.open_profile(user.clone(), window, cx);
                                    this.profile_inspector = true;
                                    window.focus(&this.composer.focus_handle(cx));
                                }
                                cx.notify();
                            })),
                    )
                },
            )
            .child(
                button("conversation-search", "")
                    .w(px(170.))
                    .h(px(28.))
                    .py_0()
                    .px_2()
                    .bg(self.color(PANEL))
                    .child(label("Find conversation").flex_1())
                    .child(icon("search", 16.))
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(&this.search.focus_handle(cx))
                    })),
            )
            .when(self.preview, |s| s.child(badge("PREVIEW")));
        let content=column().flex_1().min_w_0().h_full()
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
                            .text_color(self.color(ACCENT)),
                        )
                        .child(div().text_size(px(24.)).child("A little room to breathe."))
                        .child(label("Choose a conversation from the sidebar.")),
                )
            })
            .when(channel.is_some(), |s| {
                s.child(column().debug_selector(||"timeline".into()).w_full().relative().flex_1().min_h_0().child(
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
                                |message| {
                                    let previous=view.selected_channel.as_deref().and_then(|id|view.store.channel_messages(id).get(index.wrapping_sub(1)));
                                    let date=chrono::DateTime::parse_from_rfc3339(&message.timestamp).ok().map(|d|d.with_timezone(&chrono::Local));
                                    let new_day=date.as_ref().is_some_and(|date|previous.and_then(|p|chrono::DateTime::parse_from_rfc3339(&p.timestamp).ok()).is_none_or(|p|p.with_timezone(&chrono::Local).date_naive()!=date.date_naive()));
                                    let unread=view.unread_marker.as_ref().is_some_and(|(channel,read)|channel==&message.channel_id && snowflake_after(&message.id,read) && previous.is_none_or(|p|!snowflake_after(&p.id,read)));
                                    column().w_full().min_w_0().when(new_day,|s|s.child(view.timeline_divider(&date.unwrap().format("%B %-d, %Y").to_string(),false)))
                                        .when(unread,|s|s.child(view.timeline_divider("New messages",true)))
                                        .child(view.message(&message,grouped&&!unread,cx)).into_any_element()
                                },
                            )
                        })
                    })
                    .relative()
                    .w_full()
                    .flex_1()
                    .min_h_0(),
                )
                    .when(self.latest_opacity>0.,|s|s.child(self.latest_button(cx)))
                )
                .when(
                    selected
                        .as_ref()
                        .is_some_and(|id| self.loading.contains_key(id)),
                    |s| s.child(label("Loading messages…").px_6()),
                )
                .child(
                    column()
                        .px_4()
                        .pb_4()
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
                        .when(!self.typing.is_empty(), |s|s.child(self.typing_indicator()))
                        .when(self.reply_to.is_some(),|s|s.child(self.reply_banner(cx)))
                        .when(self.selected_sticker.is_some(),|s|s.child(self.sticker_draft(cx)))
                        .when_some(self.failed_send.clone(), |s, (channel, content)| {
                            s.child(
                                column()
                                    .p_3()
                                    .gap_2()
                                    .rounded_lg()
                                    .bg(self.color(PANEL))
                                    .child(label(format!(
                                        "Unsent: {}",
                                        if content.is_empty() {self.failed_sticker.as_ref().map(|s|s.name.clone()).unwrap_or_else(||"Sticker".into())} else {content.chars().take(160).collect::<String>()}
                                    )))
                                    .when_some(self.failed_sticker.clone(),|s,sticker| {
                                        let channel=channel.clone();
                                        s.child(button("restore-unsent-sticker","Restore sticker").on_click(cx.listener(move |this,_,window,cx| {
                                            this.choose_channel(channel.clone(),window,cx);
                                            this.selected_sticker=Some(sticker.clone());
                                            this.failed_sticker=None;this.failed_send=None;cx.notify();
                                        })))
                                    })
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
                                                        this.failed_sticker = None;
                                                    }
                                                    cx.notify();
                                                }),
                                            )),
                                    ),
                            )
                        })
                        .child(
                            row()
                                .min_h(px(44.))
                                .px_3()
                                .gap_2()
                                .rounded_xl()
                                .border_1()
                                .border_color(self.color(LINE))
                                .bg(self.color(SURFACE))
                                .child(div().flex_1().min_w_0().child(self.composer.clone()))
                                .child(toolbar_button("choose-gif", "gif", "Choose GIF").on_click(cx.listener(|this,_,window,cx|this.toggle_picker(crate::expressions::Tab::Gifs,window,cx))))
                                .child(toolbar_button("choose-sticker", "sticker", "Choose sticker").on_click(cx.listener(|this,_,window,cx|this.toggle_picker(crate::expressions::Tab::Stickers,window,cx))))
                                .child(toolbar_button("choose-emoji", "smile", "Choose emoji").on_click(cx.listener(|this,_,window,cx|this.toggle_picker(crate::expressions::Tab::Emoji,window,cx))))
                                .when(self.settings.show_send_button, |s| s.child(
                                    toolbar_button("send", "send", "Send message")
                                    .text_color(self.color(if self.sending.is_some() { MUTED } else { ACCENT }))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.send(window, cx)),
                                    ),
                                )),
                        )
                        .when(self.preview,|s|s.child(
                            row().justify_between().px_1().child(
                                label(if self.preview {
                                    "Preview messages stay in this window."
                                } else {
                                    "Enter to send · Shift+Enter for a new line · mentions do not ping"
                                })
                                .text_size(px(10.)),
                            ),
                        )),
                )
            })
            .into_any_element();
        column()
            .debug_selector(|| "conversation-shell".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .rounded_tl_lg()
            .border_l_1()
            .border_t_1()
            .border_color(self.color(LINE))
            .overflow_hidden()
            .child(header)
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(content)
                    .when(widths.members > 0., |s| {
                        s.child(self.pane_divider(panes::Pane::Members, cx)).child(
                            if let Some(group) = channel.filter(|c| c.kind == 3) {
                                self.group_members(group, px(widths.members), cx)
                            } else {
                                self.member_rail(px(widths.members), cx)
                            },
                        )
                    })
                    .when_some(
                        self.profile.as_ref().filter(|_| self.profile_inspector),
                        |s, profile| s.child(self.profile_panel(profile, cx)),
                    ),
            )
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
                .bg(self.color(SURFACE))
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
                div().text_size(px(15.)).text_color(self.color(MUTED)).line_height(px(24.))
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
                    .text_color(self.color(MUTED))
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
            .bg(self.color(PANEL))
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
                    .bg(self.color(BG))
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
            .border_color(self.color(LINE))
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
                    .bg(self.color(if enabled { 0x5865f2 } else { LINE }))
                    .when(enabled, |s| s.justify_end())
                    .child(
                        div()
                            .size(px(16.))
                            .rounded_full()
                            .bg(self.color(if enabled { ACCENT } else { MUTED })),
                    ),
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
                    body = body.child(column().mt_4().p_4().rounded_xl().border_1().border_color(self.color(LINE)).bg(self.color(SURFACE)).gap_3()
                        .child(div().text_size(px(13.)).child("Discord session token"))
                        .child(row().px_2().rounded_lg().bg(self.color(PANEL)).child(self.token.clone()))
                        .child(label("Your token is a password. It is sent only to Discord. Larpcord stores remembered sessions in your OS credential store.").line_height(px(19.)))
                        .child(button("login", "Connect account").bg(rgb(0x5865f2)).text_color(self.color(ACCENT)).on_click(cx.listener(|this, _, window, cx| this.login(window, cx)))))
                        .child(label("This is an early, independent Discord client. Voice, quests and existing JavaScript mods are still pending."));
                } else {
                    body = body.child(
                        column()
                            .p_4()
                            .rounded_xl()
                            .bg(self.color(SURFACE))
                            .gap_3()
                            .child(div().child(self.store.user.name().to_string()))
                            .child(label(self.status.clone()))
                            .child(button("edit-own-profile", "Edit profile").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.select_settings_tab(profile_edit::TAB, cx)
                                }),
                            ))
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
                    .child(button("browse-themes", "Browse themes").on_click(cx.listener(|this,_,_,cx|
                        this.select_settings_tab(modules::THEMES_TAB,cx)
                    )))
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
                            "send-button",
                            "Show Send button",
                            "Add a Send button beside the composer. Enter sends in either mode.",
                            self.settings.show_send_button,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.show_send_button = !this.settings.show_send_button;
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
            2 => {
                body = body.child(self.modules_settings(false, cx));
            }
            modules::THEMES_TAB => {
                body = body.child(self.modules_settings(true, cx));
            }
            profile_edit::TAB => {
                body = body.child(self.edit_profile_settings(cx));
            }
            _ => {
                body = body.child(img(ImageSource::Resource(Resource::Embedded("mascot.png".into()))).size(px(58.)).text_color(self.color(ACCENT)))
                    .child(div().text_size(px(30.)).child("larpcord"))
                    .child(label(format!("{} • development preview", env!("CARGO_PKG_VERSION"))))
                    .child(div().text_size(px(16.)).child("A little room to breathe."))
                    .child(label("Built in Rust with GPUI. Inspired by Kawaicord's small-footprint approach."))
                    .child(column().mt_4().p_4().rounded_xl().bg(self.color(SURFACE)).gap_3()
                        .child(div().child(format!("{} messages cached", self.store.cached_messages())))
                        .child(label("Up to 12 conversations and 200 messages per conversation. Images load as you scroll. The connection runs independently of rendering."))
                        .child(button("clear-cache", "Clear message cache").on_click(cx.listener(|this, _, _, cx| { this.store.clear_history(); this.refresh_messages(); cx.notify(); }))))
                    .child(button("github", "Open GitHub ↗").on_click(|_, _, cx| cx.open_url("https://github.com/evenafterallisaid/larpcord")))
                    .child(label("Native voice and executable plugin adapters are future milestones. Video codec support depends on your platform and installed graphics drivers."));
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
                    .bg(self.color(BG))
                    .child(
                        column()
                            .w(px(240.))
                            .h_full()
                            .p_4()
                            .gap_2()
                            .bg(self.color(PANEL))
                            .child(label("LARPCORD").mt_2().mb_4().text_size(px(10.)))
                            .children(
                                [
                                    (0, "Account"),
                                    (profile_edit::TAB, "Profile"),
                                    (1, "Preferences"),
                                    (modules::THEMES_TAB, "Themes"),
                                    (2, "Modules"),
                                    (3, "About"),
                                ]
                                .into_iter()
                                .map(|(index, name)| {
                                    row()
                                        .id(("settings-tab", index))
                                        .px_3()
                                        .py_2()
                                        .rounded_lg()
                                        .cursor_pointer()
                                        .bg(self.color(if tab == index { SURFACE } else { PANEL }))
                                        .text_color(self.color(if tab == index {
                                            ACCENT
                                        } else {
                                            MUTED
                                        }))
                                        .child(name)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.select_settings_tab(index, cx);
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
            .map(|element| fade_in(element, "settings-enter", self.motion()))
    }
}

impl Render for Larpcord {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tools_message = self
            .hovered_message
            .as_deref()
            .or_else(|| self.message_menu.as_ref().map(|(m, _)| m.id.as_str()))
            .or_else(|| self.reaction_target.as_ref().map(|m| m.id.as_str()));
        self.message_tools_top = tools_message
            .and_then(|id| self.rendered_ids.iter().position(|m| m == id))
            .and_then(|index| self.messages.bounds_for_item(index))
            .map_or(px(0.), |bounds| {
                (self.messages.viewport_bounds().top() - bounds.top()).max(px(0.))
            });
        self.media.begin_frame();
        let emoji_images = crate::content::parts(&self.composer.read(cx).content)
            .into_iter()
            .filter_map(|part| {
                if let crate::content::Part::Emoji { id, .. } = part {
                    self.media
                        .get(&format!(
                            "https://cdn.discordapp.com/emojis/{id}.webp?size=64"
                        ))
                        .map(|image| (id.to_string(), image))
                } else {
                    None
                }
            })
            .collect::<std::collections::HashMap<_, _>>();
        self.composer.update(cx, |input, cx| {
            if input.emoji_images.len() != emoji_images.len()
                || emoji_images.iter().any(|(id, image)| {
                    input
                        .emoji_images
                        .get(id)
                        .is_none_or(|old| old.id != image.id)
                })
            {
                input.emoji_images = emoji_images;
                cx.notify();
            }
        });

        self.window_active = window.is_window_active();
        if !self.motion() && self.jumping {
            self.jump_task = None;
            self.jumping = false;
            self.jump_after_latest = None;
        }
        self.update_latest_visibility(cx);
        gif::Pool::frame(
            &self.gif_pool,
            window,
            cx,
            window.is_window_active()
                && self.settings.animations
                && self.settings_tab.is_none()
                && !self.profile_is_modal()
                && self.video.is_none(),
        );
        gif::Pool::frame(
            &self.banner_pool,
            window,
            cx,
            self.motion() && self.settings_tab.is_none() && self.video.is_none(),
        );
        let widths = self.pane_widths(window.viewport_size().width);
        column()
            .relative()
            .track_focus(&self.root_focus)
            .on_drop(cx.listener(|this, _: &panes::Pane, _, cx| this.save_pane_resize(cx)))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.save_pane_resize(cx)),
            )
            .on_scroll_wheel(cx.listener(|this, _: &ScrollWheelEvent, _, _| {
                if this.jumping {
                    this.jumping = false;
                    this.jump_task = None;
                }
            }))
            .size_full()
            .bg(self.color(BG))
            .text_color(self.color(TEXT))
            .text_size(px(14.))
            .font_family("Inter")
            .on_action(|_: &Quit, _, cx| cx.quit())
            .on_action(
                cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ClosePanel, window, cx| {
                if this.message_menu.take().is_some() {
                    if let Some(focus) = this.previous_focus.take() {
                        window.focus(&focus);
                    }
                    cx.notify();
                } else if this.emoji_open {
                    this.close_picker();
                    window.focus(&this.composer.focus_handle(cx));
                    cx.notify();
                } else if this.video.is_some() {
                    this.close_video(window, cx);
                } else if this.settings_tab.is_some() {
                    this.close_settings(window, cx);
                } else if this.component_menu.take().is_some() || this.profile.take().is_some() {
                    this.profile_inspector = false;
                    if let Some(focus) = this.previous_focus.take() {
                        window.focus(&focus);
                    }
                    cx.notify();
                } else if this.login_open {
                    this.close_login(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                if this.settings_tab.is_none()
                    && !this.login_open
                    && !this.profile_is_modal()
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
                    .child(
                        column()
                            .w(widths.sidebar())
                            .flex_shrink_0()
                            .h_full()
                            .child(
                                row()
                                    .flex_1()
                                    .min_h_0()
                                    .child(self.rail(cx))
                                    .child(self.sidebar(px(widths.channels), cx)),
                            )
                            .child(self.account_bar(cx)),
                    )
                    .child(self.pane_divider(panes::Pane::Channels, cx))
                    .child(if self.home_page {
                        self.friends_page(cx)
                    } else {
                        self.conversation(widths, cx)
                    }),
            )
            .when(self.emoji_open, |s| s.child(self.emoji_panel(cx)))
            .when(self.message_menu.is_some(), |s| {
                s.child(self.message_menu_panel(cx))
            })
            .when(self.settings_tab.is_some(), |s| s.child(self.settings(cx)))
            .when(self.login_open, |s| s.child(self.login_screen(cx)))
            .when_some(
                self.profile.clone().filter(|_| !self.profile_inspector),
                |s, user| s.child(self.profile_panel(&user, cx)),
            )
            .when(self.video.is_some(), |s| s.child(self.video_panel(cx)))
            .when_some(self.component_menu.clone(), |s, (message, component)| {
                s.child(self.component_menu_panel(&message, &component, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Larpcord, Message, Page};
    use super::{Profile, User};
    use crate::network::{Envelope, Event};
    use gpui::{Context, Focusable, TestAppContext, Window};
    use gpui::{Modifiers, MouseButton, point, px, size};

    #[gpui::test]
    fn quick_reactions_toggle_only_the_selected_message_and_keep_the_draft(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |v, _, cx| {
                let message = v
                    .store
                    .channel_messages("campfire")
                    .iter()
                    .find(|m| m.id == "11")
                    .unwrap()
                    .clone();
                v.composer.update(cx, |i, cx| i.set("draft", cx));
                v.react_to(&message, "👍", cx);
                assert!(
                    v.store
                        .channel_messages("campfire")
                        .iter()
                        .find(|m| m.id == "11")
                        .unwrap()
                        .reactions
                        .iter()
                        .any(|r| r.me && r.emoji.name.as_deref() == Some("👍"))
                );
                v.react_to(&message, "👍", cx);
                assert!(
                    !v.store
                        .channel_messages("campfire")
                        .iter()
                        .find(|m| m.id == "11")
                        .unwrap()
                        .reactions
                        .iter()
                        .any(|r| r.me && r.emoji.name.as_deref() == Some("👍"))
                );
                assert_eq!(v.composer.read(cx).content.as_ref(), "draft");
            })
            .unwrap();
    }
    #[gpui::test]
    fn own_profile_does_not_offer_mutual_tabs(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.settings.animations = false;
            v.profile = Some(Profile {
                user: v.store.user.clone(),
                mutual_friends_count: Some(42),
                ..Default::default()
            });
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        assert!(cx.debug_bounds("profile-tab-1").is_none());
    }
    #[gpui::test]
    fn hovering_from_message_body_into_reply_keeps_the_action_clickable(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.settings.animations = false;
            v.store.clear_history();
            v.store.insert(Message {
                id: "98765".into(),
                channel_id: "campfire".into(),
                content: "hover here".into(),
                author: User {
                    id: "friend".into(),
                    username: "friend".into(),
                    ..Default::default()
                },
                timestamp: chrono::Utc::now().to_rfc3339(),
                ..Default::default()
            });
            v.refresh_messages();
            v.messages.scroll_to(gpui::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        let viewport = cx.update(|window, _| window.viewport_size());
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
        let row = cx.debug_bounds("message-row-98765").unwrap();
        cx.simulate_mouse_move(row.center(), None, Modifiers::default());
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
        cx.update(|_, cx| {
            assert_eq!(
                view.read(cx).hovered_message.as_deref(),
                Some("98765"),
                "hovered row {:?}",
                row
            )
        });
        let actions = cx.debug_bounds("message-actions").unwrap();
        let timeline = cx.debug_bounds("timeline").unwrap();
        assert!((actions.right() - (timeline.right() - px(16.))).abs() < px(1.));
        let action = cx.debug_bounds("reply-message").unwrap();
        cx.simulate_mouse_move(action.center(), None, Modifiers::default());
        cx.simulate_click(action.center(), Modifiers::default());
        cx.update(|_, cx| assert_eq!(view.read(cx).reply_to.as_ref().unwrap().id, "98765"));
    }
    #[test]
    fn media_preview_dimensions_never_upscale_small_art() {
        use super::media_size;
        assert_eq!(media_size(Some((32, 24)), 400., 300.), (32., 24.));
        assert_eq!(media_size(Some((600, 1200)), 400., 300.), (150., 300.));
        assert_eq!(media_size(Some((800, 200)), 400., 300.), (400., 100.));
    }

    #[gpui::test]
    fn bot_section_with_button_stays_inside_its_message(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.store.clear_history();
            v.store.insert(serde_json::from_value(serde_json::json!({
                "id": "101", "channel_id": "campfire", "flags": 32768,
                "author": {"id": "bot", "username": ".fmbot", "bot": true},
                "components": [{
                    "type": 17, "accent_color": 16755249,
                    "components": [{
                        "type": 9,
                        "components": [{"type": 10, "content": "Not enough scrobbled albums (0 of required 9) in the day time period. Try a smaller chart or a bigger time period."}],
                        "accessory": {"type": 2, "style": 2, "label": "Edit", "custom_id": "edit"}
                    }]
                }]
            })).unwrap());
            v.store.insert(Message {
                id: "102".into(),
                channel_id: "campfire".into(),
                content: "next message".into(),
                author: User { id: "friend".into(), username: "friend".into(), ..Default::default() },
                ..Default::default()
            });
            v.refresh_messages();
            v.messages.scroll_to(gpui::ListOffset { item_ix: 0, offset_in_item: px(0.) });
            cx.notify();
        });
        for width in [1100., 860.] {
            cx.draw(point(px(0.), px(0.)), size(px(width), px(730.)), |_, _| {
                view.clone()
            });
            let message = cx.debug_bounds("message-row-101").unwrap();
            let card = cx.debug_bounds("component-card-101").unwrap();
            let next = cx.debug_bounds("message-row-102").unwrap();
            assert!(card.size.width >= px(300.) && card.right() <= message.right());
            assert!(
                card.size.height >= px(64.) && card.size.height <= px(176.),
                "{card:?}"
            );
            assert!(card.bottom() <= message.bottom() && message.bottom() <= next.top());
        }
    }

    #[gpui::test]
    fn gifv_uses_a_silent_inline_tile_instead_of_video_controls(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx,|v,cx|{v.store.clear_history();v.store.insert(serde_json::from_value(serde_json::json!({"id":"55","channel_id":"campfire","author":{"id":"x","username":"friend"},"embeds":[{"type":"gifv","video":{"url":"https://cdn.discordapp.com/test.gif.mp4","width":300,"height":200},"thumbnail":{"url":"https://cdn.discordapp.com/test.png","width":300,"height":200}}]})).unwrap());v.refresh_messages();cx.notify();});
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let gif = cx.debug_bounds("autoplay-gif").unwrap();
        cx.simulate_click(gif.center(), Modifiers::default());
        cx.update(|_, cx| assert!(view.read(cx).video.is_none()));
    }

    #[gpui::test]
    fn message_context_actions_close_without_touching_the_draft(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.settings.animations = false;
            v.composer
                .update(cx, |input, cx| input.set("draft stays", cx));
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let row = cx.debug_bounds("message-row-11").unwrap();
        cx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::default());
        let target = cx.debug_bounds("message-action-3").unwrap();
        cx.simulate_click(target.center(), Modifiers::default());
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert!(v.message_menu.is_none());
            assert_eq!(v.composer.read(cx).content.as_ref(), "draft stays");
            assert!(v.sending.is_none());
        });
    }
    #[gpui::test]
    fn mutual_profile_tabs_show_their_own_lists(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.settings.animations = true;
            let friend = User {
                id: "123".into(),
                username: "mutual friend".into(),
                ..Default::default()
            };
            v.profile = Some(Profile {
                user: User {
                    id: "456".into(),
                    username: "profile".into(),
                    ..Default::default()
                },
                mutual_friends: vec![friend],
                mutual_guilds: Some(vec![crate::model::MutualGuild {
                    id: v.store.guilds[0].id.clone(),
                }]),
                ..Default::default()
            });
            cx.notify();
        });
        let viewport = cx.update(|window, _| window.viewport_size());
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
        let identity = cx.debug_bounds("profile-identity").unwrap();
        let card = cx.debug_bounds("profile-card").unwrap();
        let tabs = cx.debug_bounds("profile-tabs").unwrap();
        let body = cx.debug_bounds("profile-tab-body").unwrap();
        view.update(cx, |v, cx| {
            v.profile.as_mut().unwrap().badges.extend((0..16).map(|n| {
                crate::model::ProfileBadge {
                    id: format!("badge-{n}"),
                    description: "late badge".into(),
                    ..Default::default()
                }
            }));
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
        assert_eq!(
            cx.debug_bounds("profile-tabs").unwrap(),
            tabs,
            "late badges moved the tabs"
        );
        for tab in [1, 2, 0, 2, 1, 0] {
            let bounds = cx
                .debug_bounds(match tab {
                    0 => "profile-tab-0",
                    1 => "profile-tab-1",
                    _ => "profile-tab-2",
                })
                .unwrap();
            cx.simulate_mouse_move(bounds.center(), None, Modifiers::default());
            cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
            assert_eq!(
                cx.debug_bounds("profile-tabs").unwrap(),
                tabs,
                "hover moved the tabs"
            );
            cx.simulate_click(bounds.center(), Modifiers::default());
            cx.draw(point(px(0.), px(0.)), viewport, |_, _| view.clone());
            cx.update(|_, cx| assert_eq!(view.read(cx).profile_tab, tab));
            assert_eq!(
                cx.debug_bounds("profile-tab-body").unwrap(),
                body,
                "tab contents resized the viewport"
            );
            assert_eq!(cx.debug_bounds("profile-card").unwrap(), card);
            assert_eq!(cx.debug_bounds("profile-identity").unwrap(), identity);
        }
    }

    #[gpui::test]
    fn group_icon_updates_keep_the_group_identity_and_recipients(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window.update(cx,|view,window,cx| {
            view.dispatch("CHANNEL_CREATE",serde_json::json!({"id":"123","type":3,"name":"group","icon":"abc","recipients":[{"id":"456","username":"friend"}]}),window,cx);
            view.dispatch("CHANNEL_UPDATE",serde_json::json!({"id":"123","icon":"def"}),window,cx);
            let channel=&view.store.channels["123"];
            assert_eq!(channel.kind,3);assert_eq!(channel.label(),"group");assert_eq!(channel.recipients.len(),1);
            assert_eq!(crate::media::group_icon(&channel.id,channel.icon.as_deref()).unwrap(),"https://cdn.discordapp.com/channel-icons/123/def.webp?size=128");
            view.dispatch("CHANNEL_UPDATE",serde_json::json!({"id":"123","icon":null}),window,cx);
            assert!(view.store.channels["123"].icon.is_none());
        }).unwrap();
    }

    #[gpui::test]
    fn a_profile_message_action_opens_a_dm_and_preserves_the_old_draft(cx: &mut TestAppContext) {
        use gpui::{Modifiers, point, px, size};
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |view, cx| {
            view.composer
                .update(cx, |input, cx| input.set("keep this server draft", cx));
            view.profile = Some(crate::model::Profile {
                user: view.store.users["moss"].clone(),
                ..Default::default()
            });
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let target = cx.debug_bounds("profile-message").unwrap().center();
        cx.simulate_click(target, Modifiers::default());
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.profile.is_none());
            assert_eq!(view.selected_channel.as_deref(), Some("sample-dm-moss"));
            assert_eq!(view.drafts["campfire"], "keep this server draft");
        });
    }

    #[gpui::test]
    fn compact_chrome_aligns_with_the_conversation_and_its_controls_stay_clickable(
        cx: &mut TestAppContext,
    ) {
        use gpui::{Modifiers, point, px, size};
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |view, cx| {
            // exercise the custom platform controls even when this test runs on macos.
            let _ = view.window_controls(cx);
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let chrome = cx.debug_bounds("window-chrome").unwrap();
        let shell = cx.debug_bounds("conversation-shell").unwrap();
        let header = cx.debug_bounds("channel-header").unwrap();
        let settings = cx.debug_bounds("nav-settings").unwrap();
        assert_eq!(chrome.size.height, px(36.));
        assert_eq!(shell.origin.y, chrome.bottom());
        assert_eq!(header.size.height, px(48.));
        assert!(settings.origin.y >= chrome.origin.y && settings.bottom() <= chrome.bottom());
        cx.simulate_click(settings.center(), Modifiers::default());
        cx.update(|_, cx| assert!(view.read(cx).settings_tab.is_some()));
    }

    #[gpui::test]
    fn latest_button_floats_inside_the_timeline_without_shrinking_it(cx: &mut TestAppContext) {
        use gpui::{point, px, size};
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |view, cx| {
            view.scrolled_away = true;
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let button = cx.debug_bounds("jump-latest").unwrap();
        let timeline = cx.debug_bounds("timeline").unwrap();
        assert!(button.bottom() < timeline.bottom());
        assert!(timeline.bottom() - button.bottom() <= px(8.));
        view.update(cx, |view, cx| {
            view.scrolled_away = false;
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        assert_eq!(timeline, cx.debug_bounds("timeline").unwrap());
    }

    #[gpui::test]
    fn reply_submission_clears_before_the_next_draft_and_keeps_its_reference(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                let message = view.store.channel_messages("campfire")[0].clone();
                view.reply_to_message(&message, window, cx);
                view.composer
                    .update(cx, |input, cx| input.set("first reply", cx));
                view.send(window, cx);
                assert!(view.composer.read(cx).content.is_empty());
                view.composer
                    .update(cx, |input, cx| input.insert("next draft", cx));
                assert_eq!(view.composer.read(cx).content.as_ref(), "next draft");
                assert_eq!(
                    view.store
                        .channel_messages("campfire")
                        .last()
                        .unwrap()
                        .message_reference
                        .as_ref()
                        .unwrap()["message_id"],
                    message.id
                );
                assert!(view.reply_to.is_none());
            })
            .unwrap();
    }

    #[gpui::test]
    fn emoji_picker_inserts_at_the_cursor_and_blocks_the_composer(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.composer
                    .update(cx, |input, cx| input.set("hello ", cx));
                view.emoji_open = true;
                let before = view.store.cached_messages();
                view.send(window, cx);
                assert_eq!(view.store.cached_messages(), before);
                view.composer.update(cx, |input, cx| input.insert("🫶", cx));
                assert_eq!(view.composer.read(cx).content.as_ref(), "hello 🫶");
            })
            .unwrap();
    }

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
    fn friends_home_preserves_drafts_and_does_not_read_the_hidden_chat(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window.update(cx,|view,window,cx| {
            view.settings.notifications = false;
            view.composer.update(cx,|input,cx|input.set("unsent campfire draft",cx));
            let before = view.store.cached_messages();
            view.show_friends(window,cx);
            view.send(window,cx);
            assert_eq!(view.store.cached_messages(),before);
            view.dispatch("MESSAGE_CREATE",serde_json::json!({"id":"9999","channel_id":"campfire","author":{"id":"someone","username":"someone"},"content":"a synthetic incoming message"}),window,cx);
            assert!(view.store.unread.contains("campfire"));
            let user = view.store.users["moss"].clone();
            view.open_friend_dm(user,window,cx);
            assert!(!view.home_page);
            assert_eq!(view.selected_channel.as_deref(),Some("sample-dm-moss"));
            assert_eq!(view.drafts.get("campfire").map(String::as_str),Some("unsent campfire draft"));
            assert!(view.composer.read(cx).content.is_empty());
        }).unwrap();
    }

    #[gpui::test]
    fn profile_inspector_keeps_chat_interactive_but_full_profile_blocks_sending(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.open_profile(view.store.user.clone(), window, cx);
                view.profile_inspector = true;
                let before = view.store.cached_messages();
                view.composer
                    .update(cx, |input, cx| input.set("offline inspector draft", cx));
                view.send(window, cx);
                assert_eq!(view.store.cached_messages(), before + 1);
                assert!(view.profile.is_some());
                assert!(view.composer.read(cx).content.is_empty());
                view.profile_inspector = false;
                view.composer
                    .update(cx, |input, cx| input.set("keep this draft", cx));
                view.send(window, cx);
                assert_eq!(view.store.cached_messages(), before + 1);
                assert_eq!(view.composer.read(cx).content.as_ref(), "keep this draft");
            })
            .unwrap();
    }

    #[gpui::test]
    fn profile_inspector_reserves_a_side_column_and_scales_the_card(cx: &mut TestAppContext) {
        use gpui::{point, px, size};
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |view, cx| {
            view.profile = Some(crate::model::Profile {
                user: view.store.user.clone(),
                ..Default::default()
            });
            view.profile_inspector = true;
            view.members_open = false;
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let card = cx.debug_bounds("profile-card").unwrap();
        let timeline = cx.debug_bounds("timeline").unwrap();
        assert_eq!(card.size.width, px(296.));
        assert!(timeline.right() <= card.origin.x);
        assert!(timeline.size.width >= px(400.));
        assert!(card.size.height >= px(450.));
        view.update(cx, |view, cx| {
            view.profile.as_mut().unwrap().frame_asset = Some(crate::effects::ProfileFrame {
                inner_width: 440.,
                overflow_horizontal: 44.,
                overflow_top: 60.,
                overflow_bottom: 20.,
                layers: vec![crate::effects::FrameLayer {
                    id: "sample".into(),
                    kind: "staple".into(),
                    ..Default::default()
                }],
                ..Default::default()
            });
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let card = cx.debug_bounds("profile-card").unwrap();
        let column = cx.debug_bounds("profile-side-column").unwrap();
        let horizontal_art = card.size.width * 0.1;
        let top_art = card.size.width * (60. / 440.);
        assert!(card.origin.x - horizontal_art >= column.origin.x + px(10.));
        assert!(card.right() + horizontal_art <= column.right() - px(10.));
        assert!(card.origin.y - top_art >= column.origin.y + px(10.));
    }

    #[gpui::test]
    fn ready_prioritizes_friends_before_unrelated_users_in_the_identity_cache(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                let mut users = (0..2000)
                    .map(|id| serde_json::json!({"id":format!("other-{id}"),"username":"other"}))
                    .collect::<Vec<_>>();
                users.push(serde_json::json!({"id":"friend","username":"friend"}));
                view.dispatch(
                    "READY",
                    serde_json::json!({"users":users,"relationships":[{"id":"friend","type":1}]}),
                    window,
                    cx,
                );
                assert_eq!(view.store.users.len(), 2000);
                assert_eq!(view.store.users["friend"].username, "friend");
            })
            .unwrap();
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
                view.jump_latest(window, cx);
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
