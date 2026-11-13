use alhazen_gpui::{PlayerConfig, PlayerEvent, PlayerState, Source, VideoPlayer, video_view};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc, time::Duration};

pub struct Playback {
    player: Entity<VideoPlayer>,
    warning: Option<String>,
    track: Rc<Cell<Bounds<Pixels>>>,
    _temporary: Option<tempfile::NamedTempFile>,
    _subscriptions: Vec<Subscription>,
}

pub fn source(url: &str) -> Option<Source> {
    if !crate::media::cdn_url(url) {
        return None;
    }
    match Source::parse(url).ok()? {
        source @ Source::Http(_) => Some(source),
        _ => None,
    }
}

pub fn video_type(content_type: Option<&str>, filename: &str) -> bool {
    if let Some(kind) = content_type {
        return kind.starts_with("video/");
    }
    let name = filename
        .split('?')
        .next()
        .unwrap_or(filename)
        .to_ascii_lowercase();
    [".mp4", ".webm", ".mov", ".mkv", ".m4v"]
        .iter()
        .any(|extension| name.ends_with(extension))
}

impl Playback {
    pub fn new(
        source: Source,
        temporary: Option<tempfile::NamedTempFile>,
        cx: &mut Context<Self>,
    ) -> Self {
        // one click opens one decoder. no media threads or downloads exist before that click.
        let config = PlayerConfig {
            autoplay: true,
            decoder_threads: 2,
            frame_queue_len: 2,
            packet_queue_len: 16,
            max_output_size: Some((1280, 720)),
            ..Default::default()
        };
        let player = cx.new(|cx| VideoPlayer::new(source, config, cx));
        let subscriptions = vec![
            cx.observe(&player, |_, _, cx| cx.notify()),
            cx.subscribe(&player, |this, _, event: &PlayerEvent, cx| {
                if let PlayerEvent::Warning(warning) = event {
                    this.warning = Some(warning.chars().take(240).collect());
                    cx.notify();
                }
            }),
        ];
        Self {
            player,
            warning: None,
            track: Rc::new(Cell::new(Bounds::default())),
            _temporary: temporary,
            _subscriptions: subscriptions,
        }
    }

    fn skip(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.player.update(cx, |player, cx| {
            if player.is_seekable() {
                let seconds = (player.position().as_secs_f64() + delta as f64).max(0.);
                let end = player
                    .duration()
                    .map_or(seconds, |duration| duration.as_secs_f64().min(seconds));
                player.seek(Duration::from_secs_f64(end), cx);
            }
        });
    }
}

