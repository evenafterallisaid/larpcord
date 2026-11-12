#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod auth;
mod input;
mod model;
mod network;
mod settings;
mod ui;

use gpui::{prelude::*, *};
use std::borrow::Cow;
use ui::{ClosePanel, FocusSearch, Larpcord, OpenSettings, Quit};

struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "mascot.png" => Some(Cow::Borrowed(include_bytes!("../assets/mascot.png"))),
            _ => None,
        })
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec!["mascot.png".into()])
    }
}

fn main() {
    let app = Application::new().with_assets(Assets);
    app.on_reopen(|cx| {
        for window in cx.windows() {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        cx.activate(true);
    });
    app.run(|cx: &mut App| {
        input::bind_keys(cx);
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.bind_keys([
            KeyBinding::new(&format!("{modifier}-q"), Quit, None),
            KeyBinding::new(&format!("{modifier}-,"), OpenSettings, None),
            KeyBinding::new(&format!("{modifier}-k"), FocusSearch, None),
            KeyBinding::new("escape", ClosePanel, None),
        ]);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.), px(730.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Larpcord".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(13.), px(13.))),
                }),
                window_min_size: Some(size(px(800.), px(580.))),
                app_id: Some("app.larpcord.client".into()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Larpcord::new(window, cx)),
        )
        .expect("open native window");
        cx.activate(true);
    });
}
