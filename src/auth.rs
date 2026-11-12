use std::{sync::Arc, time::Duration};

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use futures_util::{SinkExt, StreamExt};
use qrcode::{Color, QrCode};
use rsa::{Oaep, RsaPrivateKey, pkcs8::EncodePublicKey, rand_core::OsRng};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::time::{Instant, sleep_until};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        Message, client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig,
    },
};
use zeroize::Zeroizing;

pub struct QrImage {
    pub side: usize,
    pub dark: Vec<bool>,
}

pub enum AuthEvent {
    Code(Arc<QrImage>),
    Confirm,
    Token(Zeroizing<String>),
    Failed(String),
}

pub async fn remote_login(events: async_channel::Sender<AuthEvent>) {
    if let Err(error) = run(&events).await {
        let _ = events.send(AuthEvent::Failed(error)).await;
    }
}

fn decrypt(key: &RsaPrivateKey, encoded: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Discord returned an invalid authentication challenge.".to_string())?;
    key.decrypt_blinded(&mut OsRng, Oaep::new::<Sha256>(), &bytes)
        .map(Zeroizing::new)
        .map_err(|_| "The authentication challenge could not be decrypted.".into())
}

fn qr(fingerprint: &str) -> Result<QrImage, String> {
    if fingerprint.is_empty()
        || fingerprint.len() > 128
        || !fingerprint
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Discord returned an invalid QR fingerprint.".into());
    }
    let code = QrCode::new(format!("https://discord.com/ra/{fingerprint}"))
        .map_err(|_| "Could not generate the login QR code.".to_string())?;
    Ok(QrImage {
        side: code.width(),
        dark: code
            .to_colors()
            .iter()
            .map(|color| *color == Color::Dark)
            .collect(),
    })
}

