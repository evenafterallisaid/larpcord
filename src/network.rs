use std::{collections::HashMap, sync::Arc, time::Duration};

use async_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex, mpsc, watch},
    task::JoinSet,
    time::{Instant, sleep, sleep_until},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message as Frame, protocol::WebSocketConfig},
};
use zeroize::Zeroizing;

use crate::model::{Channel, Guild, Member, Message, Page, Profile, User};

pub enum Event {
    Reacted {
        channel: String,
        message: String,
        result: Result<(), String>,
    },
    EmojiRecents(Result<crate::gif_favorites::Recents, String>),
    StickerDetails(Vec<crate::expressions::Sticker>),
    ProfileBadges {
        id: String,
        guild: Option<String>,
        request: u64,
        badges: Vec<crate::model::ProfileBadge>,
    },
    GifFavorites(Result<Vec<crate::expressions::Gif>, String>),
    Gifs {
        request: u64,
        result: Result<Vec<crate::expressions::Gif>, String>,
    },
    StickerPacks(Result<Vec<crate::expressions::StickerPack>, String>),
    GuildExpressions {
        guild: String,
        emojis: Option<Vec<crate::expressions::Emoji>>,
        stickers: Option<Vec<crate::expressions::Sticker>>,
    },
    OpenedDm {
        user: String,
        result: Result<Channel, String>,
    },
    Auth {
        attempt: u64,
        event: crate::auth::AuthEvent,
    },
    Status(String),
    Dispatch(String, Value),
    History {
        channel: String,
        started: u64,
        request: u64,
        page: Page,
        result: Result<Vec<Message>, String>,
    },
    Sent {
        nonce: String,
        result: Result<Box<Message>, String>,
    },
    Profile {
        id: String,
        guild: Option<String>,
        request: u64,
        result: Result<Box<Profile>, String>,
    },
    ProfileEffect {
        id: String,
        guild: Option<String>,
        request: u64,
        effect: Option<Box<crate::effects::Effect>>,
        frame: Option<Box<crate::effects::ProfileFrame>>,
    },
    Interaction {
        message: String,
        result: Result<(), String>,
    },
    GuildDetails {
        id: String,
        started: std::time::Instant,
        result: Result<Guild, String>,
    },
    Error(String),
    Layout(Result<Value, String>),
    SavedSession(Option<Zeroizing<String>>),
}

pub struct Envelope {
    pub generation: u64,
    pub event: Event,
}

pub enum Command {
    React {
        channel: String,
        message: String,
        emoji: String,
        remove: bool,
    },
    EmojiRecents,
    StickerDetails(Vec<String>),
    GifFavorites(Option<(crate::expressions::Gif, bool)>),
    CancelGifs,
    Gifs {
        request: u64,
        query: String,
    },
    StickerPacks,
    GuildExpressions(String),
    OpenDm(String),
    QrStart {
        attempt: u64,
    },
    QrCancel,
    Connect {
        generation: u64,
        token: Zeroizing<String>,
        remember: bool,
    },
    Disconnect,
    History {
        channel: String,
        started: u64,
        request: u64,
        page: Page,
        cursor: Option<String>,
    },
    Send {
        channel: String,
        content: String,
        nonce: String,
        reply: Option<String>,
        sticker: Option<String>,
    },
    Notify {
        title: String,
        body: String,
    },
    Profile {
        id: String,
        guild: Option<String>,
        request: u64,
        roles_needed: bool,
    },
    Typing(String),
    Ack {
        channel: String,
        message: String,
    },
    ViewGuild(Option<String>),
    ViewMembers {
        guild: String,
        channel: String,
        bucket: usize,
        enabled: bool,
    },
    Interact {
        message: String,
        channel: String,
        guild: Option<String>,
        application: String,
        flags: u64,
        data: Value,
        nonce: String,
    },
    GuildDetails(String, std::time::Instant),
    Layout,
    Forget,
}

pub struct Backend {
    tx: mpsc::Sender<Command>,
}

#[derive(Default)]
struct EffectCache {
    by_id: HashMap<String, crate::effects::Effect>,
    attempted: Option<Instant>,
}

impl EffectCache {
    fn remember(&mut self, id: String, effect: crate::effects::Effect) {
        if self.by_id.len() >= 1024
            && !self.by_id.contains_key(&id)
            && let Some(old) = self.by_id.keys().next().cloned()
        {
            self.by_id.remove(&old);
        }
        self.by_id.insert(id, effect);
    }
}

#[derive(Clone)]
struct Api {
    client: Client,
    #[cfg(test)]
    endpoint: Option<String>,
    token: Arc<Zeroizing<String>>,
    gate: Arc<Mutex<Instant>>,
    discovery_gate: Arc<Mutex<Instant>>,
    effects: Arc<Mutex<EffectCache>>,
    session_id: Arc<std::sync::Mutex<Option<String>>>,
    reads: Arc<Mutex<ReadSync>>,
}

#[derive(Default)]
struct ReadSync {
    token: Option<String>,
    channels: HashMap<String, String>,
}
struct RequestFailure {
    message: String,
    retryable: bool,
}

