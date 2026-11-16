use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::RenderImage;
use image::{AnimationDecoder, Frame, ImageDecoder, ImageReader};
#[path = "media/animation.rs"]
pub mod animation;
#[path = "media/disk.rs"]
mod disk;

const BUDGET: usize = 12 * 1024 * 1024;
const IDENTITY_BUDGET: usize = 8 * 1024 * 1024;
const ENTRIES: usize = 384;

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

pub fn gif_preview(value: &str) -> bool {
    value.len() <= 2048
        && reqwest::Url::parse(value).ok().is_some_and(|url| {
            url.scheme() == "https"
                && url.port_or_known_default() == Some(443)
                && url.username().is_empty()
                && url.password().is_none()
                && url.host_str().is_some_and(|h| {
                    matches!(
                        h,
                        "media.tenor.com"
                            | "c.tenor.com"
                            | "static.klipy.com"
                            | "static1.klipy.com"
                            | "static2.klipy.com"
                    )
                })
                && [".gif", ".png", ".jpg", ".jpeg", ".webp"]
                    .iter()
                    .any(|ext| url.path().ends_with(ext))
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
        "https://cdn.discordapp.com/avatars/{id}/{hash}.webp?size=128"
    ))
}

pub fn group_icon(id: &str, hash: Option<&str>) -> Option<String> {
    let hash = hash?;
    if id.is_empty()
        || id.len() > 20
        || !id.bytes().all(|b| b.is_ascii_digit())
        || hash.is_empty()
        || hash.len() > 128
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'a' || b == b'_')
    {
        return None;
    }
    Some(format!(
        "https://cdn.discordapp.com/channel-icons/{id}/{hash}.webp?size=128"
    ))
}

pub fn sticker(id: &str, format: u8, motion: bool) -> Option<String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let suffix = if motion && matches!(format, 2 | 4) {
        "#motion"
    } else {
        ""
    };
    match format {
        1 | 2 => Some(format!(
            "https://cdn.discordapp.com/stickers/{id}.png{suffix}"
        )),
        4 => Some(format!(
            "https://media.discordapp.net/stickers/{id}.gif{suffix}"
        )),
        _ => None,
    }
}

fn identity_asset(url: &str) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|url| {
        let path = url.path();
        [
            "/avatars/",
            "/icons/",
            "/emojis/",
            "/channel-icons/",
            "/badge-icons/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
            || path.contains("/avatars/")
    })
}

fn decode_bounds(url: &str) -> (u32, u32) {
    if url.contains("/avatar-decoration-presets/") {
        return (192, 192);
    }
    if url.contains("/nameplates/") {
        return (640, 128);
    }
    if gif_preview(url) {
        return (400, 264);
    }
    if url.contains("#motion") {
        (128, 192)
    } else if url.contains("/media/v1/collectibles-shop/")
        || url.contains("/assets/profile_effects/")
    {
        (480, 640)
    } else if url.contains("/stickers/") {
        (192, 192)
    } else if url.contains("/icons/")
        || url.contains("/emojis/")
        || url.contains("/channel-icons/")
        || url.contains("/badge-icons/")
    {
        (128, 128)
    } else if identity_asset(url) {
        (256, 256)
    } else if url.contains("/banners/") {
        (640, 256)
    } else {
        (640, 480)
    }
}

// release old tiles after frames already submitted to the gpu have finished using them.
pub fn retire(images: Vec<Arc<RenderImage>>, window: &mut gpui::Window, frames: u8) {
    if images.is_empty() {
        return;
    }
    window.on_next_frame(move |window, cx| {
        if frames == 0 {
            for image in images {
                cx.drop_image(image, Some(window));
            }
            window.refresh();
        } else {
            retire(images, window, frames - 1);
        }
    });
    window.refresh();
}

struct Entry {
    identity: bool,
    image: Option<Arc<RenderImage>>,
    bytes: usize,
    used: u64,
    pending: bool,
    attempted: Instant,
    failures: u8,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Entry>,
    clock: u64,
    bytes: usize,
    visible: HashSet<String>,
}

