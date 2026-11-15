use gpui::{prelude::*, *};

pub fn icon(name: &str, size: f32) -> Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(size))
        .text_color(rgb(0xdcddde))
        .flex_shrink_0()
}

pub static ASSETS: &[(&str, &[u8])] = &[
    ("icons/grid.svg", include_bytes!("../assets/icons/grid.svg")),
    (
        "icons/clock.svg",
        include_bytes!("../assets/icons/clock.svg"),
    ),
    ("icons/gif.svg", include_bytes!("../assets/icons/gif.svg")),
    (
        "icons/sticker.svg",
        include_bytes!("../assets/icons/sticker.svg"),
    ),
    (
        "icons/refresh.svg",
        include_bytes!("../assets/icons/refresh.svg"),
    ),
    (
        "icons/message.svg",
        include_bytes!("../assets/icons/message.svg"),
    ),
    ("icons/copy.svg", include_bytes!("../assets/icons/copy.svg")),
    (
        "icons/announcement.svg",
        include_bytes!("../assets/icons/announcement.svg"),
    ),
    (
        "icons/thread.svg",
        include_bytes!("../assets/icons/thread.svg"),
    ),
    (
        "icons/arrow-left.svg",
        include_bytes!("../assets/icons/arrow-left.svg"),
    ),
    (
        "icons/arrow-right.svg",
        include_bytes!("../assets/icons/arrow-right.svg"),
    ),
    (
        "icons/sparkles.svg",
        include_bytes!("../assets/icons/sparkles.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_bytes!("../assets/icons/chevron-right.svg"),
    ),
    ("icons/copy.svg", include_bytes!("../assets/icons/copy.svg")),
    (
        "icons/reply.svg",
        include_bytes!("../assets/icons/reply.svg"),
    ),
    (
        "icons/smile.svg",
        include_bytes!("../assets/icons/smile.svg"),
    ),
    (
        "icons/users.svg",
        include_bytes!("../assets/icons/users.svg"),
    ),
    ("icons/user.svg", include_bytes!("../assets/icons/user.svg")),
    (
        "icons/close.svg",
        include_bytes!("../assets/icons/close.svg"),
    ),
    (
        "icons/minus.svg",
        include_bytes!("../assets/icons/minus.svg"),
    ),
    (
        "icons/maximize.svg",
        include_bytes!("../assets/icons/maximize.svg"),
    ),
    (
        "icons/chevron-down.svg",
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
    ("icons/hash.svg", include_bytes!("../assets/icons/hash.svg")),
    (
        "icons/volume.svg",
        include_bytes!("../assets/icons/volume.svg"),
    ),
    (
        "icons/volume-off.svg",
        include_bytes!("../assets/icons/volume-off.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../assets/icons/search.svg"),
    ),
    ("icons/send.svg", include_bytes!("../assets/icons/send.svg")),
    (
        "icons/external-link.svg",
        include_bytes!("../assets/icons/external-link.svg"),
    ),
    (
        "icons/arrow-down.svg",
        include_bytes!("../assets/icons/arrow-down.svg"),
    ),
    ("icons/play.svg", include_bytes!("../assets/icons/play.svg")),
    (
        "icons/pause.svg",
        include_bytes!("../assets/icons/pause.svg"),
    ),
    (
        "icons/boost.svg",
        include_bytes!("../assets/icons/boost.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../assets/icons/settings.svg"),
    ),
];

pub fn channel_icon(kind: u8) -> &'static str {
    match kind {
        2 | 13 => "volume",
        5 => "announcement",
        10..=12 => "thread",
        _ => "hash",
    }
}