impl RequestFailure {
    fn retry(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }
}
impl From<String> for RequestFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            retryable: false,
        }
    }
}
impl From<&str> for RequestFailure {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

fn retry_delay(attempt: u32) -> Duration {
    Duration::from_secs(2u64.saturating_pow(attempt.min(5)).min(30))
}

impl Api {
    async fn validate(&self, events: &Sender<Envelope>, generation: u64) -> bool {
        let mut attempt = 0;
        loop {
            match self
                .request_checked::<Value>(Method::GET, "/users/@me", None)
                .await
            {
                Ok(_) => return true,
                Err(error) if error.retryable => {
                    emit(events, generation, Event::Status("Reconnecting".into())).await;
                    emit(
                        events,
                        generation,
                        Event::Error("Cannot reach Discord. Retrying automatically…".into()),
                    )
                    .await;
                    sleep(retry_delay(attempt)).await;
                    attempt = attempt.saturating_add(1);
                }
                Err(error) => {
                    emit(
                        events,
                        generation,
                        Event::Status("Connection failed".into()),
                    )
                    .await;
                    emit(events, generation, Event::Error(error.message)).await;
                    return false;
                }
            }
        }
    }
    fn new(token: Zeroizing<String>) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!(
                "Larpcord/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/evenafterallisaid/larpcord)"
            ))
            .build()
            .map_err(|_| "Could not initialize the network client.".to_string())?;
        Ok(Self {
            client,
            #[cfg(test)]
            endpoint: None,
            token: Arc::new(token),
            gate: Arc::new(Mutex::new(Instant::now())),
            discovery_gate: Arc::new(Mutex::new(Instant::now())),
            effects: Arc::new(Mutex::new(EffectCache::default())),
            session_id: Arc::new(std::sync::Mutex::new(None)),
            reads: Arc::new(Mutex::new(ReadSync::default())),
        })
    }

    async fn effect(&self, id: &str) -> Option<crate::effects::Effect> {
        if !snowflake(id) {
            return None;
        }
        let mut cache = self.effects.lock().await;
        if let Some(effect) = cache.by_id.get(id) {
            return Some(effect.clone());
        }
        if let Ok(value) = self
            .request::<Value>(Method::GET, &format!("/collectibles-products/{id}"), None)
            .await
        {
            for (id, effect) in crate::effects::catalog(&value) {
                cache.remember(id, effect);
            }
        }
        if let Some(effect) = cache.by_id.get(id) {
            return Some(effect.clone());
        }
        if cache
            .attempted
            .is_none_or(|at| at.elapsed() > Duration::from_secs(300))
        {
            cache.attempted = Some(Instant::now());
            if let Ok(value) = self
                .request::<Value>(
                    Method::GET,
                    "/collectibles-categories?include_unpublished=false",
                    None,
                )
                .await
            {
                for (id, effect) in crate::effects::catalog(&value) {
                    cache.remember(id, effect);
                }
            }
        }
        cache.by_id.get(id).cloned()
    }

    fn discovery(&self) -> Self {
        let mut api = self.clone();
        api.gate = self.discovery_gate.clone();
        api
    }

    async fn gif_favorites(
        &self,
        change: Option<(crate::expressions::Gif, bool)>,
    ) -> Result<Vec<crate::expressions::Gif>, String> {
        let path = "/users/@me/settings-proto/2";
        let current = crate::gif_favorites::Favorites::decode(
            &self.request::<Value>(Method::GET, path, None).await?,
        )?;
        if let Some((gif, save)) = change {
            if !crate::expressions::gif_url(&gif.url) && !crate::media::cdn_url(&gif.url) {
                return Err("Unsupported GIF address".into());
            }
            if current.contains(&gif.url) != save {
                let patch = current.patch(&gif, save)?;
                let saved = crate::gif_favorites::Favorites::decode(
                    &self
                        .request::<Value>(
                            Method::PATCH,
                            path,
                            Some(json!({"settings":patch,"required_data_version":current.version})),
                        )
                        .await?,
                )?;
                if !saved.confirms(&current, &gif.url, save) {
                    return Err(
                        "Discord did not confirm the favorite change. Refresh before trying again."
                            .into(),
                    );
                }
                return Ok(saved.gifs());
            }
        }
        Ok(current.gifs())
    }
    async fn react(
        &self,
        channel: &str,
        message: &str,
        emoji: &str,
        remove: bool,
    ) -> Result<(), String> {
        if !snowflake(channel) || !snowflake(message) || emoji.is_empty() || emoji.len() > 128 {
            return Err("Invalid reaction".into());
        }
        let mut url = reqwest::Url::parse("https://discord.com/").unwrap();
        url.path_segments_mut().unwrap().push(emoji);
        self.request::<Value>(
            if remove { Method::DELETE } else { Method::PUT },
            &format!(
                "/channels/{channel}/messages/{message}/reactions/{}/@me?type=0",
                url.path().trim_start_matches('/')
            ),
            None,
        )
        .await
        .map(|_| ())
    }
    async fn gifs(&self, query: &str) -> Result<Vec<crate::expressions::Gif>, String> {
        let options = "media_format=gif&provider=klipy&locale=en-US&limit=50";
        let path = if query.is_empty() {
            format!("/gifs/trending-gifs?{options}")
        } else {
            let encoded: String =
                reqwest::Url::parse_with_params("https://discord.com/", [("q", query)])
                    .map_err(|_| "Invalid search".to_string())?
                    .query()
                    .unwrap_or_default()
                    .into();
            format!("/gifs/search?{encoded}&{options}")
        };
        self.request(Method::GET, &path, None)
            .await
            .map(crate::expressions::gifs)
    }

    async fn ack(&self, channel: &str, message: &str) -> Result<(), String> {
        let mut reads = self.reads.lock().await;
        if reads.channels.get(channel).is_some_and(|old| {
            old.parse::<u64>().unwrap_or(0) >= message.parse::<u64>().unwrap_or(0)
        }) {
            return Ok(());
        }
        let result: Value = self
            .request(
                Method::POST,
                &format!("/channels/{channel}/messages/{message}/ack"),
                Some(json!({"token":reads.token})),
            )
            .await?;
        reads.token = result["token"].as_str().map(str::to_string);
        if reads.channels.len() >= 512 {
            reads.channels.clear();
        }
        reads.channels.insert(channel.into(), message.into());
        Ok(())
    }

    async fn profile(&self, id: &str, guild: Option<&str>) -> Result<Profile, String> {
        let mut path = format!(
            "/users/{id}/profile?with_mutual_guilds=true&with_mutual_friends=true&with_mutual_friends_count=true"
        );
        if let Some(guild) = guild {
            path.push_str(&format!("&guild_id={guild}"));
        }
        match self.request::<Profile>(Method::GET, &path, None).await {
            Ok(profile) if profile.user.id == id => Ok(profile),
            _ => {
                let user: User = self
                    .request(Method::GET, &format!("/users/{id}"), None)
                    .await?;
                let guild_member = match guild {
                    Some(guild) => self
                        .request::<Member>(
                            Method::GET,
                            &format!("/guilds/{guild}/members/{id}"),
                            None,
                        )
                        .await
                        .ok(),
                    None => None,
                };
                Ok(Profile {
                    user,
                    guild_member,
                    ..Profile::default()
                })
            }
        }
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, String> {
        self.request_checked(method, path, body)
            .await
            .map_err(|error| error.message)
    }

    fn endpoint(&self, path: &str) -> String {
        #[cfg(test)]
        if let Some(endpoint) = &self.endpoint {
            return format!("{endpoint}{path}");
        }
        format!("https://discord.com/api/v10{path}")
    }

    async fn request_checked<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, RequestFailure> {
        // one request lane observes retry-after and avoids a burst when switching channels.
        let mut next = self.gate.lock().await;
        for _ in 0..3 {
            sleep_until(*next).await;
            let mut request = self
                .client
                .request(method.clone(), self.endpoint(path))
                .header("Authorization", self.token.as_str());
            if path.ends_with("/typing") {
                request = request.timeout(Duration::from_secs(3));
            }
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(|_| {
                RequestFailure::retry("The request could not be completed. Check your connection.")
            })?;
            let status = response.status();
            let exhausted = response
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|v| v == "0");
            let reset = response
                .headers()
                .get("x-ratelimit-reset-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            if exhausted {
                *next = Instant::now() + safe_delay(reset);
            }
            if status.as_u16() == 429 {
                let retry = response
                    .json::<Value>()
                    .await
                    .ok()
                    .and_then(|v| v["retry_after"].as_f64())
                    .unwrap_or(5.0);
                *next = Instant::now() + safe_delay(retry);
                continue;
            }
            if status.as_u16() == 401 {
                return Err("Your session has expired. Sign in again.".into());
            }
            if status.as_u16() == 403 {
                return Err("Discord did not allow access to this channel or action.".into());
            }
            if !status.is_success() {
                let message = format!(
                    "Discord returned HTTP {}. Try again shortly.",
                    status.as_u16()
                );
                return Err(if status.is_server_error() || status.as_u16() == 408 {
                    RequestFailure::retry(message)
                } else {
                    message.into()
                });
            }
            let mut response = response;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| RequestFailure::retry("The response was interrupted."))?
            {
                if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                    return Err("The response exceeded the client memory limit.".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            if bytes.is_empty() {
                bytes.extend_from_slice(b"null");
            }
            return serde_json::from_slice(&bytes)
                .map_err(|_| "Discord returned an unexpected response.".into());
        }
        Err(RequestFailure::retry(
            "Discord is rate limiting this session. Wait a moment and try again.",
        ))
    }
}

fn safe_delay(seconds: f64) -> Duration {
    Duration::from_secs_f64(if seconds.is_finite() {
        seconds.clamp(0.1, 300.0)
    } else {
        5.0
    })
}