pub struct Loaded {
    pub url: String,
    pub image: Option<Arc<RenderImage>>,
    bytes: usize,
}

pub struct Media {
    tx: async_channel::Sender<String>,
    portraits: async_channel::Sender<String>,
    cache: RefCell<Cache>,
}

impl Media {
    pub fn new() -> (Self, async_channel::Receiver<Loaded>) {
        let (tx, requests) = async_channel::bounded::<String>(32);
        let (portraits, portrait_requests) = async_channel::bounded::<String>(128);
        let (results, rx) = async_channel::bounded(8);
        // headless checks never download public media.
        if cfg!(test) {
            return (
                Self {
                    tx,
                    portraits,
                    cache: RefCell::new(Cache::default()),
                },
                rx,
            );
        }
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
                    let disk = Arc::new(disk::Disk::new());
                    let identity_decoders = Arc::new(tokio::sync::Semaphore::new(1));
                    let media_decoders = Arc::new(tokio::sync::Semaphore::new(1));
                    let mut workers = tokio::task::JoinSet::new();
                    for (requests, count, identity) in
                        [(portrait_requests, 4, true), (requests, 2, false)]
                    {
                        for _ in 0..count {
                            let (client, requests, results, disk, decoders) = (
                                client.clone(),
                                requests.clone(),
                                results.clone(),
                                disk.clone(),
                                if identity {
                                    identity_decoders.clone()
                                } else {
                                    media_decoders.clone()
                                },
                            );
                            workers.spawn(async move {
                                while let Ok(url) = requests.recv().await {
                                    let cache = disk.clone();
                                    let key = url.clone();
                                    let cached =
                                        tokio::task::spawn_blocking(move || cache.read(&key))
                                            .await
                                            .ok()
                                            .flatten();
                                    let from_disk = cached.is_some();
                                    let data = match cached {
                                        Some(bytes) => Some(bytes),
                                        None => download(&client, &url).await,
                                    };
                                    let image = if let Some(data) = data {
                                        let Ok(_permit) = decoders.acquire().await else {
                                            break;
                                        };
                                        let key = url.clone();
                                        let cache = disk.clone();
                                        tokio::task::spawn_blocking(move || {
                                            let image = if key.ends_with("#motion") {
                                                decode_motion(data.clone(), decode_bounds(&key))
                                            } else {
                                                decode(data.clone(), decode_bounds(&key))
                                            };
                                            if image.is_some() && !from_disk {
                                                cache.write(&key, &data);
                                            }
                                            if image.is_none() && from_disk {
                                                cache.remove(&key);
                                            }
                                            image
                                        })
                                        .await
                                        .ok()
                                        .flatten()
                                    } else {
                                        None
                                    };
                                    let bytes = image.as_ref().map_or(0, |i| {
                                        (0..i.frame_count())
                                            .filter_map(|n| i.as_bytes(n))
                                            .map(|b| b.len())
                                            .sum()
                                    });
                                    if results.send(Loaded { url, image, bytes }).await.is_err() {
                                        break;
                                    }
                                }
                            });
                        }
                    }
                    while workers.join_next().await.is_some() {}
                });
            })
            .expect("media worker");
        (
            Self {
                tx,
                portraits,
                cache: RefCell::new(Cache::default()),
            },
            rx,
        )
    }

    pub fn begin_frame(&self) {
        self.cache.borrow_mut().visible.clear();
    }

    // requests happen only for rendered rows. failed images retry on a later render, without a timer.
    pub fn get(&self, url: &str) -> Option<Arc<RenderImage>> {
        if !cdn_url(url) && !gif_preview(url) {
            return None;
        }
        let mut cache = self.cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        if cache.visible.len() < 256 {
            cache.visible.insert(url.to_string());
        }
        if let Some(entry) = cache.entries.get_mut(url) {
            entry.used = clock;
            if entry.pending
                || entry.image.is_some()
                || entry.attempted.elapsed()
                    < Duration::from_secs(if entry.failures == 1 { 2 } else { 60 })
            {
                return entry.image.clone();
            }
        }
        let failures = cache.entries.get(url).map_or(0, |e| e.failures);
        let queue = if identity_asset(url) {
            &self.portraits
        } else {
            &self.tx
        };
        if queue.try_send(url.to_string()).is_ok() {
            cache.entries.insert(
                url.to_string(),
                Entry {
                    identity: identity_asset(url),
                    image: None,
                    bytes: 0,
                    used: clock,
                    pending: true,
                    attempted: Instant::now(),
                    failures,
                },
            );
        }
        None
    }

    pub fn receive(&self, loaded: Loaded) -> Vec<Arc<RenderImage>> {
        let mut retired = Vec::new();
        let mut cache = self.cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        let mut failures = 0;
        if let Some(old) = cache.entries.remove(&loaded.url) {
            failures = old.failures;
            cache.bytes -= old.bytes;
            retired.extend(old.image);
        }
        cache.bytes += loaded.bytes;
        let identity = identity_asset(&loaded.url);
        failures = if loaded.image.is_none() {
            failures.saturating_add(1)
        } else {
            0
        };
        cache.entries.insert(
            loaded.url,
            Entry {
                identity,
                image: loaded.image,
                bytes: loaded.bytes,
                used: clock,
                pending: false,
                attempted: Instant::now(),
                failures,
            },
        );
        // portraits and icons have their own allowance, so a large attachment cannot evict them.
        for identity in [true, false] {
            let budget = if identity { IDENTITY_BUDGET } else { BUDGET };
            let entries = if identity { ENTRIES * 2 } else { ENTRIES };
            loop {
                let (bytes, count) = cache
                    .entries
                    .iter()
                    .filter(|(_, entry)| entry.identity == identity)
                    .fold((0, 0), |(bytes, count), (_, entry)| {
                        (bytes + entry.bytes, count + 1)
                    });
                if bytes <= budget && count <= entries {
                    break;
                }
                let key = cache
                    .entries
                    .iter()
                    .filter(|(url, e)| {
                        !e.pending && e.identity == identity && !cache.visible.contains(*url)
                    })
                    .min_by_key(|(_, e)| e.used)
                    .map(|(key, _)| key.clone());
                let Some(key) = key else {
                    // a visible working set may exceed the soft allowance. never cycle its images
                    // through placeholders on every repaint; retain a bounded working set instead.
                    let hard_budget = budget * 3;
                    if bytes > hard_budget || count > entries * 2 {
                        let key = cache
                            .entries
                            .iter()
                            .filter(|(_, e)| e.identity == identity && !e.pending)
                            .max_by_key(|(_, e)| e.used)
                            .map(|(key, _)| key.clone());
                        if let Some(key) = key
                            && let Some(entry) = cache.entries.get_mut(&key)
                        {
                            let removed = entry.bytes;
                            entry.bytes = 0;
                            retired.extend(entry.image.take());
                            cache.bytes -= removed;
                        }
                    }
                    break;
                };
                if let Some(entry) = cache.entries.remove(&key) {
                    cache.bytes -= entry.bytes;
                    retired.extend(entry.image);
                }
            }
        }
        retired
    }

    pub fn retry_after(&self) -> Option<Duration> {
        let cache = self.cache.borrow();
        cache
            .entries
            .iter()
            .filter(|(url, e)| {
                cache.visible.contains(*url) && !e.pending && e.image.is_none() && e.failures == 1
            })
            .map(|(_, e)| Duration::from_secs(2).saturating_sub(e.attempted.elapsed()))
            .min()
    }
}

