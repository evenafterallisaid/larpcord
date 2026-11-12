use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use gpui::{div, list, prelude::*, px, rgb, *};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    input::{Changed, Submit, TextInput},
    model::{Channel, Message, Store},
    network::{Backend, Command, Envelope, Event, guild_channels, guild_from_ready},
    settings::Settings,
};

const BG: u32 = 0x15191f;
const PANEL: u32 = 0x101419;
const SURFACE: u32 = 0x1e242c;
const LINE: u32 = 0x2a313b;
const TEXT: u32 = 0xe3e7ee;
const MUTED: u32 = 0x8f99a8;
const MINT: u32 = 0xb8ebd0;
const LILAC: u32 = 0xc1b5ed;

actions!(larpcord, [Quit, OpenSettings, ClosePanel, FocusSearch]);

pub struct Larpcord {
    backend: Backend,
    store: Store,
    settings: Settings,
    generation: u64,
    preview: bool,
    status: String,
    selected_guild: Option<String>,
    selected_channel: Option<String>,
    composer: Entity<TextInput>,
    search: Entity<TextInput>,
    token: Entity<TextInput>,
    messages: ListState,
    rendered_ids: Vec<String>,
    settings_tab: Option<usize>,
    error: Option<String>,
    loading: HashSet<String>,
    drafts: HashMap<String, String>,
    sending: Option<(String, String, String)>,
    _subscriptions: Vec<Subscription>,
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
        .text_color(rgb(MINT))
        .child(text)
}
fn button(id: &'static str, text: &'static str) -> Stateful<Div> {
    row()
        .id(id)
        .gap_2()
        .px_4()
        .py_2()
        .rounded_lg()
        .cursor_pointer()
        .bg(rgb(SURFACE))
        .text_size(px(13.))
        .hover(|s| s.bg(rgb(LINE)))
        .child(text)
}