fn send_body(content: &str, nonce: &str, reply: Option<&str>, sticker: Option<&str>) -> Value {
    let mut body = json!({"content":content,"nonce":nonce,"enforce_nonce":true,"allowed_mentions":{"parse":[],"replied_user":false}});
    if let Some(id) = reply.filter(|id| snowflake(id)) {
        body["message_reference"] = json!({"message_id":id,"fail_if_not_exists":false});
    }
    if let Some(id) = sticker.filter(|id| snowflake(id)) {
        body["sticker_ids"] = json!([id]);
    }
    body
}
fn snowflake(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

async fn emit(events: &Sender<Envelope>, generation: u64, event: Event) {
    let _ = events.send(Envelope { generation, event }).await;
}

impl Backend {
    pub fn start(load_saved_session: bool) -> (Self, async_channel::Receiver<Envelope>) {
        let (tx, mut commands) = mpsc::channel::<Command>(32);
        let (events, receiver) = async_channel::bounded(256);
        std::thread::Builder::new().name("larpcord-network".into()).spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("network runtime");
            runtime.block_on(async move {
                let mut tasks = JoinSet::new();
                let mut requests = JoinSet::new();
                let mut gif_request:Option<tokio::task::AbortHandle>=None;
                let mut auth = JoinSet::new();
                let mut api = None::<Api>;
                let mut generation = 0;
                let (viewed_guild, viewed_rx) = watch::channel::<Viewed>(Viewed::default());
                let saved_events = events.clone();
                if load_saved_session { tokio::task::spawn_blocking(move || {
                    let token = credential().ok().and_then(|entry| entry.get_password().ok()).map(Zeroizing::new);
                    let _ = saved_events.send_blocking(Envelope { generation: 0, event: Event::SavedSession(token) });
                }); }
                loop {
                    tokio::select! {
                        Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
                        Some(_) = requests.join_next(), if !requests.is_empty() => {}
                        Some(_) = auth.join_next(), if !auth.is_empty() => {}
                        command = commands.recv() => {
                            let Some(command) = command else { break; };
                            match command {
                                Command::QrStart { attempt } => {
                                    auth.abort_all();
                                    let events = events.clone();
                                    auth.spawn(async move {
                                        let (tx, rx) = async_channel::bounded(8);
                                        let forward = async {
                                            while let Ok(event) = rx.recv().await {
                                                emit(&events, generation, Event::Auth { attempt, event }).await;
                                            }
                                        };
                                        tokio::join!(crate::auth::remote_login(tx), forward);
                                    });
                                }
                                Command::QrCancel => auth.abort_all(),
                                Command::Connect { generation: new_generation, token, remember } => {
                                    auth.abort_all();
                                    tasks.abort_all();
                                    requests.abort_all();
                                    generation = new_generation;
                                    let _ = viewed_guild.send(Viewed::default());
                                    match Api::new(token) {
                                        Ok(client) => {
                                            api = Some(client.clone());
                                            let events = events.clone();
                                            let viewed = viewed_rx.clone();
                                            tasks.spawn(async move {
                                                // validate before saving a mistyped or expired credential.
                                                if !client.validate(&events, generation).await { return; }
                                                let token = client.token.clone();
                                                let save = tokio::task::spawn_blocking(move || {
                                                    let entry = credential().map_err(|_| ())?;
                                                    if remember { entry.set_password(token.as_str()).map_err(|_| ()) } else {
                                                        match entry.delete_credential() { Ok(()) | Err(keyring::Error::NoEntry) => Ok(()), Err(_) => Err(()) }
                                                    }
                                                }).await;
                                                if !matches!(save, Ok(Ok(()))) {
                                                    emit(&events, generation, Event::Error("Connected, but the credential store could not be updated. This session will remain in memory.".into())).await;
                                                }
                                                gateway(client, events, generation, viewed).await;
                                            });
                                        }
                                        Err(error) => emit(&events, generation, Event::Error(error)).await,
                                    }
                                }
                                Command::Disconnect => { tasks.abort_all(); requests.abort_all(); auth.abort_all(); api = None; let _ = viewed_guild.send(Viewed::default()); }
                                Command::Forget => {
                                    let events = events.clone();
                                    tokio::task::spawn_blocking(move || {
                                        if let Ok(entry) = credential()
                                            && let Err(error) = entry.delete_credential()
                                            && !matches!(error, keyring::Error::NoEntry) {
                                            let _ = events.send_blocking(Envelope { generation, event: Event::Error("Could not remove the saved session from the credential store.".into()) });
                                        }
                                    });
                                }
                                Command::Ack{channel,message}=>{
                                    let Some(client)=api.clone() else {continue;};
                                    if !snowflake(&channel)||!snowflake(&message) {continue;}
                                    let events=events.clone();
                                    requests.spawn(async move {if client.ack(&channel,&message).await.is_err() {emit(&events,generation,Event::Error("Could not sync this conversation's read state with Discord.".into())).await;}});
                                }
                                Command::Layout => {
                                    let Some(client)=api.clone() else {continue;};let events=events.clone();
                                    requests.spawn(async move {let result=client.request(Method::GET,"/users/@me/settings-proto/1",None).await;emit(&events,generation,Event::Layout(result)).await;});
                                }
                                Command::React {channel,message,emoji,remove} => {
                                    let Some(client)=api.clone() else {continue;};let events=events.clone();
                                    requests.spawn(async move {let result=client.react(&channel,&message,&emoji,remove).await;emit(&events,generation,Event::Reacted{channel,message,result}).await;});
                                }
                                Command::StickerDetails(ids) => {
                                    let Some(client)=api.as_ref().map(Api::discovery) else {continue;};
                                    let events = events.clone();
                                    requests.spawn(async move {
                                        let stickers = futures_util::stream::iter(ids.into_iter().filter(|id| crate::expressions::id(id)).take(32))
                                            .map(|id| {let client=client.clone(); async move {
                                                client.request::<crate::expressions::Sticker>(Method::GET,&format!("/stickers/{id}"),None).await.ok().filter(crate::expressions::Sticker::valid)
                                            }})
                                            .buffer_unordered(2).filter_map(|s|async move{s}).collect().await;
                                        emit(&events,generation,Event::StickerDetails(stickers)).await;
                                    });
                                }
                                Command::EmojiRecents => {
                                    let events=events.clone();
                                    let Some(client)=api.as_ref().map(Api::discovery) else {continue;};
                                    requests.spawn(async move {let result=client.request::<Value>(Method::GET,"/users/@me/settings-proto/2",None).await.and_then(|v|crate::gif_favorites::emoji_recents(&v));emit(&events,generation,Event::EmojiRecents(result)).await;});
                                }
                                Command::GifFavorites(change) => {
                                    let Some(client)=api.as_ref().map(Api::discovery) else {continue;};let events=events.clone();
                                    requests.spawn(async move {let result=client.gif_favorites(change).await;emit(&events,generation,Event::GifFavorites(result)).await;});
                                }
                                Command::CancelGifs => {if let Some(old)=gif_request.take() {old.abort();}}
                                Command::Gifs {request,query} => {
                                    let Some(client)=api.as_ref().map(Api::discovery) else {continue;};
                                    if let Some(old)=gif_request.take() {old.abort();}
                                    let events=events.clone();let query=query.chars().take(100).collect::<String>();
                                    gif_request=Some(requests.spawn(async move {
                                        let result=client.gifs(&query).await;
                                        emit(&events,generation,Event::Gifs{request,result}).await;
                                    }));
                                }
                                Command::StickerPacks => {
                                    let Some(client)=api.as_ref().map(Api::discovery) else {continue;};let events=events.clone();
                                    requests.spawn(async move {
                                        let result=client.request(Method::GET,"/sticker-packs",None).await.map(crate::expressions::packs);
                                        emit(&events,generation,Event::StickerPacks(result)).await;
                                    });
                                }
                                Command::GuildExpressions(guild) => {
                                    let Some(client)=api.as_ref().map(Api::discovery) else {continue;};
                                    if !snowflake(&guild) {continue;}let events=events.clone();
                                    requests.spawn(async move {
                                        let emojis=client.request::<Vec<crate::expressions::Emoji>>(Method::GET,&format!("/guilds/{guild}/emojis"),None).await.ok();
                                        let stickers=client.request::<Vec<crate::expressions::Sticker>>(Method::GET,&format!("/guilds/{guild}/stickers"),None).await.ok();
                                        emit(&events,generation,Event::GuildExpressions{guild,emojis,stickers}).await;
                                    });
                                }
                                Command::OpenDm(user) => {
                                    let Some(client)=api.clone() else {continue;};
                                    if !snowflake(&user) {continue;}
                                    let events=events.clone();
                                    requests.spawn(async move {
                                        let result=client.request::<Channel>(Method::POST,"/users/@me/channels",Some(json!({"recipient_id":user}))).await
                                            .and_then(|channel|if channel.kind==1 && channel.recipients.iter().any(|u|u.id==user) {Ok(channel)} else {Err("Discord returned another conversation.".into())});
                                        emit(&events,generation,Event::OpenedDm{user,result}).await;
                                    });
                                }
                                Command::GuildDetails(id,started) => {
                                    let Some(client)=api.clone() else {continue};
                                    if !snowflake(&id) {continue;}
                                    let events=events.clone();
                                    requests.spawn(async move {
                                        let result=client.request::<Guild>(Method::GET,&format!("/guilds/{id}"),None).await.and_then(|guild|if guild.id==id {Ok(guild)} else {Err("Discord returned another server.".into())});
                                        emit(&events,generation,Event::GuildDetails {id,started,result}).await;
                                    });
                                }
                                Command::Profile {id, guild, request, roles_needed} => {
                                    let Some(client) = api.clone() else { continue; };
                                    if !snowflake(&id) || guild.as_ref().is_some_and(|id| !snowflake(id)) { continue; }
                                    let events = events.clone();
                                    requests.spawn(async move {
                                        let mut result = client.profile(&id, guild.as_deref()).await;
                                        if let Ok(profile) = &mut result && roles_needed && let Some(guild) = &guild {
                                            profile.roles = client.request(Method::GET,&format!("/guilds/{guild}/roles"),None).await.unwrap_or_default();
                                        }
                                        let effect_ref = result.as_ref().ok().and_then(|p| p.effect_ref());
                                        let frame_ref=result.as_ref().ok().and_then(|p|p.collectible_ref(3)).map(str::to_string);
                                        emit(&events, generation, Event::Profile {id:id.clone(), guild:guild.clone(), request, result:result.map(Box::new)}).await;
                                        let badges=async {
                                            let client=client.discovery();
                                            if let Ok(Ok(value))=tokio::time::timeout(Duration::from_secs(5),client.request::<Value>(Method::GET,&format!("/users/{id}/badges"),None)).await {
                                                let badges=profile_badges(&value);
                                                emit(&events,generation,Event::ProfileBadges{id:id.clone(),guild:guild.clone(),request,badges}).await;
                                            }
                                        };
                                        let cosmetics=async {
                                        let mut effect=None;
                                        if let Some(reference)=effect_ref {
                                            effect=client.effect(reference.sku_id.as_deref().unwrap_or(&reference.id)).await;
                                            if effect.is_none() && let Some(url)=reference.sku_id.as_deref().and_then(crate::media::collectible) {
                                                effect=Some(crate::effects::Effect{static_frame_src:Some(url),..Default::default()});
                                            }
                                        }
                                        let frame=match frame_ref {Some(sku)=>client.effect(&sku).await.and_then(|e|e.frame),None=>None};
                                        emit(&events,generation,Event::ProfileEffect{id:id.clone(),guild:guild.clone(),request,effect:effect.map(Box::new),frame:frame.map(Box::new)}).await;
                                        };
                                        tokio::join!(badges,cosmetics);
                                    });
                                }
                                Command::History { channel, started, request, page, cursor } => {
                                    let Some(client) = api.clone() else { continue; };
                                    if !snowflake(&channel) || cursor.as_ref().is_some_and(|id| !snowflake(id)) { continue; }
                                    let events = events.clone();
                                    requests.spawn(async move {
                                        let mut path = format!("/channels/{channel}/messages?limit=100");
                                        if let Some(cursor) = cursor {
                                            match page { Page::Before => path.push_str(&format!("&before={cursor}")), Page::After => path.push_str(&format!("&after={cursor}")), Page::Latest => {} }
                                        }
                                        let result = client.request(Method::GET, &path, None).await;
                                        emit(&events, generation, Event::History { channel, started, request, page, result }).await;
                                    });
                                }
                                Command::Interact {message,channel,guild,application,flags,data,nonce} => {
                                    let Some(client) = api.clone() else {continue;};
                                    if ![&message,&channel,&application].into_iter().all(|id| snowflake(id)) || guild.as_ref().is_some_and(|id| !snowflake(id)) {continue;}
                                    let events=events.clone();
                                    requests.spawn(async move {
                                        let session = client.session_id.lock().ok().and_then(|id| id.clone());
                                        let result = match session {
                                            Some(session) => client.request::<()>(Method::POST,"/interactions",Some(json!({"type":3,"nonce":nonce,"application_id":application,"guild_id":guild,"channel_id":channel,"message_id":message,"message_flags":flags,"session_id":session,"data":data}))).await,
                                            None => Err("Wait for the connection before using this control.".into()),
                                        };
                                        emit(&events,generation,Event::Interaction {message,result}).await;
                                    });
                                }
                                Command::ViewGuild(guild) => {viewed_guild.send_modify(|view|{let guild=guild.filter(|id|snowflake(id));if view.guild!=guild {*view=Viewed{guild,..Default::default()};}});}
                                Command::ViewMembers{guild,channel,bucket,enabled}=>{if snowflake(&guild)&&snowflake(&channel){let _=viewed_guild.send(Viewed{guild:Some(guild),channel:Some(channel),bucket:bucket.min(1_999_900)/100*100,members:enabled});}}
                                Command::Typing(channel) => {
                                    let Some(client) = api.clone() else { continue; };
                                    if !snowflake(&channel) { continue; }
                                    requests.spawn(async move {let _ = client.request::<()>(Method::POST,&format!("/channels/{channel}/typing"),None).await;});
                                }
                                Command::Send { channel, content, nonce,reply,sticker } => {
                                    let Some(client) = api.clone() else { continue; };
                                    if !snowflake(&channel) || (content.is_empty() && sticker.is_none()) || content.chars().count() > 2000 || sticker.as_deref().is_some_and(|id|!snowflake(id)) { continue; }
                                    let events = events.clone();
                                    requests.spawn(async move {
                                        let result = client.request::<Message>(Method::POST, &format!("/channels/{channel}/messages"), Some(send_body(&content,&nonce,reply.as_deref(),sticker.as_deref()))).await;
                                        emit(&events, generation, Event::Sent { nonce, result: result.map(Box::new) }).await;
                                    });
                                }
                                Command::Notify { title, body } => {
                                    tokio::task::spawn_blocking(move || {
                                        let _ = notify_rust::Notification::new().summary(&title).body(&body).appname("Larpcord").show();
                                    });
                                }
                            }
                        }
                    }
                }
                tasks.abort_all();
                requests.abort_all();
                auth.abort_all();
            });
        }).expect("network thread");
        (Self { tx }, receiver)
    }

    pub fn command(&self, command: Command) -> Result<(), String> {
        self.tx
            .try_send(command)
            .map_err(|_| "The client is busy. Try again in a moment.".into())
    }
}