async fn download(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    if !cdn_url(url) && !gif_preview(url) {
        return None;
    }
    let mut response = client
        .get(url)
        .timeout(Duration::from_secs(if identity_asset(url) {
            8
        } else {
            20
        }))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
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

fn decode(bytes: Vec<u8>, bounds: (u32, u32)) -> Option<Arc<RenderImage>> {
    if bytes.len() <= 512 * 1024
        && std::str::from_utf8(&bytes).ok().is_some_and(|s| {
            s.trim_start().starts_with("<svg")
                || (s.trim_start().starts_with("<?xml") && s.contains("<svg"))
        })
    {
        let options = resvg::usvg::Options {
            image_href_resolver: resvg::usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..Default::default()
        };
        let tree = resvg::usvg::Tree::from_data(&bytes, &options).ok()?;
        let source = tree.size();
        if source.width() > 8192. || source.height() > 8192. {
            return None;
        }
        let scale = (bounds.0 as f32 / source.width())
            .min(bounds.1 as f32 / source.height())
            .min(1.);
        let mut pixmap = resvg::tiny_skia::Pixmap::new(
            (source.width() * scale).ceil().max(1.) as u32,
            (source.height() * scale).ceil().max(1.) as u32,
        )?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let mut pixels = Vec::with_capacity(pixmap.data().len());
        for pixel in pixmap.pixels() {
            let c = pixel.demultiply();
            pixels.extend_from_slice(&[c.blue(), c.green(), c.red(), c.alpha()]);
        }
        let image = image::RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixels)?;
        return Some(Arc::new(RenderImage::new(vec![Frame::new(image)])));
    }
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = image.thumbnail(bounds.0.min(image.width()), bounds.1.min(image.height()));
    let mut pixels = image.into_rgba8();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)])))
}