impl Larpcord {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (backend, events) = Backend::start();
        let composer = cx.new(|cx| TextInput::new("Message the campfire", false, cx));
        let search = cx.new(|cx| TextInput::new("Find a conversation", false, cx));
        let token = cx.new(|cx| TextInput::new("Paste a session token", true, cx));
        let subscriptions = vec![
            cx.subscribe_in(&composer, window, |this, _, _: &Submit, window, cx| {
                this.send(window, cx)
            }),
            cx.subscribe_in(&token, window, |this, _, _: &Submit, window, cx| {
                this.login(window, cx)
            }),
            cx.subscribe(&search, |_, _, _: &Changed, cx| cx.notify()),
        ];
        cx.spawn_in(window, async move |view, cx| {
            while let Ok(event) = events.recv().await {
                if view
                    .update_in(cx, |view, window, cx| view.receive(event, window, cx))
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
            backend,
            store: Store::demo(),
            settings: Settings::load(),
            generation: 0,
            preview: true,
            status: "Preview".into(),
            selected_guild: Some("larp".into()),
            selected_channel: Some("campfire".into()),
            composer,
            search,
            token,
            messages: ListState::new(4, ListAlignment::Bottom, px(300.)),
            rendered_ids: vec!["1".into(), "2".into(), "3".into(), "4".into()],
            settings_tab: None,
            error: None,
            loading: HashSet::new(),
            drafts: HashMap::new(),
            sending: None,
            _subscriptions: subscriptions,
        }
    }

    fn command(&mut self, command: Command) {
        if let Err(error) = self.backend.command(command) {
            self.error = Some(error);
        }
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        if self.settings.save().is_err() {
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

    fn connect(&mut self, token: Zeroizing<String>, cx: &mut Context<Self>) {
        self.generation += 1;
        self.preview = false;
        self.store = Store::default();
        self.selected_guild = None;
        self.selected_channel = None;
        self.loading.clear();
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
        self.drafts.clear();
        self.composer.update(cx, |input, cx| input.set("", cx));
        self.messages.reset(4);
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
        self.selected_channel = Some(id.clone());
        self.store.pin(&id);
        self.store.unread.remove(&id);
        self.error = None;
        let draft = self.drafts.get(&id).cloned().unwrap_or_default();
        self.composer.update(cx, |input, cx| input.set(&draft, cx));
        self.messages.reset(self.store.channel_messages(&id).len());
        self.rendered_ids = self
            .store
            .channel_messages(&id)
            .iter()
            .map(|m| m.id.clone())
            .collect();
        if !self.preview {
            self.load_history(None);
        }
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }

    fn load_history(&mut self, before: Option<String>) {
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        if self.loading.contains(&channel) || self.loading.len() >= 8 {
            return;
        }
        self.loading.insert(channel.clone());
        if let Err(error) = self.backend.command(Command::History {
            channel: channel.clone(),
            started: self.store.revision(),
            before,
        }) {
            self.loading.remove(&channel);
            self.error = Some(error);
        }
    }

    fn send(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.sending.is_some() {
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
                Ok(()) => self.sending = Some((nonce, channel, content)),
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
        if let Some(anchor) = anchor
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
            if self.generation == 0
                && let Some(token) = token
            {
                self.connect(token, cx);
            }
            return;
        }
        if envelope.generation != self.generation {
            return;
        }
        match envelope.event {
            Event::Status(status) => {
                self.status = status;
            }
            Event::Error(error) => {
                self.error = Some(error);
                self.loading.clear();
            }
            Event::History {
                channel,
                started,
                older,
                messages,
            } => {
                self.loading.remove(&channel);
                self.store.merge_history(&channel, messages, started, older);
                if self.selected_channel.as_ref() == Some(&channel) {
                    self.refresh_messages();
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
                            self.store.insert(message);
                            if self.drafts.get(&channel) == Some(&content) {
                                self.drafts.remove(&channel);
                            }
                            if self.selected_channel.as_ref() == Some(&channel)
                                && self.composer.read(cx).content.as_ref() == content
                            {
                                self.composer.update(cx, |input, cx| input.set("", cx));
                            }
                            self.refresh_messages();
                        }
                        Err(error) => {
                            self.error = Some(format!(
                                "{error} Your draft has been kept. Check the channel before sending again."
                            ))
                        }
                    }
                }
            }
            Event::Dispatch(kind, data) => self.dispatch(&kind, data, window, cx),
            Event::SavedSession(_) => {}
        }
        cx.notify();
    }

    fn dispatch(&mut self, kind: &str, data: Value, window: &mut Window, cx: &mut Context<Self>) {
        match kind {
            "READY" => {
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
                self.settings_tab = None;
                if self.selected_channel.is_none() {
                    let mut dms: Vec<_> = self
                        .store
                        .channels
                        .values()
                        .filter(|c| c.guild_id.is_none())
                        .map(|c| c.id.clone())
                        .collect();
                    dms.sort();
                    if let Some(id) = dms.first() {
                        self.choose_channel(id.clone(), window, cx);
                    }
                } else if let Some(id) = self.selected_channel.clone() {
                    if self.store.channels.contains_key(&id) {
                        self.load_history(None);
                    } else {
                        self.selected_channel = None;
                        self.selected_guild = None;
                        self.refresh_messages();
                    }
                }
            }
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
            "MESSAGE_CREATE" => {
                if let Ok(message) = serde_json::from_value::<Message>(data) {
                    let selected = self.selected_channel.as_ref() == Some(&message.channel_id);
                    let own = message.author.id == self.store.user.id;
                    if !own && !(selected && window.is_window_active()) {
                        self.store.unread.insert(message.channel_id.clone());
                        let dm = self
                            .store
                            .channels
                            .get(&message.channel_id)
                            .is_some_and(|c| c.guild_id.is_none());
                        let mentioned = message.mentions.iter().any(|u| u.id == self.store.user.id);
                        if self.settings.notifications && (dm || mentioned) {
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
                        self.store.insert(message);
                        self.refresh_messages();
                    }
                }
            }
            "MESSAGE_UPDATE" => {
                self.store.update(&data);
                self.refresh_messages();
            }
            "MESSAGE_DELETE" => {
                if let (Some(channel), Some(id)) =
                    (data["channel_id"].as_str(), data["id"].as_str())
                {
                    self.store.delete(channel, id);
                    self.refresh_messages();
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
                        self.store.delete(channel, id);
                    }
                    self.refresh_messages();
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
        if let Some(guild) = guild_from_ready(data) {
            if let Some(existing) = self.store.guilds.iter_mut().find(|g| g.id == guild.id) {
                *existing = guild;
            } else {
                self.store.guilds.push(guild);
            }
        }
        for channel in guild_channels(data) {
            self.store.channels.insert(channel.id.clone(), channel);
        }
    }

    fn titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        row()
            .h(px(42.))
            .flex_shrink_0()
            .px_4()
            .gap_2()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .when(cfg!(target_os = "macos"), |s| s.pl(px(86.)))
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                if event.click_count == 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
            })
            .child(svg().path("mark.svg").size(px(19.)).text_color(rgb(MINT)))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("larpcord"),
            )
            .child(
                div()
                    .ml_2()
                    .text_color(rgb(MUTED))
                    .text_size(px(11.))
                    .child("a little place to hang out"),
            )
            .child(div().flex_1())
            .child(badge("NATIVE PREVIEW"))
            .when(!cfg!(target_os = "macos"), |s| {
                s.children([
                    button("minimize", "−")
                        .on_click(|_, window, _| window.minimize_window())
                        .into_any_element(),
                    button("maximize", "□")
                        .on_click(|_, window, _| window.zoom_window())
                        .into_any_element(),
                    button("close-window", "×")
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
        let guilds = self.store.guilds.clone();
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
                    .size(px(44.))
                    .justify_center()
                    .rounded_xl()
                    .bg(rgb(if self.selected_guild.is_none() {
                        0x354b43
                    } else {
                        SURFACE
                    }))
                    .cursor_pointer()
                    .child(svg().path("mark.svg").size(px(25.)).text_color(rgb(MINT)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.selected_guild = None;
                        this.search.update(cx, |input, cx| input.set("", cx));
                        cx.notify();
                    })),
            )
            .child(div().w(px(26.)).h(px(1.)).bg(rgb(LINE)))
            .child(
                column()
                    .id("servers")
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .overflow_y_scroll()
                    .items_center()
                    .gap_3()
                    .children(guilds.into_iter().map(|guild| {
                        let id = guild.id.clone();
                        let selected = self.selected_guild.as_ref() == Some(&id);
                        let initials: String = guild
                            .name
                            .split_whitespace()
                            .take(2)
                            .filter_map(|s| s.chars().next())
                            .collect();
                        row()
                            .id(SharedString::from(format!("guild-{id}")))
                            .size(px(44.))
                            .justify_center()
                            .rounded_xl()
                            .cursor_pointer()
                            .bg(rgb(if selected { 0x424052 } else { SURFACE }))
                            .text_color(rgb(if selected { LILAC } else { MUTED }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .hover(|s| s.bg(rgb(LINE)))
                            .child(initials)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.selected_guild = Some(id.clone());
                                this.search.update(cx, |input, cx| input.set("", cx));
                                let mut channels: Vec<_> = this
                                    .store
                                    .channels
                                    .values()
                                    .filter(|c| c.guild_id.as_ref() == Some(&id) && c.readable())
                                    .cloned()
                                    .collect();
                                channels.sort_by(|a, b| {
                                    a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id))
                                });
                                if let Some(channel) = channels.first() {
                                    this.choose_channel(channel.id.clone(), window, cx);
                                }
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .size(px(7.))
                    .rounded_full()
                    .bg(rgb(if self.status == "Connected" {
                        MINT
                    } else {
                        MUTED
                    })),
            )
            .child(label(self.status.clone()).text_size(px(9.)))
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .selected_guild
            .as_ref()
            .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id))
            .map_or("Direct messages", |g| g.name.as_str())
            .to_string();
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
            a.position
                .cmp(&b.position)
                .then_with(|| a.label().cmp(&b.label()))
        });
        column().w(px(236.)).h_full().flex_shrink_0().bg(rgb(0x191e25)).border_r_1().border_color(rgb(LINE))
            .child(column().p_4().gap_3().child(div().font_weight(FontWeight::SEMIBOLD).truncate().child(title))
                .child(row().px_2().rounded_lg().bg(rgb(PANEL)).child(self.search.clone())))
            .child(row().px_4().pb_2().justify_between().child(label(if self.selected_guild.is_some() { "CONVERSATIONS" } else { "MESSAGES" }).text_size(px(10.)))
                .child(label(format!("{}", channels.len())).text_size(px(10.))))
            .child(column().id("channels").flex_1().min_h_0().px_2().gap_1().overflow_y_scroll().children(channels.into_iter().map(|channel| {
                let id = channel.id.clone();
                let selected = self.selected_channel.as_ref() == Some(&id);
                row().id(SharedString::from(format!("channel-{id}"))).px_3().py_2().gap_3().rounded_lg().cursor_pointer()
                    .bg(rgb(if selected { 0x2a333b } else { 0x191e25 })).hover(|s| s.bg(rgb(SURFACE)))
                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                    .child(div().text_size(px(17.)).text_color(rgb(if selected { MINT } else { MUTED })).child(if channel.kind == 2 { "◖" } else if channel.guild_id.is_none() { "@" } else { "#" }))
                    .child(div().flex_1().truncate().child(channel.label()))
                    .when(self.store.unread.contains(&id), |s| s.child(div().size(px(6.)).rounded_full().bg(rgb(MINT))))
                    .on_click(cx.listener(move |this, _, window, cx| this.choose_channel(id.clone(), window, cx)))
            })))
            .when(self.preview, |s| s.child(column().mx_3().my_3().p_3().rounded_lg().border_1().border_color(rgb(0x35473f)).bg(rgb(0x1b2924)).gap_2()
                .child(div().text_size(px(12.)).text_color(rgb(MINT)).child("Make yourself at home"))
                .child(label("You're exploring a sample workspace. Connect to see your conversations.").text_size(px(11.)))
                .child(button("connect-sidebar", "Connect account").bg(rgb(0x35473f)).text_color(rgb(MINT)).on_click(cx.listener(|this, _, _, cx| { this.settings_tab = Some(0); cx.notify(); })))))
            .child(row().h(px(68.)).px_3().gap_3().border_t_1().border_color(rgb(LINE))
                .child(row().size(px(34.)).rounded_full().justify_center().bg(rgb(0x3b394d)).text_color(rgb(LILAC)).child(self.store.user.name().chars().next().unwrap_or('l').to_string()))
                .child(column().flex_1().min_w_0().child(div().truncate().text_size(px(12.)).child(self.store.user.name().to_string())).child(label(if self.preview { "Just looking around" } else { "Connected account" }).text_size(px(10.))))
                .child(button("settings", "⚙").px_2().text_color(rgb(MUTED)).on_click(cx.listener(|this, _, _, cx| { this.settings_tab = Some(1); cx.notify(); }))))
    }

    fn message(&self, message: &Message, cx: &mut Context<Self>) -> AnyElement {
        let initial = message
            .author
            .name()
            .chars()
            .next()
            .unwrap_or('?')
            .to_string();
        let time = chrono::DateTime::parse_from_rfc3339(&message.timestamp)
            .ok()
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default();
        let attachments = message.attachments.clone();
        let mut content = message.content.clone();
        for user in &message.mentions {
            content = content
                .replace(&format!("<@{}>", user.id), &format!("@{}", user.name()))
                .replace(&format!("<@!{}>", user.id), &format!("@{}", user.name()));
        }
        row()
            .items_start()
            .gap_3()
            .px_6()
            .py(if self.settings.compact {
                px(5.)
            } else {
                px(10.)
            })
            .w_full()
            .child(
                row()
                    .size(px(34.))
                    .flex_shrink_0()
                    .justify_center()
                    .rounded_xl()
                    .bg(rgb(if message.author.id == self.store.user.id {
                        0x343347
                    } else {
                        0x293d35
                    }))
                    .text_color(rgb(if message.author.id == self.store.user.id {
                        LILAC
                    } else {
                        MINT
                    }))
                    .child(initial),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        row()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(message.author.name().to_string()),
                            )
                            .child(label(time).text_size(px(10.)))
                            .when(message.edited_timestamp.is_some(), |s| {
                                s.child(label("edited").text_size(px(10.)))
                            }),
                    )
                    .child(div().text_size(px(14.)).line_height(px(22.)).child(content))
                    .children(attachments.into_iter().map(|attachment| {
                        let url = attachment.url;
                        button("attachment", "↗")
                            .child(attachment.filename)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                if let Ok(url) = reqwest::Url::parse(&url)
                                    && url.scheme() == "https"
                                {
                                    cx.open_url(url.as_str());
                                }
                            }))
                    })),
            )
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
                    .h(px(65.))
                    .flex_shrink_0()
                    .px_6()
                    .gap_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(div().text_size(px(24.)).text_color(rgb(MINT)).child("#"))
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                            .child(label(topic).truncate().text_size(px(11.))),
                    )
                    .child(badge(if self.preview {
                        "SAMPLE WORKSPACE"
                    } else {
                        "TEXT CHANNEL"
                    })),
            )
            .when(channel.is_none(), |s| {
                s.child(
                    column()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(svg().path("mark.svg").size(px(70.)).text_color(rgb(MINT)))
                        .child(div().text_size(px(24.)).child("A little room to breathe."))
                        .child(label("Choose a conversation from the sidebar.")),
                )
            })
            .when(channel.is_some(), |s| {
                s.child(
                    row()
                        .px_6()
                        .pt_4()
                        .pb_2()
                        .gap_3()
                        .child(div().h(px(1.)).flex_1().bg(rgb(LINE)))
                        .child(
                            label(if self.preview {
                                "a fresh start"
                            } else {
                                "recent messages"
                            })
                            .text_size(px(10.)),
                        )
                        .child(div().h(px(1.)).flex_1().bg(rgb(LINE))),
                )
                .when(!self.preview, |s| {
                    s.child(
                        row().px_6().child(
                            button(
                                "older",
                                if selected
                                    .as_ref()
                                    .is_some_and(|id| self.loading.contains(id))
                                {
                                    "Loading…"
                                } else {
                                    "Load earlier messages"
                                },
                            )
                            .text_size(px(10.))
                            .py_1()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let before = this
                                    .selected_channel
                                    .as_deref()
                                    .and_then(|id| this.store.channel_messages(id).first())
                                    .map(|m| m.id.clone());
                                this.load_history(before);
                                cx.notify();
                            })),
                        ),
                    )
                })
                .child(
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
                            message.map_or_else(
                                || div().into_any_element(),
                                |message| view.message(&message, cx),
                            )
                        })
                    })
                    .flex_1()
                    .min_h_0(),
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
                                    .bg(rgb(0x33483e))
                                    .text_color(rgb(MINT))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.send(window, cx)),
                                    ),
                                ),
                        )
                        .child(
                            row()
                                .justify_between()
                                .px_1()
                                .child(
                                    label(if self.preview {
                                        "Preview messages stay in this window."
                                    } else {
                                        "Enter to send. Mentions are sent without pings."
                                    })
                                    .text_size(px(10.)),
                                )
                                .child(
                                    label(format!(
                                        "{} messages in memory",
                                        self.store.cached_messages()
                                    ))
                                    .text_size(px(10.)),
                                ),
                        ),
                )
            })
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
                    .bg(rgb(if enabled { 0x4b7361 } else { LINE }))
                    .when(enabled, |s| s.justify_end())
                    .child(div().size(px(16.)).rounded_full().bg(rgb(if enabled {
                        MINT
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
                        .child(button("login", "Connect account").bg(rgb(0x33483e)).text_color(rgb(MINT)).on_click(cx.listener(|this, _, window, cx| this.login(window, cx)))))
                        .child(label("This is an early, independent Discord client. Voice, video, quests and existing JavaScript mods are still pending."));
                } else {
                    body = body.child(
                        column()
                            .p_4()
                            .rounded_xl()
                            .bg(rgb(SURFACE))
                            .gap_3()
                            .child(div().child(self.store.user.name().to_string()))
                            .child(label(self.status.clone()))
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
                    );
            }
            _ => {
                body = body.child(svg().path("mark.svg").size(px(58.)).text_color(rgb(MINT)))
                    .child(div().text_size(px(30.)).child("larpcord"))
                    .child(label("0.1.0 • native preview"))
                    .child(div().text_size(px(16.)).child("A little room to breathe."))
                    .child(label("Built in Rust with GPUI. Inspired by Kawaicord's small-footprint approach."))
                    .child(column().mt_4().p_4().rounded_xl().bg(rgb(SURFACE)).gap_3()
                        .child(div().child(format!("{} messages cached", self.store.cached_messages())))
                        .child(label("Up to 12 conversations and 200 messages per conversation. Media stays unloaded. The connection runs independently of rendering."))
                        .child(button("clear-cache", "Clear message cache").on_click(cx.listener(|this, _, _, cx| { this.store.clear_history(); this.refresh_messages(); cx.notify(); }))))
                    .child(button("github", "Open GitHub ↗").on_click(|_, _, cx| cx.open_url("https://github.com/evenafterallisaid/larpcord")))
                    .child(label("Native voice, rich media, packaging and plugin adapters are future milestones. This preview supports text conversations."));
            }
        }
        column()
            .absolute()
            .inset_0()
            .bg(rgba(0x090c10dd))
            .items_center()
            .justify_center()
            .child(
                row()
                    .w(px(720.))
                    .h(px(530.))
                    .rounded_2xl()
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(BG))
                    .shadow_lg()
                    .child(
                        column()
                            .w(px(176.))
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
                                                MINT
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
                                    |this, _, _, cx| {
                                        this.settings_tab = None;
                                        cx.notify();
                                    },
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
            .into_any_element()
    }
}

impl Render for Larpcord {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        column()
            .relative()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(14.))
            .font_family(if cfg!(target_os = "macos") {
                ".AppleSystemUIFont"
            } else if cfg!(target_os = "windows") {
                "Segoe UI"
            } else {
                "DejaVu Sans"
            })
            .on_action(|_: &Quit, _, cx| cx.quit())
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                this.settings_tab = Some(1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClosePanel, _, cx| {
                this.settings_tab = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                window.focus(&this.search.focus_handle(cx))
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
    }
}