fn credential() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new("app.larpcord.client", "discord-session")
}

#[derive(Clone, Default, PartialEq)]
struct Viewed {
    guild: Option<String>,
    channel: Option<String>,
    bucket: usize,
    members: bool,
}
#[derive(Default)]
struct Session {
    subscribed_guild: Option<Viewed>,
    id: Option<String>,
    sequence: Option<u64>,
    resume_url: Option<String>,
}

fn resume_url(url: Option<&str>) -> String {
    let valid = url
        .and_then(|url| reqwest::Url::parse(url).ok())
        .filter(|url| {
            url.scheme() == "wss"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url
                    .host_str()
                    .is_some_and(|h| h == "gateway.discord.gg" || h.ends_with(".discord.gg"))
        });
    let base = valid.map_or_else(
        || "wss://gateway.discord.gg/".to_string(),
        |url| {
            url.as_str()
                .split('?')
                .next()
                .unwrap_or_default()
                .to_string()
        },
    );
    format!("{base}?v=10&encoding=json")
}

async fn gateway(
    api: Api,
    events: Sender<Envelope>,
    generation: u64,
    mut viewed: watch::Receiver<Viewed>,
) {
    let mut session = Session::default();
    let mut attempts = 0u32;
    loop {
        emit(
            &events,
            generation,
            Event::Status(
                if session.id.is_some() {
                    "Reconnecting"
                } else {
                    "Connecting"
                }
                .into(),
            ),
        )
        .await;
        let started = Instant::now();
        let result = connection(&api, &events, generation, &mut session, &mut viewed).await;
        if started.elapsed() > Duration::from_secs(60) {
            attempts = 0;
        }
        match result {
            Ok(()) => attempts = 0,
            Err(ConnectionError::Fatal) => {
                emit(
                    &events,
                    generation,
                    Event::Error(
                        "Discord ended this session. Open settings and connect again.".into(),
                    ),
                )
                .await;
                emit(&events, generation, Event::Status("Disconnected".into())).await;
                return;
            }
            Err(ConnectionError::Transient) => attempts = (attempts + 1).min(6),
        }
        emit(&events, generation, Event::Status("Reconnecting".into())).await;
        let delay = (1u64 << attempts).min(60) * 1000 + rand::random_range(0..1000);
        sleep(Duration::from_millis(delay)).await;
    }
}