pub fn motion_url(url: String, enabled: bool) -> String {
    if !enabled {
        return url;
    }
    let mut url = url.replace("/static", "/animated");
    if url.contains("/avatar-decoration-presets/") {
        url = url.replace("passthrough=false", "passthrough=true");
    }
    if url.contains("/assets/profile_effects/")
        || url.contains("/animated")
        || url.contains("passthrough=true")
    {
        url.push_str("#motion");
    }
    url
}
fn decode_motion(bytes: Vec<u8>, bounds: (u32, u32)) -> Option<Arc<RenderImage>> {
    let fallback = || decode(bytes.clone(), bounds);
    let format = image::guess_format(&bytes).ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    let frames = match format {
        image::ImageFormat::Png => {
            let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(&bytes)).ok()?;
            decoder.set_limits(limits).ok()?;
            if !decoder.is_apng().ok()? {
                return fallback();
            }
            decoder.apng().ok()?.into_frames()
        }
        image::ImageFormat::WebP => {
            let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(&bytes)).ok()?;
            decoder.set_limits(limits).ok()?;
            if !decoder.has_animation() {
                return fallback();
            }
            decoder.into_frames()
        }
        image::ImageFormat::Gif => {
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(&bytes)).ok()?;
            decoder.set_limits(limits).ok()?;
            decoder.into_frames()
        }
        _ => return fallback(),
    };
    let mut decoded = Vec::new();
    let mut total = 0;
    for frame in frames {
        let frame = frame.ok()?;
        let delay = frame.delay();
        let image = image::DynamicImage::ImageRgba8(frame.into_buffer());
        let pixels = image
            .thumbnail(bounds.0.min(image.width()), bounds.1.min(image.height()))
            .into_rgba8();
        total += pixels.len();
        if decoded.len() >= 120 || total > 4 * 1024 * 1024 {
            return fallback();
        }
        let mut pixels = pixels;
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        decoded.push(Frame::from_parts(pixels, 0, 0, delay));
    }
    (!decoded.is_empty()).then(|| Arc::new(RenderImage::new(decoded)))
}

