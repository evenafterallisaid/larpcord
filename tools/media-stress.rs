//! real-renderer regression check: churn images, retain a portrait, then leave the window idle.
//! no account, credentials, network requests or discord messages are involved.
//! run as a normal binary: examples enable gpui test-support through dev dependencies.
#![allow(dead_code)]
include!("../src/media.rs");
use gpui::{prelude::*, *};

struct Stress {
    media: Media,
    visible: Vec<Arc<RenderImage>>,
    portrait: Arc<RenderImage>,
    round: u32,
}
impl Stress {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (tx, _) = async_channel::bounded(1);
        let portrait = Arc::new(RenderImage::new(vec![Frame::new(
            image::RgbaImage::from_pixel(80, 80, image::Rgba([180, 90, 60, 255])),
        )]));
        cx.spawn_in(window, async move |view, cx| {
            for round in 1..=1200 {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if view
                    .update_in(cx, |this, window, cx| {
                        this.round = round;
                        this.visible.clear();
                        for index in 0..3 {
                            let image = Arc::new(RenderImage::new(vec![Frame::new(
                                image::RgbaImage::from_pixel(
                                    320,
                                    240,
                                    image::Rgba([
                                        (round % 255) as u8,
                                        (index * 70) as u8,
                                        100,
                                        255,
                                    ]),
                                ),
                            )]));
                            this.visible.push(image.clone());
                            let retired = this.media.receive(Loaded {
                                url: format!(
                                    "https://cdn.discordapp.com/attachments/{round}-{index}.png"
                                ),
                                image: Some(image),
                                bytes: 320 * 240 * 4,
                            });
                            retire(retired, window, 3);
                        }
                        if round % 300 == 0 {
                            eprintln!(
                                "round {round}: decoded cache {} bytes; {} entries",
                                this.media.cache.borrow().bytes,
                                this.media.cache.borrow().entries.len()
                            );
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
            eprintln!("soak complete: 3600 uploaded images, 1054 mib churned; now idle");
        })
        .detach();
        Self {
            media: Media {
                tx,
                cache: RefCell::new(Cache::default()),
            },
            visible: Vec::new(),
            portrait,
            round: 0,
        }
    }
}
impl Render for Stress {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x111214))
            .text_color(rgb(0xffffff))
            .p_4()
            .gap_4()
            .child(format!("image atlas soak: {} / 1200 rounds", self.round))
            .child(img(self.portrait.clone()).size(px(40.)))
            .child(
                div().flex().gap_2().children(
                    self.visible
                        .iter()
                        .map(|image| img(image.clone()).w(px(240.)).h(px(180.))),
                ),
            )
    }
}
fn main() {
    Application::new().run(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(800.), px(350.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Larpcord texture soak".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Stress::new(window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