enum ConnectionError {
    Fatal,
    Transient,
}
impl From<tokio_tungstenite::tungstenite::Error> for ConnectionError {
    fn from(_: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::Transient
    }
}

async fn connection(
    api: &Api,
    events: &Sender<Envelope>,
    generation: u64,
    session: &mut Session,
    viewed: &mut watch::Receiver<Viewed>,
) -> Result<(), ConnectionError> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(16 * 1024 * 1024))
        .max_frame_size(Some(16 * 1024 * 1024));
    let (socket, _) = tokio::time::timeout(
        Duration::from_secs(20),
        connect_async_with_config(
            resume_url(session.resume_url.as_deref()),
            Some(config),
            false,
        ),
    )
    .await
    .map_err(|_| ConnectionError::Transient)?
    .map_err(|_| ConnectionError::Transient)?;
    drive_watched_connection(socket, api, events, generation, session, viewed).await
}

async fn drive_watched_connection<S>(
    mut socket: tokio_tungstenite::WebSocketStream<S>,
    api: &Api,
    events: &Sender<Envelope>,
    generation: u64,
    session: &mut Session,
    viewed: &mut watch::Receiver<Viewed>,
) -> Result<(), ConnectionError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let hello = tokio::time::timeout(Duration::from_secs(20), socket.next())
        .await
        .map_err(|_| ConnectionError::Transient)?
        .ok_or(ConnectionError::Transient)??;
    let hello: Value =
        serde_json::from_slice(&hello.into_data()).map_err(|_| ConnectionError::Transient)?;
    let interval = hello["d"]["heartbeat_interval"]
        .as_u64()
        .filter(|n| *n >= 1000 && *n <= 120000)
        .ok_or(ConnectionError::Transient)?;
    let identify = match (&session.id, session.sequence) {
        (Some(id), Some(sequence)) => {
            json!({"op":6,"d":{"token":api.token.as_str(),"session_id":id,"seq":sequence}})
        }
        _ => {
            json!({"op":2,"d":{"token":api.token.as_str(),"properties":{"os":std::env::consts::OS,"browser":"Larpcord","device":"Larpcord"},"compress":false,"large_threshold":50}})
        }
    };
    socket
        .send(Frame::Text(identify.to_string().into()))
        .await?;
    let mut deadline = Instant::now() + Duration::from_millis(rand::random_range(0..interval));
    let mut acknowledged = true;
    let mut ready = false;
    let mut subscribed = Viewed::default();
    loop {
        tokio::select! {
            changed = viewed.changed(), if ready => {
                if changed.is_err() {return Err(ConnectionError::Transient);}
                let current = viewed.borrow_and_update().clone();
                for payload in view_subscription_frames(&subscribed,&current) {
                    socket.send(Frame::Text(payload.to_string().into())).await?;
                }
                session.subscribed_guild = Some(current.clone());
                subscribed = current;
            }
            _ = sleep_until(deadline) => {
                if !acknowledged { return Err(ConnectionError::Transient); }
                socket.send(Frame::Text(json!({"op":1,"d":session.sequence}).to_string().into())).await?;
                acknowledged = false;
                deadline = Instant::now() + Duration::from_millis(interval);
            }
            frame = socket.next() => {
                let frame = frame.ok_or(ConnectionError::Transient)??;
                match frame {
                    Frame::Ping(bytes) => { socket.send(Frame::Pong(bytes)).await?; continue; }
                    Frame::Close(close) => {
                        if let Some(close) = close {
                            let code: u16 = close.code.into();
                            if matches!(code, 4004 | 4010..=4014) { return Err(ConnectionError::Fatal); }
                            if matches!(code, 4007 | 4009) { *session = Session::default(); }
                        }
                        return Err(ConnectionError::Transient);
                    }
                    Frame::Text(_) => {},
                    _ => continue,
                }
                let mut payload: Value = serde_json::from_slice(&frame.into_data()).map_err(|_| ConnectionError::Transient)?;
                if let Some(sequence) = payload["s"].as_u64() { session.sequence = Some(sequence); }
                match payload["op"].as_u64() {
                    Some(0) => {
                        let kind = payload["t"].as_str().unwrap_or_default().to_string();
                        let data = payload["d"].take();
                        if kind == "READY" {
                            session.id = data["session_id"].as_str().map(str::to_string);
                            session.resume_url = data["resume_gateway_url"].as_str().map(str::to_string);
                            if let Ok(mut id) = api.session_id.lock() {*id = session.id.clone();}
                            emit(events, generation, Event::Status("Connected".into())).await;
                        }
                        if kind == "RESUMED" { emit(events, generation, Event::Status("Connected".into())).await; }
                        if kind == "READY" || kind == "RESUMED" {
                            ready = true;
                            subscribed = if kind == "RESUMED" {session.subscribed_guild.clone().unwrap_or_default()}else{Viewed::default()};
                            let current = viewed.borrow_and_update().clone();
                            for payload in view_subscription_frames(&subscribed,&current) {socket.send(Frame::Text(payload.to_string().into())).await?;}
                            session.subscribed_guild = Some(current.clone());
                            subscribed = current;
                        }
                        if matches!(kind.as_str(), "READY" | "READY_SUPPLEMENTAL" | "RELATIONSHIP_ADD" | "RELATIONSHIP_REMOVE" | "GUILD_CREATE" | "GUILD_UPDATE" | "GUILD_DELETE" | "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "CHANNEL_DELETE" | "CHANNEL_RECIPIENT_ADD" | "CHANNEL_RECIPIENT_REMOVE" | "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" | "MESSAGE_REACTION_REMOVE_ALL" | "MESSAGE_REACTION_REMOVE_EMOJI" | "MESSAGE_DELETE" | "MESSAGE_DELETE_BULK" | "CALL_CREATE" | "TYPING_START" | "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" | "GUILD_ROLE_DELETE" | "GUILD_MEMBER_UPDATE" | "GUILD_EMOJIS_UPDATE" | "GUILD_STICKERS_UPDATE" | "MESSAGE_ACK" | "USER_SETTINGS_UPDATE" | "USER_SETTINGS_PROTO_UPDATE" | "USER_GUILD_SETTINGS_UPDATE" | "USER_UPDATE" | "GUILD_MEMBER_LIST_UPDATE" | "PRESENCE_UPDATE") {
                            emit(events, generation, Event::Dispatch(kind, data)).await;
                        }
                    }
                    Some(1) => { socket.send(Frame::Text(json!({"op":1,"d":session.sequence}).to_string().into())).await?; }
                    Some(7) => return Ok(()),
                    Some(9) => {
                        if payload["d"].as_bool() != Some(true) { *session = Session::default(); }
                        sleep(Duration::from_millis(rand::random_range(1000..5000))).await;
                        return Ok(());
                    }
                    Some(11) => acknowledged = true,
                    _ => {},
                }
            }
        }
    }
}