fn asset_hash(hash: &str) -> bool {
    !hash.is_empty()
        && hash.len() <= 128
        && hash.bytes().all(|b| b.is_ascii_hexdigit() || b == b'_')
}
fn asset_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}
pub fn guild_icon(id: &str, hash: Option<&str>) -> Option<String> {
    let hash = hash?;
    (asset_id(id) && asset_hash(hash))
        .then(|| format!("https://cdn.discordapp.com/icons/{id}/{hash}.webp?size=128"))
}
pub fn banner(id: &str, guild: Option<&str>, hash: &str) -> Option<String> {
    if !asset_id(id) || !asset_hash(hash) {
        return None;
    }
    let path = match guild {
        Some(guild) if asset_id(guild) => format!("guilds/{guild}/users/{id}/banners"),
        Some(_) => return None,
        None => format!("banners/{id}"),
    };
    Some(format!(
        "https://cdn.discordapp.com/{path}/{hash}.png?size=1024"
    ))
}
pub fn collectible(sku: &str) -> Option<String> {
    asset_id(sku)
        .then(|| format!("https://cdn.discordapp.com/media/v1/collectibles-shop/{sku}/static"))
}
pub fn decoration(asset: &str) -> Option<String> {
    asset_hash(asset).then(|| format!("https://cdn.discordapp.com/avatar-decoration-presets/{asset}.png?size=256&passthrough=false"))
}
pub fn member_avatar(guild: &str, user: &str, hash: &str) -> Option<String> {
    (asset_id(guild) && asset_id(user) && asset_hash(hash)).then(|| {
        format!(
            "https://cdn.discordapp.com/guilds/{guild}/users/{user}/avatars/{hash}.webp?size=256"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_svg_rasterization_is_bounded_and_keeps_alpha() {
        let svg=br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><circle cx="50" cy="50" r="45" fill="red"/></svg>"#.to_vec();
        let image = decode(svg, (32, 32)).unwrap();
        assert_eq!(image.size(0).width.0, 32);
        assert_eq!(image.size(0).height.0, 32);
        assert_eq!(image.as_bytes(0).unwrap()[3], 0);
        assert_eq!(image.as_bytes(0).unwrap()[(16 * 32 + 16) * 4 + 2], 255);
    }
    #[test]
    fn portraits_do_not_queue_behind_attachments_or_large_cosmetics() {
        let (tx, content) = async_channel::bounded(1);
        let (portraits, icons) = async_channel::bounded(4);
        let media = Media {
            tx,
            portraits,
            cache: RefCell::new(Cache::default()),
        };
        media.get("https://cdn.discordapp.com/attachments/1/image.png");
        media.get("https://cdn.discordapp.com/avatars/1/abc.png?size=128");
        media.get("https://cdn.discordapp.com/icons/2/abc.png?size=128");
        assert!(icons.try_recv().unwrap().contains("/avatars/"));
        assert!(icons.try_recv().unwrap().contains("/icons/"));
        assert!(content.try_recv().unwrap().contains("/attachments/"));
        assert!(!identity_asset(
            "https://cdn.discordapp.com/media/v1/collectibles-shop/1/static"
        ));
    }

    #[test]
    fn a_visible_failed_image_gets_one_short_retry_instead_of_a_refresh_loop() {
        let (tx, requests) = async_channel::bounded(4);
        let media = Media {
            portraits: tx.clone(),
            tx,
            cache: RefCell::new(Cache::default()),
        };
        let url = "https://cdn.discordapp.com/icons/1/abc.webp";
        media.get(url);
        requests.try_recv().unwrap();
        media.receive(Loaded {
            url: url.into(),
            image: None,
            bytes: 0,
        });
        assert!(media.retry_after().is_some());
        media
            .cache
            .borrow_mut()
            .entries
            .get_mut(url)
            .unwrap()
            .attempted = Instant::now() - Duration::from_secs(3);
        media.get(url);
        assert!(requests.try_recv().is_ok());
        media.receive(Loaded {
            url: url.into(),
            image: None,
            bytes: 0,
        });
        assert!(media.retry_after().is_none());
        media.get(url);
        assert!(requests.try_recv().is_err());
    }
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
            tx: tx.clone(),
            portraits: tx,
            cache: RefCell::new(Cache::default()),
        };
        for index in 0..ENTRIES + 32 {
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
        assert!(media.cache.borrow().bytes <= BUDGET + IDENTITY_BUDGET);
    }

    #[test]
    fn attachment_churn_keeps_portraits_stable_and_returns_every_retired_texture() {
        let (tx, _rx) = async_channel::bounded(1);
        let media = Media {
            tx: tx.clone(),
            portraits: tx,
            cache: RefCell::new(Cache::default()),
        };
        let portrait = "https://cdn.discordapp.com/avatars/1/abcdef.png?size=128";
        let image = Arc::new(RenderImage::new(vec![Frame::new(image::RgbaImage::new(
            64, 64,
        ))]));
        let portrait_id = image.id;
        assert!(
            media
                .receive(Loaded {
                    url: portrait.into(),
                    image: Some(image),
                    bytes: 16384
                })
                .is_empty()
        );
        let mut retired = 0;
        for index in 0..600 {
            let image = Arc::new(RenderImage::new(vec![Frame::new(image::RgbaImage::new(
                64, 64,
            ))]));
            retired += media
                .receive(Loaded {
                    url: format!("https://cdn.discordapp.com/attachments/{index}.png"),
                    image: Some(image),
                    bytes: 16384,
                })
                .len();
            assert_eq!(media.get(portrait).unwrap().id, portrait_id);
        }
        assert_eq!(retired, 600 - ENTRIES);
        assert_eq!(media.cache.borrow().entries.len(), ENTRIES + 1);
        assert!(media.cache.borrow().bytes <= BUDGET + IDENTITY_BUDGET);
        assert_eq!(
            decode_bounds("https://cdn.discordapp.com/icons/1/abcdef.png"),
            (128, 128)
        );
    }

    #[test]
    fn idle_repaints_do_not_evict_the_visible_working_set() {
        let (tx, _rx) = async_channel::bounded(1);
        let media = Media {
            tx: tx.clone(),
            portraits: tx,
            cache: RefCell::new(Cache::default()),
        };
        let mut ids = Vec::new();
        for index in 0..48 {
            let url = format!("https://cdn.discordapp.com/avatars/{index}/abc.png");
            media.get(&url);
            let image = Arc::new(RenderImage::new(vec![Frame::new(image::RgbaImage::new(
                256, 256,
            ))]));
            ids.push((url.clone(), image.id));
            assert!(
                media
                    .receive(Loaded {
                        url,
                        image: Some(image),
                        bytes: 256 * 256 * 4
                    })
                    .is_empty()
            );
        }
        assert!(media.cache.borrow().bytes > IDENTITY_BUDGET);
        for _ in 0..40 {
            media.begin_frame();
            for (url, id) in &ids {
                assert_eq!(media.get(url).unwrap().id, *id);
            }
        }
        media.begin_frame();
        media.receive(Loaded {
            url: "https://cdn.discordapp.com/avatars/999/abc.png".into(),
            image: None,
            bytes: 0,
        });
        assert!(media.cache.borrow().bytes <= IDENTITY_BUDGET);
    }

    #[test]
    fn animated_decoding_preserves_alpha_delay_and_accounts_for_every_frame() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for _ in 0..3 {
                encoder
                    .encode_frame(Frame::from_parts(
                        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255])),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        let decoded = decode_motion(bytes, (8, 8)).unwrap();
        assert_eq!(decoded.frame_count(), 3);
        assert_eq!(decoded.delay(1).numer_denom_ms(), (100, 1));
        assert_eq!(&decoded.as_bytes(1).unwrap()[..4], &[0, 0, 255, 255]);
        assert_eq!(
            (0..decoded.frame_count())
                .map(|n| decoded.as_bytes(n).unwrap().len())
                .sum::<usize>(),
            3 * 8 * 8 * 4
        );
    }

    #[test]
    fn decode_is_bounded_and_converts_color_channels() {
        let image = image::RgbaImage::from_pixel(2000, 1000, image::Rgba([200, 80, 30, 255]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let decoded = decode(bytes.into_inner(), (640, 480)).unwrap();
        assert_eq!(&decoded.as_bytes(0).unwrap()[..4], &[30, 80, 200, 255]);
        assert!(decoded.as_bytes(0).unwrap().len() <= 640 * 480 * 4);
        assert!(decode(b"not an image".to_vec(), (640, 480)).is_none());
    }
}
