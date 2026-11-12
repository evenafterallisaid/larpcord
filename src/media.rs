use std::{
    cell::RefCell,
    collections::HashMap,
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::RenderImage;
use image::{Frame, ImageReader};

const BUDGET: usize = 24 * 1024 * 1024;
const ENTRIES: usize = 128;

pub fn cdn_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            matches!(host, "cdn.discordapp.com" | "media.discordapp.net")
                || host
                    .strip_prefix("images-ext-")
                    .and_then(|h| h.strip_suffix(".discordapp.net"))
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
}

pub fn avatar(id: &str, hash: Option<&str>) -> Option<String> {
    let hash = hash?;
    if id.is_empty()
        || !id.bytes().all(|b| b.is_ascii_digit())
        || hash.is_empty()
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'a' || b == b'_')
    {
        return None;
    }
    Some(format!(
        "https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128"
    ))
}

struct Entry {
    image: Option<Arc<RenderImage>>,
    bytes: usize,
    used: u64,
    pending: bool,
    attempted: Instant,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Entry>,
    clock: u64,
    bytes: usize,
}

pub struct Loaded {
    pub url: String,
    pub image: Option<Arc<RenderImage>>,
    bytes: usize,
}

pub struct Media {
    tx: async_channel::Sender<String>,
    cache: RefCell<Cache>,
}

impl Media {
    pub fn new() -> (Self, async_channel::Receiver<Loaded>) {
        let (tx, requests) = async_channel::bounded::<String>(32);
        let (results, rx) = async_channel::bounded(8);
        std::thread::Builder::new()
            .name("larpcord-media".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .expect("media runtime");
                runtime.block_on(async move {
                    let Ok(client) = reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .timeout(Duration::from_secs(20))
                        .build()
                    else {
                        return;
                    };
                    let gate = Arc::new(tokio::sync::Semaphore::new(3));
                    while let Ok(url) = requests.recv().await {
                        let Ok(permit) = gate.clone().acquire_owned().await else {
                            break;
                        };
                        let client = client.clone();
                        let results = results.clone();
                        tokio::spawn(async move {
                            let data = download(&client, &url).await;
                            let image = match data {
                                Some(data) => tokio::task::spawn_blocking(move || decode(data))
                                    .await
                                    .ok()
                                    .flatten(),
                                None => None,
                            };
                            let bytes = image
                                .as_ref()
                                .and_then(|i| i.as_bytes(0))
                                .map_or(0, |b| b.len());
                            let _ = results.send(Loaded { url, image, bytes }).await;
                            drop(permit);
                        });
                    }
                });
            })
            .expect("media worker");
        (
            Self {
                tx,
                cache: RefCell::new(Cache::default()),
            },
            rx,
        )
    }

    // requests happen only for rendered rows. failed images retry on a later render, without a timer.
    pub fn get(&self, url: &str) -> Option<Arc<RenderImage>> {
        if !cdn_url(url) {
            return None;
        }
        let mut cache = self.cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        if let Some(entry) = cache.entries.get_mut(url) {
            entry.used = clock;
            if entry.pending
                || entry.image.is_some()
                || entry.attempted.elapsed() < Duration::from_secs(60)
            {
                return entry.image.clone();
            }
        }
        if self.tx.try_send(url.to_string()).is_ok() {
            cache.entries.insert(
                url.to_string(),
                Entry {
                    image: None,
                    bytes: 0,
                    used: clock,
                    pending: true,
                    attempted: Instant::now(),
                },
            );
        }
        None
    }

    pub fn receive(&self, loaded: Loaded) {
        let mut cache = self.cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        if let Some(old) = cache.entries.remove(&loaded.url) {
            cache.bytes -= old.bytes;
        }
        cache.bytes += loaded.bytes;
        cache.entries.insert(
            loaded.url,
            Entry {
                image: loaded.image,
                bytes: loaded.bytes,
                used: clock,
                pending: false,
                attempted: Instant::now(),
            },
        );
        while cache.bytes > BUDGET || cache.entries.len() > ENTRIES {
            let Some(key) = cache
                .entries
                .iter()
                .filter(|(_, e)| !e.pending)
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(entry) = cache.entries.remove(&key) {
                cache.bytes -= entry.bytes;
            }
        }
    }
}

async fn download(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    if !cdn_url(url) {
        return None;
    }
    let mut response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|n| n > 8 * 1024 * 1024)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

fn decode(bytes: Vec<u8>) -> Option<Arc<RenderImage>> {
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?.thumbnail(1024, 768);
    let mut pixels = image.into_rgba8();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)])))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_media_never_uses_an_arbitrary_origin() {
        for url in [
            "https://cdn.discordapp.com/avatars/1/x.png",
            "https://images-ext-1.discordapp.net/external/a",
        ] {
            assert!(cdn_url(url));
        }
        for url in [
            "http://cdn.discordapp.com/a",
            "https://cdn.discordapp.com.evil.test/a",
            "https://evil.test/a",
            "https://user:pass@cdn.discordapp.com/a",
            "https://cdn.discordapp.com:444/a",
        ] {
            assert!(!cdn_url(url));
        }
    }
    #[test]
    fn decoded_cache_evicts_old_rows_and_keeps_recent_images() {
        let (tx, _rx) = async_channel::bounded(1);
        let media = Media {
            tx,
            cache: RefCell::new(Cache::default()),
        };
        for index in 0..160 {
            let pixels = image::RgbaImage::new(64, 64);
            media.receive(Loaded {
                url: format!("https://cdn.discordapp.com/{index}.png"),
                image: Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)]))),
                bytes: 64 * 64 * 4,
            });
        }
        assert_eq!(media.cache.borrow().entries.len(), ENTRIES);
        assert!(
            !media
                .cache
                .borrow()
                .entries
                .contains_key("https://cdn.discordapp.com/0.png")
        );
        assert!(media.get("https://cdn.discordapp.com/32.png").is_some());
        media.receive(Loaded {
            url: "https://cdn.discordapp.com/next.png".into(),
            image: None,
            bytes: 0,
        });
        assert!(
            media
                .cache
                .borrow()
                .entries
                .contains_key("https://cdn.discordapp.com/32.png")
        );
        for index in 0..10 {
            let pixels = image::RgbaImage::new(1024, 768);
            media.receive(Loaded {
                url: format!("https://cdn.discordapp.com/large{index}.png"),
                image: Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)]))),
                bytes: 1024 * 768 * 4,
            });
        }
        assert!(media.cache.borrow().bytes <= BUDGET);
    }

    #[test]
    fn decode_is_bounded_and_converts_color_channels() {
        let image = image::RgbaImage::from_pixel(2000, 1000, image::Rgba([200, 80, 30, 255]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let decoded = decode(bytes.into_inner()).unwrap();
        assert_eq!(&decoded.as_bytes(0).unwrap()[..4], &[30, 80, 200, 255]);
        assert!(decoded.as_bytes(0).unwrap().len() <= 1024 * 768 * 4);
        assert!(decode(b"not an image".to_vec()).is_none());
    }
}