fn view_subscription_change(previous: &Viewed, current: &Viewed) -> Option<Value> {
    if previous == current {
        return None;
    }
    let mut subscriptions = serde_json::Map::new();
    if previous.guild != current.guild
        && let Some(id) = &previous.guild
    {
        subscriptions.insert(
            id.clone(),
            json!({"typing":false,"activities":false,"threads":false,"channels":{}}),
        );
    }
    if let Some(id) = &current.guild {
        let mut channels = serde_json::Map::new();
        if current.members
            && let Some(channel) = &current.channel
        {
            let mut ranges = vec![json!([0, 99])];
            if current.bucket > 0 {
                ranges.push(json!([current.bucket, current.bucket + 99]));
            }
            channels.insert(channel.clone(), json!(ranges));
        }
        subscriptions.insert(
            id.clone(),
            json!({"typing":true,"activities":current.members,"threads":false,"channels":channels}),
        );
    }
    (!subscriptions.is_empty()).then(|| json!({"op":37,"d":{"subscriptions":subscriptions}}))
}

fn view_subscription_frames(previous: &Viewed, current: &Viewed) -> Vec<Value> {
    let Some(mut final_frame) = view_subscription_change(previous, current) else {
        return Vec::new();
    };
    let Some(guild) = &current.guild else {
        return vec![final_frame];
    };
    let mut frames = Vec::new();
    let active = current.members && current.channel.is_some();
    if active
        && (previous.guild != current.guild
            || previous.channel != current.channel
            || !previous.members)
    {
        // the guild must be subscribed in an earlier frame before discord accepts ranges.
        if previous.guild != current.guild {
            let mut activate = final_frame.clone();
            activate["d"]["subscriptions"][guild]["channels"] = json!({});
            frames.push(activate);
        }
        // unchanged ranges produce no new sync, even after leaving the guild. move the
        // window first, then restore it. empty per-channel ranges cause a 4002 close.
        let mut displaced = final_frame.clone();
        let channel = current.channel.as_ref().unwrap();
        let start = current.bucket + 100;
        displaced["d"]["subscriptions"][guild]["channels"] = json!({channel:[[start,start+99]]});
        frames.push(displaced);
    } else if !active
        && previous.members
        && previous.guild == current.guild
        && let Some(channel) = &previous.channel
    {
        // closing the rail parks the server range outside the retained viewport.
        let start = previous.bucket + 100;
        final_frame["d"]["subscriptions"][guild]["channels"] = json!({channel:[[start,start+99]]});
    }
    frames.push(final_frame);
    frames
}
#[cfg(test)]
fn guild_subscription_change(previous: Option<&str>, current: Option<&str>) -> Option<Value> {
    if previous == current {
        return None;
    }
    let mut subscriptions = serde_json::Map::new();
    if let Some(previous) = previous.filter(|id| snowflake(id)) {
        subscriptions.insert(
            previous.into(),
            json!({"typing":false,"activities":false,"threads":false,"channels":{}}),
        );
    }
    if let Some(current) = current.filter(|id| snowflake(id)) {
        subscriptions.insert(
            current.into(),
            json!({"typing":true,"activities":false,"threads":false,"channels":{}}),
        );
    }
    (!subscriptions.is_empty()).then(|| json!({"op":37,"d":{"subscriptions":subscriptions}}))
}

#[cfg(test)]
async fn drive_connection<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    api: &Api,
    events: &Sender<Envelope>,
    generation: u64,
    session: &mut Session,
) -> Result<(), ConnectionError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (_sender, mut viewed) = watch::channel(Viewed::default());
    drive_watched_connection(socket, api, events, generation, session, &mut viewed).await
}

pub fn guild_from_ready(value: &Value) -> Option<Guild> {
    let mut properties = value
        .get("properties")
        .cloned()
        .unwrap_or_else(|| value.clone());
    properties["id"] = value.get("id")?.clone();
    for field in [
        "name",
        "icon",
        "banner",
        "premium_subscription_count",
        "premium_tier",
        "roles",
        "emojis",
        "stickers",
    ] {
        if let Some(outer) = value.get(field) {
            properties[field] = outer.clone();
        }
    }
    serde_json::from_value(properties).ok()
}

pub fn guild_channels(value: &Value) -> Vec<Channel> {
    value["channels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|channel| {
            let mut channel: Channel = serde_json::from_value(channel.clone()).ok()?;
            channel.guild_id = value["id"].as_str().map(str::to_string);
            Some(channel)
        })
        .collect()
}

