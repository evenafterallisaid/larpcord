use std::{sync::Arc, time::Duration};

use async_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinSet,
    time::{Instant, sleep, sleep_until},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message as Frame, protocol::WebSocketConfig},
};
use zeroize::Zeroizing;

use crate::model::{Channel, Guild, Message};

pub enum Event {
    Auth {
        attempt: u64,
        event: crate::auth::AuthEvent,
    },
    Status(String),
    Dispatch(String, Value),
    History {
        channel: String,
        started: u64,
        older: bool,
        messages: Vec<Message>,
    },
    Sent {
        nonce: String,
        result: Result<Message, String>,
    },
    Error(String),
    SavedSession(Option<Zeroizing<String>>),
}

pub struct Envelope {
    pub generation: u64,
    pub event: Event,
}

pub enum Command {
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
        before: Option<String>,
    },
    Send {
        channel: String,
        content: String,
        nonce: String,
    },
    Notify {
        title: String,
        body: String,
    },
    Forget,
}

pub struct Backend {
    tx: mpsc::Sender<Command>,
}

#[derive(Clone)]
struct Api {
    client: Client,
    token: Arc<Zeroizing<String>>,
    gate: Arc<Mutex<Instant>>,
}

impl Api {
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
            token: Arc::new(token),
            gate: Arc::new(Mutex::new(Instant::now())),
        })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, String> {
        // one request lane observes retry-after and avoids a burst when switching channels.
        let mut next = self.gate.lock().await;
        for _ in 0..3 {
            sleep_until(*next).await;
            let mut request = self
                .client
                .request(method.clone(), format!("https://discord.com/api/v10{path}"))
                .header("Authorization", self.token.as_str());
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(|_| {
                "The request could not be completed. Check your connection.".to_string()
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
                return Err(
                    "Your session has expired. Connect again with a valid session token.".into(),
                );
            }
            if status.as_u16() == 403 {
                return Err("Discord did not allow access to this channel or action.".into());
            }
            if !status.is_success() {
                return Err(format!(
                    "Discord returned HTTP {}. Try again shortly.",
                    status.as_u16()
                ));
            }
            let mut response = response;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "The response was interrupted.".to_string())?
            {
                if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                    return Err("The response exceeded the client memory limit.".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            return serde_json::from_slice(&bytes)
                .map_err(|_| "Discord returned an unexpected response.".into());
        }
        Err("Discord is rate limiting this session. Wait a moment and try again.".into())
    }
}

fn safe_delay(seconds: f64) -> Duration {
    Duration::from_secs_f64(if seconds.is_finite() {
        seconds.clamp(0.1, 300.0)
    } else {
        5.0
    })
}

fn snowflake(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

async fn emit(events: &Sender<Envelope>, generation: u64, event: Event) {
    let _ = events.send(Envelope { generation, event }).await;
}

impl Backend {
    pub fn start() -> (Self, async_channel::Receiver<Envelope>) {
        let (tx, mut commands) = mpsc::channel::<Command>(32);
        let (events, receiver) = async_channel::bounded(256);
        std::thread::Builder::new().name("larpcord-network".into()).spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("network runtime");
            runtime.block_on(async move {
                let mut tasks = JoinSet::new();
                let mut requests = JoinSet::new();
                let mut auth = JoinSet::new();
                let mut api = None::<Api>;
                let mut generation = 0;
                let saved_events = events.clone();
                tokio::task::spawn_blocking(move || {
                    let token = credential().ok().and_then(|entry| entry.get_password().ok()).map(Zeroizing::new);
                    let _ = saved_events.send_blocking(Envelope { generation: 0, event: Event::SavedSession(token) });
                });
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
                                    match Api::new(token) {
                                        Ok(client) => {
                                            api = Some(client.clone());
                                            let events = events.clone();
                                            tasks.spawn(async move {
                                                // validate before saving a mistyped or expired credential.
                                                if let Err(error) = client.request::<Value>(Method::GET, "/users/@me", None).await {
                                                    emit(&events, generation, Event::Error(error)).await;
                                                    return;
                                                }
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
                                                gateway(client, events, generation).await;
                                            });
                                        }
                                        Err(error) => emit(&events, generation, Event::Error(error)).await,
                                    }
                                }
                                Command::Disconnect => { tasks.abort_all(); requests.abort_all(); auth.abort_all(); api = None; }
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
                                Command::History { channel, started, before } => {
                                    let Some(client) = api.clone() else { continue; };
                                    if !snowflake(&channel) || before.as_ref().is_some_and(|id| !snowflake(id)) { continue; }
                                    let events = events.clone();
                                    requests.spawn(async move {
                                        let older = before.is_some();
                                        let mut path = format!("/channels/{channel}/messages?limit=100");
                                        if let Some(before) = before { path.push_str(&format!("&before={before}")); }
                                        let event = match client.request(Method::GET, &path, None).await {
                                            Ok(messages) => Event::History { channel, started, older, messages },
                                            Err(error) => Event::Error(error),
                                        };
                                        emit(&events, generation, event).await;
                                    });
                                }
                                Command::Send { channel, content, nonce } => {
                                    let Some(client) = api.clone() else { continue; };
                                    if !snowflake(&channel) || content.is_empty() || content.chars().count() > 2000 { continue; }
                                    let events = events.clone();
                                    requests.spawn(async move {
                                        let result = client.request(Method::POST, &format!("/channels/{channel}/messages"), Some(json!({"content":content, "nonce":nonce, "enforce_nonce":true, "allowed_mentions":{"parse":[]}}))).await;
                                        emit(&events, generation, Event::Sent { nonce, result }).await;
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

#[derive(Default)]
struct Session {
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

async fn gateway(api: Api, events: Sender<Envelope>, generation: u64) {
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
        let result = connection(&api, &events, generation, &mut session).await;
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
    drive_connection(socket, api, events, generation, session).await
}

async fn drive_connection<S>(
    mut socket: tokio_tungstenite::WebSocketStream<S>,
    api: &Api,
    events: &Sender<Envelope>,
    generation: u64,
    session: &mut Session,
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
    loop {
        tokio::select! {
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
                            emit(events, generation, Event::Status("Connected".into())).await;
                        }
                        if kind == "RESUMED" { emit(events, generation, Event::Status("Connected".into())).await; }
                        if matches!(kind.as_str(), "READY" | "GUILD_CREATE" | "GUILD_UPDATE" | "GUILD_DELETE" | "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "CHANNEL_DELETE" | "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "MESSAGE_DELETE" | "MESSAGE_DELETE_BULK" | "CALL_CREATE") {
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

pub fn guild_from_ready(value: &Value) -> Option<Guild> {
    Some(Guild {
        id: value["id"].as_str()?.into(),
        name: value["name"]
            .as_str()
            .or_else(|| value["properties"]["name"].as_str())
            .unwrap_or("Server")
            .into(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
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