fn control(id: &'static str, text: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(0x2b2d31))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x3b3d44)))
        .child(text)
}
fn time(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl Render for Playback {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = self.player.read(cx);
        let position = player.position();
        let duration = player.duration();
        let progress = duration.filter(|d| !d.is_zero()).map_or(0., |d| {
            (position.as_secs_f32() / d.as_secs_f32()).clamp(0., 1.)
        });
        let status = match player.state() {
            PlayerState::Loading => Some("Opening video…".to_string()),
            PlayerState::Buffering => Some("Buffering…".to_string()),
            PlayerState::Error(_) => Some("This video could not play. Its codec may be unavailable on this system, or the attachment link may have expired. You can still open the original file.".to_string()),
            _ => None,
        };
        let play_label = if player.is_playing() { "Pause" } else { "Play" };
        let mute_label = if player.is_muted() { "Unmute" } else { "Mute" };
        let volume = format!("{}%", (player.volume() * 100.).round() as u32);
        let track = self.track.clone();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .gap_2()
            .child(
                video_view(self.player.clone())
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .bg(rgb(0x000000)),
            )
            .when_some(status, |s, status| {
                s.child(
                    div()
                        .px_3()
                        .text_size(px(12.))
                        .text_color(rgb(0xc5c5cd))
                        .child(status),
                )
            })
            .when_some(self.warning.clone(), |s, warning| {
                s.child(
                    div()
                        .px_3()
                        .text_size(px(11.))
                        .text_color(rgb(0xe7c982))
                        .child(warning),
                )
            })
            .child(
                div()
                    .id("video-seek")
                    .relative()
                    .h(px(12.))
                    .mx_3()
                    .cursor_pointer()
                    .bg(rgb(0x3b3d44))
                    .child(div().h_full().w(relative(progress)).bg(rgb(0x5865f2)))
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                track.set(bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            let bounds = this.track.get();
                            if bounds.size.width <= px(0.) {
                                return;
                            }
                            let fraction = ((event.position.x - bounds.origin.x)
                                / bounds.size.width)
                                .clamp(0., 1.);
                            this.player.update(cx, |player, cx| {
                                if player.is_seekable()
                                    && let Some(duration) = player.duration()
                                {
                                    player.seek(duration.mul_f32(fraction), cx);
                                }
                            });
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .px_3()
                    .pb_3()
                    .text_size(px(12.))
                    .child(
                        control("video-back", "−10s")
                            .on_click(cx.listener(|this, _, _, cx| this.skip(-10, cx))),
                    )
                    .child(control("video-play", play_label).on_click(cx.listener(
                        |this, _, _, cx| this.player.update(cx, |player, cx| player.toggle(cx)),
                    )))
                    .child(
                        control("video-forward", "+10s")
                            .on_click(cx.listener(|this, _, _, cx| this.skip(10, cx))),
                    )
                    .child(control("video-mute", mute_label).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.player
                                .update(cx, |player, cx| player.set_muted(!player.is_muted(), cx))
                        },
                    )))
                    .child(control("video-quieter", "Vol −").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.player.update(cx, |player, cx| {
                                player.set_volume(player.volume() - 0.1, cx)
                            })
                        },
                    )))
                    .child(control("video-louder", "Vol +").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.player.update(cx, |player, cx| {
                                player.set_volume(player.volume() + 0.1, cx)
                            })
                        },
                    )))
                    .child(volume)
                    .child(div().flex_1())
                    .child(format!(
                        "{} / {}",
                        time(position),
                        duration.map(time).unwrap_or_else(|| "--:--".into())
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{source, video_type};
    use std::time::{Duration, Instant};
    #[test]
    fn videos_are_never_treated_as_images_and_sources_stay_public() {
        assert!(video_type(Some("video/mp4"), "clip.mp4"));
        assert!(video_type(None, "CLIP.WEBM?ex=123"));
        assert!(!video_type(Some("image/gif"), "clip.mp4"));
        assert!(source("https://cdn.discordapp.com/attachments/1/2/clip.mp4?ex=123").is_some());
        for url in [
            "file:///etc/passwd",
            "http://cdn.discordapp.com/clip.mp4",
            "https://discord.com/api/v10/users/@me",
            "https://cdn.discordapp.com.evil.test/a.mp4",
            "https://127.0.0.1/a.mp4",
        ] {
            assert!(source(url).is_none());
        }
    }
    #[test]
    fn native_video_decodes_audio_and_recovers_after_seeking() {
        use alhazen_gpui::{
            PlayerConfig, Source,
            alhazen_core::{
                Player,
                audio::{AudioOutputConfig, NullOutput},
            },
        };
        let audio = NullOutput::new(48000, 2);
        let player = Player::open(
            Source::File(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video.webm"),
            ),
            PlayerConfig {
                autoplay: true,
                decoder_threads: 2,
                frame_queue_len: 2,
                packet_queue_len: 16,
                audio_output: AudioOutputConfig::Null(audio.clone()),
                ..Default::default()
            },
        )
        .expect("sample opens without an external player or audio device");
        assert!(player.has_video() && player.has_audio() && player.is_seekable());
        let mut heard_audio = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            heard_audio |= audio.pull(480).iter().any(|sample| sample.abs() > 0.001);
            if heard_audio && player.current_frame().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(heard_audio, "decoded audio reaches the output sink");
        assert!(
            player.current_frame().is_some(),
            "decoded frames reach the renderer"
        );
        player.pause();
        player.seek(Duration::from_secs(2));
        player.play();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let _ = audio.pull(480);
            if player
                .current_frame()
                .is_some_and(|frame| frame.pts() >= Duration::from_millis(1900))
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("video did not resume at the requested time");
    }
}
