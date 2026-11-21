use super::*;
use std::time::Duration;

const ENTER_DURATION: Duration = Duration::from_millis(180);
const EXIT_DURATION: Duration = Duration::from_millis(125);

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) enum Panel {
    Profile,
    ProfileDetails,
    Settings,
    Picker,
    Search,
    Members,
}

impl Larpcord {
    pub(super) fn cancel_panel_exit(&mut self, panel: Panel) {
        self.panel_exits.remove(&panel);
    }

    pub(super) fn panel_closing(&self, panel: Panel) -> bool {
        self.panel_exits.contains_key(&panel)
    }

    pub(super) fn animate_panel_close(
        &mut self,
        panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if panel == Panel::Profile {
            self.cancel_panel_exit(Panel::ProfileDetails);
        }
        if !self.motion() {
            self.finish_panel_close(panel, window, cx);
            return;
        }
        if self.panel_closing(panel) {
            return;
        }
        self.panel_exits.insert(
            panel,
            cx.spawn_in(window, async move |view, cx| {
                cx.background_executor().timer(EXIT_DURATION).await;
                let _ = view.update_in(cx, |this, window, cx| {
                    this.finish_panel_close(panel, window, cx)
                });
            }),
        );
        cx.notify();
    }

    fn finish_panel_close(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_panel_exit(panel);
        match panel {
            Panel::Profile => self.dismiss_profile(window, cx),
            Panel::ProfileDetails => self.collapse_profile(cx),
            Panel::Settings => self.close_settings(window, cx),
            Panel::Picker => {
                self.close_picker();
                window.focus(&self.composer.focus_handle(cx));
                cx.notify();
            }
            Panel::Search => self.close_conversation_search(window, cx),
            Panel::Members => {
                self.members_open = false;
                self.subscribe_members();
                cx.notify();
            }
        }
    }

    pub(super) fn panel_transition<E: IntoElement + 'static>(
        &self,
        element: E,
        panel: Panel,
        enter: &'static str,
        effect: impl Fn(E, f32) -> E + 'static,
    ) -> AnyElement {
        if !self.motion() {
            return element.into_any_element();
        }
        if self.panel_closing(panel) {
            element
                .with_animation(
                    ElementId::NamedInteger(enter.into(), 1),
                    Animation::new(EXIT_DURATION).with_easing(ease_in_out),
                    move |s, t| effect(s, 1. - t),
                )
                .into_any_element()
        } else {
            element
                .with_animation(
                    enter,
                    Animation::new(ENTER_DURATION).with_easing(ease_out_quint()),
                    effect,
                )
                .into_any_element()
        }
    }
}
