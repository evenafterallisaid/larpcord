//! Demand-driven GIF frames. Keep the source and a tiny queue, not the whole decoded loop.
use super::*;
use std::sync::mpsc;

type Frames = Box<dyn Iterator<Item = image::ImageResult<Frame>>>;

pub struct AnimatedFrame {
    pub image: Arc<RenderImage>,
    pub delay: Duration,
}
pub struct Stream {
    pub requests: mpsc::SyncSender<()>,
    pub frames: async_channel::Receiver<Option<AnimatedFrame>>,
}

pub fn image_url(url: &str) -> bool {
    (cdn_url(url) || gif_preview(url))
        && reqwest::Url::parse(url).ok().is_some_and(|u| {
            let path = u.path().to_ascii_lowercase();
            path.ends_with(".gif") || path.ends_with(".apng")
        })
}

fn frames(bytes: Arc<[u8]>) -> Option<Frames> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    match image::guess_format(&bytes).ok()? {
        image::ImageFormat::Gif => {
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?;
            decoder.set_limits(limits).ok()?;
            Some(Box::new(decoder.into_frames()))
        }
        image::ImageFormat::Png => {
            let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).ok()?;
            decoder.set_limits(limits).ok()?;
            if !decoder.is_apng().ok()? {
                return None;
            }
            Some(Box::new(decoder.apng().ok()?.into_frames()))
        }
        _ => None,
    }
}

struct Decoder {
    bytes: Arc<[u8]>,
    frames: Frames,
}
impl Decoder {
    fn new(bytes: Vec<u8>) -> Option<Self> {
        let bytes: Arc<[u8]> = bytes.into();
        Some(Self {
            frames: frames(bytes.clone())?,
            bytes,
        })
    }
    fn next(&mut self) -> Option<AnimatedFrame> {
        let frame = match self.frames.next() {
            Some(frame) => frame.ok()?,
            None => {
                // Rewind only after the complete source cycle, including long GIFs.
                self.frames = frames(self.bytes.clone())?;
                self.frames.next()?.ok()?
            }
        };
        let (ms, denom) = frame.delay().numer_denom_ms();
        let delay =
            Duration::from_secs_f64((ms as f64 / denom.max(1) as f64 / 1000.).clamp(0.016, 10.));
        let image = image::DynamicImage::ImageRgba8(frame.into_buffer());
        let mut pixels = image
            .thumbnail(image.width().min(800), image.height().min(600))
            .into_rgba8();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Some(AnimatedFrame {
            image: Arc::new(RenderImage::new(vec![Frame::new(pixels)])),
            delay,
        })
    }
}

pub fn start(url: String) -> Stream {
    let (requests, requested) = mpsc::sync_channel(1);
    let (results, frames) = async_channel::bounded(1);
    let _ = std::thread::Builder::new()
        .name("larpcord-gif".into())
        .spawn(move || {
            if !image_url(&url) || requested.recv().is_err() {
                return;
            }
            let cache = disk::Disk::new();
            let bytes = cache.read(&url).or_else(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()?;
                runtime.block_on(async {
                    let client = reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .build()
                        .ok()?;
                    let bytes = tokio::select! {
                        _ = results.closed() => return None,
                        bytes = download(&client, &url) => bytes?,
                    };
                    cache.write(&url, &bytes);
                    Some(bytes)
                })
            });
            let mut decoder = bytes.and_then(Decoder::new);
            loop {
                if results.is_closed() {
                    break;
                }
                let frame = decoder.as_mut().and_then(Decoder::next);
                let failed = frame.is_none();
                if results.send_blocking(frame).is_err() || failed || requested.recv().is_err() {
                    break;
                }
            }
        });
    Stream { requests, frames }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_gif_keeps_full_resolution_and_the_complete_loop() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for color in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
                encoder
                    .encode_frame(Frame::from_parts(
                        image::RgbaImage::from_pixel(400, 240, image::Rgba(color)),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        let mut decoder = Decoder::new(bytes).unwrap();
        for color in [
            [0, 0, 255, 255],
            [0, 255, 0, 255],
            [255, 0, 0, 255],
            [0, 0, 255, 255],
        ] {
            let frame = decoder.next().unwrap();
            assert_eq!(frame.image.size(0).width.0, 400);
            assert_eq!(frame.image.size(0).height.0, 240);
            assert_eq!(&frame.image.as_bytes(0).unwrap()[..4], &color);
            assert_eq!(frame.delay, Duration::from_millis(100));
            assert_eq!(frame.image.frame_count(), 1);
        }
        assert!(image_url(
            "https://cdn.discordapp.com/attachments/1/2/a.gif?ex=abc"
        ));
        assert!(!image_url("https://untrusted.invalid/a.gif"));
    }
}
