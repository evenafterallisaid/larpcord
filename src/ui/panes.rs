use super::*;

const RAIL: f32 = 72.;
const DIVIDER: f32 = 6.;
const MIN_CHAT: f32 = 300.;
const MIN_CHANNELS: f32 = 160.;
const MIN_MEMBERS: f32 = 150.;
const MAX_PANE: f32 = 540.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pane {
    Channels,
    Members,
}

#[derive(Clone, Copy)]
pub(super) struct PaneWidths {
    pub channels: f32,
    pub members: f32,
    pub inspector: f32,
}

impl PaneWidths {
    pub fn sidebar(self) -> Pixels {
        px(RAIL + self.channels)
    }

    fn side_budget(self, viewport: f32) -> f32 {
        viewport
            - RAIL
            - self.inspector
            - MIN_CHAT
            - DIVIDER * if self.members > 0. { 2. } else { 1. }
    }
}

struct ResizePreview;

impl Render for ResizePreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(0.))
    }
}

impl Larpcord {
    pub(super) fn pane_widths(&self, viewport: Pixels) -> PaneWidths {
        let inspector = if self.profile_inspector && self.profile.is_some() {
            296.
        } else {
            0.
        };
        let members_visible = !self.home_page
            && self.members_open
            && (self.selected_guild.is_some()
                || self
                    .selected_channel
                    .as_ref()
                    .and_then(|id| self.store.channels.get(id))
                    .is_some_and(|c| c.kind == 3));
        let mut result = PaneWidths {
            channels: f32::from(self.settings.channel_pane_width).clamp(MIN_CHANNELS, MAX_PANE),
            members: if members_visible {
                f32::from(self.settings.member_pane_width).clamp(MIN_MEMBERS, MAX_PANE)
            } else {
                0.
            },
            inspector,
        };
        let available = result.side_budget(f32::from(viewport));
        let min_members = if members_visible { MIN_MEMBERS } else { 0. };
        result.channels = result
            .channels
            .min((available - min_members).max(MIN_CHANNELS));
        result.members = result
            .members
            .min((available - result.channels).max(min_members));
        result
    }

    pub(super) fn pane_divider(&self, pane: Pane, cx: &mut Context<Self>) -> AnyElement {
        let id = match pane {
            Pane::Channels => "channel-pane-divider",
            Pane::Members => "member-pane-divider",
        };
        row()
            .id(id)
            .debug_selector(move || id.to_string())
            .w(px(DIVIDER))
            .h_full()
            .flex_shrink_0()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|s| s.bg(self.color(ACCENT)))
            .on_drag(pane, |_, _, _, cx| cx.new(|_| ResizePreview))
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<Pane>, window, cx| {
                    if pane != *event.drag(cx) {
                        return;
                    }
                    let viewport = f32::from(window.viewport_size().width);
                    let pointer = f32::from(event.event.position.x);
                    let widths = this.pane_widths(px(viewport));
                    let budget = widths.side_budget(viewport);
                    let (value, requested, min, other) = match pane {
                        Pane::Channels => (
                            &mut this.settings.channel_pane_width,
                            pointer - RAIL - DIVIDER / 2.,
                            MIN_CHANNELS,
                            widths.members,
                        ),
                        Pane::Members => (
                            &mut this.settings.member_pane_width,
                            viewport - widths.inspector - pointer - DIVIDER / 2.,
                            MIN_MEMBERS,
                            widths.channels,
                        ),
                    };
                    let width = requested
                        .clamp(min, (budget - other).clamp(min, MAX_PANE))
                        .round() as u16;
                    if *value != width {
                        *value = width;
                        this.pane_resize_dirty = true;
                        cx.notify();
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn save_pane_resize(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.pane_resize_dirty) {
            self.persist(cx);
        }
    }
}