fn profile_badges(value: &Value) -> Vec<crate::model::ProfileBadge> {
    value["badges"]
        .as_array()
        .into_iter()
        .flatten()
        .take(80)
        .filter(|b| b["owned"].as_bool() == Some(true) && b["hidden"].as_bool() != Some(true))
        .filter_map(|badge| {
            let art = badge["current_tier"]
                .as_str()
                .and_then(|key| {
                    badge["tiers"]
                        .as_array()?
                        .iter()
                        .find(|t| t["key"].as_str() == Some(key))
                })
                .unwrap_or(badge);
            let icon = [
                "simple_icon_raster_url",
                "simple_icon_url",
                "complex_icon_static_url",
            ]
            .iter()
            .filter_map(|key| art[*key].as_str().or(badge[*key].as_str()))
            .find(|url| crate::media::cdn_url(url))?
            .to_string();
            let description = badge["name"]
                .as_str()
                .or(badge["description"].as_str())
                .unwrap_or("Discord badge")
                .chars()
                .take(160)
                .collect();
            let mut description: String = description;
            if let Some(tier) = art["name"].as_str().filter(|n| !description.contains(n)) {
                description.push_str(" · ");
                description.push_str(tier);
            }
            if let Some(info) = badge["info_label"].as_str() {
                description.push_str(" · ");
                description.push_str(info);
            }
            description = description.chars().take(240).collect();
            Some(crate::model::ProfileBadge { icon, description })
        })
        .take(40)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn reaction_requests_encode_emoji_and_accept_discord_empty_responses() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut api = Api::new(Zeroizing::new("synthetic-session".into())).unwrap();
        api.endpoint = Some(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move {
            for method in ["PUT", "DELETE"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![0; 4096];
                let n = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]);
                assert!(request.starts_with(&format!(
                    "{method} /channels/1/messages/2/reactions/%F0%9F%91%8D/@me?type=0 "
                )));
                socket
                    .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
            }
        });
        api.react("1", "2", "👍", false).await.unwrap();
        api.react("1", "2", "👍", true).await.unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn reading_gif_favorites_never_writes_settings() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut api = Api::new(Zeroizing::new("synthetic-session".into())).unwrap();
        api.endpoint = Some(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 8192];
            let n = socket.read(&mut bytes).await.unwrap();
            assert!(
                String::from_utf8_lossy(&bytes[..n])
                    .starts_with("GET /users/@me/settings-proto/2 HTTP/1.1")
            );
            let body = r#"{"settings":""}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), listener.accept())
                    .await
                    .is_err()
            );
        });
        assert!(api.gif_favorites(None).await.unwrap().is_empty());
        server.await.unwrap();
    }
    #[test]
    fn badge_catalogs_only_display_owned_visible_raster_art() {
        let result = profile_badges(
            &json!({"badges":[{"name":"new badge","owned":true,"simple_icon_raster_url":"https://cdn.discordapp.com/assets/new-badge.png"},{"owned":false,"simple_icon_raster_url":"https://cdn.discordapp.com/assets/unearned.png"},{"owned":true,"hidden":true,"simple_icon_raster_url":"https://cdn.discordapp.com/assets/hidden.png"},{"owned":true,"simple_icon_url":"https://evil.invalid/badge.png"}]}),
        );
        assert_eq!(result.len(), 1);
        assert!(result[0].image().unwrap().contains("new-badge.png"));
    }

    #[tokio::test]
    async fn gif_discovery_encodes_searches_and_does_not_hold_the_message_lane() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut api = Api::new(Zeroizing::new("synthetic-session".into())).unwrap();
        api.endpoint = Some(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 8192];
            let size = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..size]);
            let path = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let url = reqwest::Url::parse(&format!("http://localhost{path}")).unwrap();
            assert_eq!(url.path(), "/gifs/search");
            assert_eq!(
                url.query_pairs()
                    .find(|(k, _)| k == "media_format")
                    .unwrap()
                    .1,
                "gif"
            );
            assert_eq!(
                url.query_pairs().find(|(k, _)| k == "q").unwrap().1,
                "cats & 犬"
            );
            assert!(!path.contains("synthetic-session"));
            let body = r#"[{"url":"https://klipy.com/gifs/cat","preview":"https://static.klipy.com/cat.gif","title":"cat"}]"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let _message_lane = api.gate.lock().await;
        let result =
            tokio::time::timeout(Duration::from_secs(2), api.discovery().gifs("cats & 犬"))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(result.len(), 1);
        server.await.unwrap();
    }
    async fn validation_server(statuses: Vec<u16>) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            for status in statuses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                let _ = socket.read(&mut request).await.unwrap();
                let response = format!(
                    "HTTP/1.1 {status} status\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (endpoint, task)
    }

    #[test]
    fn member_ranges_follow_the_viewport_and_close_without_an_activity_stream() {
        let before = Viewed {
            guild: Some("1".into()),
            channel: Some("2".into()),
            bucket: 0,
            members: true,
        };
        let next = Viewed {
            bucket: 300,
            ..before.clone()
        };
        let payload = view_subscription_change(&before, &next).unwrap();
        assert_eq!(
            payload["d"]["subscriptions"]["1"]["channels"]["2"],
            json!([[0, 99], [300, 399]])
        );
        let closed = Viewed {
            members: false,
            ..next.clone()
        };
        let payload = view_subscription_change(&next, &closed).unwrap();
        assert_eq!(payload["d"]["subscriptions"]["1"]["channels"], json!({}));
        assert_eq!(payload["d"]["subscriptions"]["1"]["activities"], false);
        assert!(view_subscription_change(&closed, &closed).is_none());
        let switched = Viewed {
            channel: Some("3".into()),
            ..before.clone()
        };
        let payload = view_subscription_change(&before, &switched).unwrap();
        assert!(
            payload["d"]["subscriptions"]["1"]["channels"]
                .get("2")
                .is_none()
        );
        assert_eq!(
            payload["d"]["subscriptions"]["1"]["channels"]["3"],
            json!([[0, 99]])
        );
        let body = send_body("hi", "123", Some("456"), Some("789"));
        assert_eq!(body["message_reference"]["message_id"], "456");
        assert_eq!(body["allowed_mentions"]["replied_user"], false);
        assert_eq!(body["sticker_ids"], json!(["789"]));
    }

    #[tokio::test]
    async fn member_subscriptions_prime_guilds_and_refresh_ranges_after_returning() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let view = |guild: &str, channel: &str| Viewed {
            guild: Some(guild.into()),
            channel: Some(channel.into()),
            members: true,
            bucket: 0,
        };
        let (selection, mut viewed) = watch::channel(view("1", "2"));
        let _keep_selection = selection.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket
                .send(Frame::Text(
                    json!({"op":10,"d":{"heartbeat_interval":1000}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let _identify = socket.next().await.unwrap().unwrap();
            socket
                .send(Frame::Text(
                    json!({"op":0,"t":"READY","s":1,"d":{"session_id":"fixture"}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let mut active = std::collections::HashSet::new();
            let mut held = HashMap::<String, Value>::new();
            let mut syncs = 0;
            let mut sequence = 1;
            while let Some(Ok(Frame::Text(text))) = socket.next().await {
                let packet: Value = serde_json::from_str(&text).unwrap();
                if packet["op"] == 1 {
                    socket
                        .send(Frame::Text(json!({"op":11}).to_string().into()))
                        .await
                        .unwrap();
                    continue;
                }
                for (guild, subscription) in packet["d"]["subscriptions"].as_object().unwrap() {
                    let primed = active.contains(guild);
                    if subscription["typing"] == true {
                        active.insert(guild.clone());
                    } else {
                        active.remove(guild);
                    }
                    for ranges in subscription["channels"].as_object().unwrap().values() {
                        // the real service rejects an empty per-channel range array.
                        assert!(!ranges.as_array().unwrap().is_empty());
                        if !primed || held.get(guild) == Some(ranges) {
                            continue;
                        }
                        held.insert(guild.clone(), ranges.clone());
                        sequence += 1;
                        socket.send(Frame::Text(json!({"op":0,"t":"GUILD_MEMBER_LIST_UPDATE","s":sequence,"d":{"guild_id":guild,"id":"everyone","ops":[{"op":"SYNC","range":ranges[0],"items":[]}]}}).to_string().into())).await.unwrap();
                        if ranges == &json!([[0, 99]]) {
                            syncs += 1;
                            match syncs {
                                1 => selection.send(view("3", "4")).unwrap(),
                                2 => selection.send(view("1", "2")).unwrap(),
                                3 => {
                                    socket
                                        .send(Frame::Close(Some(
                                            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                                code: 4004.into(),
                                                reason: "fixture complete".into(),
                                            },
                                        )))
                                        .await
                                        .unwrap();
                                    return syncs;
                                }
                                _ => panic!("unexpected repeated sync"),
                            }
                        }
                    }
                }
            }
            syncs
        });
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let api = Api::new(Zeroizing::new("synthetic-not-a-credential".into())).unwrap();
        let (events, _receiver) = async_channel::bounded(16);
        let mut session = Session::default();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            drive_watched_connection(socket, &api, &events, 1, &mut session, &mut viewed),
        )
        .await;
        assert!(matches!(result, Ok(Err(ConnectionError::Fatal))));
        assert_eq!(server.await.unwrap(), 3);
    }

    #[tokio::test]
    async fn read_sync_sends_latest_only_and_reuses_the_ack_token() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for (message, token) in [("100", Value::Null), ("101", json!("ack-test"))] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut data = Vec::new();
                loop {
                    let mut buffer = [0; 2048];
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&buffer[..n]);
                    if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&data[..end]);
                        let len = header
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|s| s.trim().parse::<usize>().ok())
                            })
                            .unwrap();
                        if data.len() >= end + 4 + len {
                            assert!(
                                header.starts_with(&format!(
                                    "POST /channels/1/messages/{message}/ack "
                                ))
                            );
                            let body: Value = serde_json::from_slice(&data[end + 4..]).unwrap();
                            assert_eq!(body["token"], token);
                            break;
                        }
                    }
                }
                let body = r#"{"token":"ack-test"}"#;
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let mut api = Api::new(Zeroizing::new("test-only".into())).unwrap();
        api.endpoint = Some(endpoint);
        api.ack("1", "100").await.unwrap();
        api.ack("1", "99").await.unwrap();
        api.ack("1", "100").await.unwrap();
        api.ack("1", "101").await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn startup_retries_transient_failure_and_recovers_without_another_login() {
        let (endpoint, server) = validation_server(vec![503, 200]).await;
        let mut api = Api::new(Zeroizing::new("test-only".into())).unwrap();
        api.endpoint = Some(endpoint);
        let (events, receiver) = async_channel::bounded(8);
        assert!(
            tokio::time::timeout(Duration::from_secs(5), api.validate(&events, 7))
                .await
                .unwrap()
        );
        assert!(
            matches!(receiver.recv().await.unwrap().event, Event::Status(status) if status == "Reconnecting")
        );
        server.await.unwrap();
        assert_eq!(retry_delay(100), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn expired_credentials_stop_retrying_and_get_a_useful_status() {
        let (endpoint, server) = validation_server(vec![401]).await;
        let mut api = Api::new(Zeroizing::new("test-only".into())).unwrap();
        api.endpoint = Some(endpoint);
        let (events, receiver) = async_channel::bounded(8);
        assert!(!api.validate(&events, 7).await);
        assert!(
            matches!(receiver.recv().await.unwrap().event, Event::Status(status) if status == "Connection failed")
        );
        assert!(
            matches!(receiver.recv().await.unwrap().event, Event::Error(error) if error.contains("expired"))
        );
        server.await.unwrap();
    }

    #[test]
    fn changing_guild_subscriptions_disables_the_previous_guild() {
        let payload = guild_subscription_change(Some("1"), Some("2")).unwrap();
        assert_eq!(payload["op"], 37);
        assert_eq!(payload["d"]["subscriptions"]["1"]["typing"], false);
        assert_eq!(payload["d"]["subscriptions"]["2"]["typing"], true);
        assert!(guild_subscription_change(None, None).is_none());
        assert!(guild_subscription_change(None, Some("not a snowflake")).is_none());
    }

    #[test]
    fn ready_guild_properties_keep_artwork_and_roles() {
        let guild=guild_from_ready(&json!({"id":"123","properties":{"name":"guild","icon":"abc","banner":"def"},"premium_subscription_count":8,"premium_tier":2,"roles":[{"id":"7","name":"mod","position":3,"color":4}]})).unwrap();
        assert_eq!(guild.icon.as_deref(), Some("abc"));
        assert_eq!(guild.banner.as_deref(), Some("def"));
        assert_eq!(guild.roles[0].name, "mod");
        assert_eq!(guild.premium_subscription_count, Some(8));
        assert_eq!(guild.premium_tier, Some(2));
    }

    #[test]
    fn resume_endpoint_cannot_redirect_credentials_to_another_host() {
        for url in [
            "wss://evil.test",
            "wss://gateway.discord.gg.evil.test/",
            "ws://gateway.discord.gg/",
            "wss://evil.test@gateway.discord.gg/",
        ] {
            assert_eq!(
                resume_url(Some(url)),
                "wss://gateway.discord.gg/?v=10&encoding=json"
            );
        }
        assert_eq!(
            resume_url(Some("wss://gateway-us-east1-b.discord.gg/")),
            "wss://gateway-us-east1-b.discord.gg/?v=10&encoding=json"
        );
    }

    #[tokio::test]
    async fn heartbeat_and_resume_use_the_last_dispatch_sequence() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                socket
                    .send(Frame::Text(
                        json!({"op":10,"d":{"heartbeat_interval":1000}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                let identify: Value =
                    serde_json::from_slice(&socket.next().await.unwrap().unwrap().into_data())
                        .unwrap();
                if attempt == 0 {
                    assert_eq!(identify["op"], 2);
                    assert_eq!(identify["d"]["properties"]["browser"], "Larpcord");
                    socket.send(Frame::Text(json!({"op":0,"s":42,"t":"READY","d":{"session_id":"sample-session","resume_gateway_url":"wss://gateway.discord.gg/"}}).to_string().into())).await.unwrap();
                } else {
                    assert_eq!(identify["op"], 6);
                    assert_eq!(identify["d"]["seq"], 42);
                    assert_eq!(identify["d"]["session_id"], "sample-session");
                }
                let heartbeat: Value =
                    serde_json::from_slice(&socket.next().await.unwrap().unwrap().into_data())
                        .unwrap();
                assert_eq!(heartbeat["op"], 1);
                assert_eq!(heartbeat["d"], 42);
                socket
                    .send(Frame::Text(json!({"op":11}).to_string().into()))
                    .await
                    .unwrap();
                socket
                    .send(Frame::Text(json!({"op":7}).to_string().into()))
                    .await
                    .unwrap();
            }
        });
        let api = Api::new(Zeroizing::new("offline-fixture-token".into())).unwrap();
        let (events, _receiver) = async_channel::bounded(16);
        let mut session = Session::default();
        for _ in 0..2 {
            let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
                .await
                .unwrap();
            assert!(
                drive_connection(socket, &api, &events, 1, &mut session)
                    .await
                    .is_ok()
            );
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn viewed_guild_typing_is_subscribed_after_ready_and_changes_live() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (viewed_tx, mut viewed_rx) = watch::channel(Viewed {
            guild: Some("1".to_string()),
            ..Default::default()
        });
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(Frame::Text(
                json!({"op":10,"d":{"heartbeat_interval":1000}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let identify: Value =
                serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
            assert_eq!(identify["op"], 2);
            ws.send(Frame::Text(
                json!({"op":0,"s":1,"t":"READY","d":{"session_id":"local-test"}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let mut changes = 0;
            while let Some(Ok(frame)) = ws.next().await {
                let value: Value = serde_json::from_slice(&frame.into_data()).unwrap();
                if value["op"] == 1 {
                    ws.send(Frame::Text(json!({"op":11}).to_string().into()))
                        .await
                        .unwrap();
                    continue;
                }
                if value["op"] != 37 {
                    continue;
                }
                if changes == 0 {
                    assert_eq!(value["d"]["subscriptions"]["1"]["typing"], true);
                    viewed_tx
                        .send(Viewed {
                            guild: Some("2".into()),
                            ..Default::default()
                        })
                        .unwrap();
                    changes += 1;
                } else {
                    assert_eq!(value["d"]["subscriptions"]["1"]["typing"], false);
                    assert_eq!(value["d"]["subscriptions"]["2"]["typing"], true);
                    break;
                }
            }
            assert_eq!(changes, 1);
        });
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let api = Api::new(Zeroizing::new("local-test-not-a-credential".into())).unwrap();
        let (events, _) = async_channel::bounded(8);
        let mut session = Session::default();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            drive_watched_connection(socket, &api, &events, 1, &mut session, &mut viewed_rx),
        )
        .await;
        assert!(result.is_ok());
        server.await.unwrap();
        assert_eq!(
            session
                .subscribed_guild
                .as_ref()
                .and_then(|view| view.guild.as_deref()),
            Some("2")
        );
        assert_eq!(
            api.session_id.lock().unwrap().as_deref(),
            Some("local-test")
        );
    }

    #[tokio::test]
    async fn a_missing_heartbeat_acknowledgement_ends_the_connection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket
                .send(Frame::Text(
                    json!({"op":10,"d":{"heartbeat_interval":1000}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            while socket.next().await.is_some() {}
        });
        let api = Api::new(Zeroizing::new("offline-fixture-token".into())).unwrap();
        let (events, _receiver) = async_channel::bounded(16);
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(4),
            drive_connection(socket, &api, &events, 1, &mut Session::default()),
        )
        .await;
        assert!(matches!(result, Ok(Err(ConnectionError::Transient))));
        server.abort();
    }
}