async fn run(events: &async_channel::Sender<AuthEvent>) -> Result<(), String> {
    let key = tokio::task::spawn_blocking(|| RsaPrivateKey::new(&mut OsRng, 2048))
        .await
        .map_err(|_| "Could not generate a login key.".to_string())?
        .map_err(|_| "Could not generate a login key.".to_string())?;
    let public = key
        .to_public_key()
        .to_public_key_der()
        .map_err(|_| "Could not encode the login key.".to_string())?;
    let mut request = "wss://remote-auth-gateway.discord.gg/?v=2"
        .into_client_request()
        .map_err(|_| "Invalid authentication endpoint.".to_string())?;
    request
        .headers_mut()
        .insert("Origin", HeaderValue::from_static("https://discord.com"));
    let config = WebSocketConfig::default()
        .max_message_size(Some(128 * 1024))
        .max_frame_size(Some(128 * 1024));
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(20),
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| "The QR connection timed out.".to_string())?
    .map_err(|_| "Could not connect to Discord's QR login service.".to_string())?;
    let mut expires = Instant::now() + Duration::from_secs(20);
    let mut heartbeat = expires;
    let mut interval = Duration::from_secs(40);
    let mut acknowledged = true;
    let mut initialized = false;
    loop {
        tokio::select! {
            _ = sleep_until(expires) => return Err("This QR code has expired. Generate a new one to continue.".into()),
            _ = sleep_until(heartbeat), if initialized => {
                if !acknowledged { return Err("The QR connection was interrupted. Generate a new code.".into()); }
                socket.send(Message::Text(json!({"op":"heartbeat"}).to_string().into())).await.map_err(|_| "The QR connection was interrupted.".to_string())?;
                acknowledged = false;
                heartbeat = Instant::now() + interval;
            }
            frame = socket.next() => {
                let frame = frame.ok_or_else(|| "Discord closed the QR login connection.".to_string())?.map_err(|_| "The QR connection was interrupted.".to_string())?;
                match frame {
                    Message::Ping(bytes) => { socket.send(Message::Pong(bytes)).await.map_err(|_| "The QR connection was interrupted.".to_string())?; continue; }
                    Message::Close(_) => return Err("Discord ended this QR login attempt. Generate a new code.".into()),
                    Message::Text(_) => {},
                    _ => continue,
                }
                let data: Value = serde_json::from_slice(&frame.into_data()).map_err(|_| "Discord returned an invalid QR response.".to_string())?;
                match data["op"].as_str().unwrap_or_default() {
                    "hello" if !initialized => {
                        let timeout = data["timeout_ms"].as_u64().ok_or("Missing QR expiry.")?.clamp(1000, 300000);
                        interval = Duration::from_millis(data["heartbeat_interval"].as_u64().ok_or("Missing QR heartbeat.")?.clamp(1000, 60000));
                        heartbeat = Instant::now() + interval;
                        expires = Instant::now() + Duration::from_millis(timeout);
                        initialized = true;
                        socket.send(Message::Text(json!({"op":"init","encoded_public_key":STANDARD.encode(public.as_bytes())}).to_string().into())).await.map_err(|_| "The QR handshake was interrupted.".to_string())?;
                    }
                    "heartbeat_ack" => acknowledged = true,
                    "nonce_proof" => {
                        let nonce = decrypt(&key, data["encrypted_nonce"].as_str().ok_or("Missing authentication challenge.")?)?;
                        let proof = URL_SAFE_NO_PAD.encode(Sha256::digest(nonce.as_slice()));
                        socket.send(Message::Text(json!({"op":"nonce_proof","proof":proof}).to_string().into())).await.map_err(|_| "The QR handshake was interrupted.".to_string())?;
                    }
                    "pending_remote_init" => {
                        let image = qr(data["fingerprint"].as_str().ok_or("Missing QR fingerprint.")?)?;
                        events.send(AuthEvent::Code(Arc::new(image))).await.map_err(|_| "Login was closed.".to_string())?;
                    }
                    "pending_ticket" => { events.send(AuthEvent::Confirm).await.map_err(|_| "Login was closed.".to_string())?; }
                    "pending_login" => {
                        let ticket = Zeroizing::new(data["ticket"].as_str().ok_or("Missing login ticket.")?.to_string());
                        let client = reqwest::Client::builder().timeout(Duration::from_secs(25)).user_agent(concat!("Larpcord/", env!("CARGO_PKG_VERSION"))).build().map_err(|_| "Could not initialize login.".to_string())?;
                        let response = client.post("https://discord.com/api/v10/users/@me/remote-auth/login").header("Origin", "https://discord.com").json(&json!({"ticket":ticket.as_str()}))
                            .send().await.map_err(|_| "Could not finish signing in. Generate a new QR code.".to_string())?;
                        if response.status().as_u16() == 429 { return Err("Discord is limiting login attempts. Wait before trying again.".into()); }
                        let success = response.status().is_success();
                        let data: Value = response.json().await.map_err(|_| "Discord returned an unexpected login response.".to_string())?;
                        if data.get("captcha_key").is_some() { return Err("Discord requested a CAPTCHA for this login. CAPTCHA support is still pending. Try a new QR code later.".into()); }
                        if !success { return Err("Discord could not finish this login. Generate a new QR code.".into()); }
                        let token = decrypt(&key, data["encrypted_token"].as_str().ok_or("Discord did not return a session.")?)?;
                        let token = std::str::from_utf8(token.as_slice()).map_err(|_| "Discord returned an invalid session.".to_string())?;
                        events.send(AuthEvent::Token(Zeroizing::new(token.into()))).await.map_err(|_| "Login was closed.".to_string())?;
                        let _ = socket.close(None).await;
                        return Ok(());
                    }
                    "cancel" => return Err("Sign-in was cancelled on your phone. Generate a new QR code when ready.".into()),
                    _ => {},
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_payload_accepts_only_discord_fingerprints() {
        assert!(qr("sample_Fingerprint-0123").is_ok());
        assert!(qr("https://elsewhere.test").is_err());
        assert!(qr("../redirect").is_err());
        let code = qr("sample").unwrap();
        assert_eq!(code.dark.len(), code.side * code.side);
    }

    #[test]
    fn encrypted_session_roundtrip_uses_oaep_sha256() {
        let key = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let encrypted = key
            .to_public_key()
            .encrypt(&mut OsRng, Oaep::new::<Sha256>(), b"fixture-session")
            .unwrap();
        assert_eq!(
            decrypt(&key, &STANDARD.encode(encrypted))
                .unwrap()
                .as_slice(),
            b"fixture-session"
        );
    }

    #[tokio::test]
    #[ignore = "contacts discord's unauthenticated remote auth gateway, never completes a login"]
    async fn discord_accepts_the_qr_handshake() {
        let (tx, rx) = async_channel::bounded(8);
        let task = tokio::spawn(remote_login(tx));
        let event = tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .unwrap()
            .unwrap();
        task.abort();
        match event {
            AuthEvent::Code(code) => assert!(code.side > 20),
            AuthEvent::Failed(error) => panic!("{error}"),
            _ => panic!("unexpected authentication state"),
        }
    }
}
