use super::*;
use std::time::Duration;

const EXIT_DURATION: Duration = Duration::from_millis(125);

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Panel {
    Profile,
    Settings,
    Picker,
}

pub(super) struct Exit {
    panel: Panel,
    _task: Task<()>,
}

impl Larpcord {
    pub(super) fn cancel_panel_exit(&mut self, panel: Panel) {
        if self
            .panel_exit
            .as_ref()
            .is_some_and(|exit| exit.panel == panel)
        {
            self.panel_exit = None;
        }
    }
    pub(super) fn animate_panel_close(
        &mut self,
        panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.motion() {
            self.finish_panel_close(panel, window, cx);
            return;
        }
        if self
            .panel_exit
            .as_ref()
            .is_some_and(|exit| exit.panel == panel)
        {
            return;
        }
        self.panel_exit = Some(Exit {
            panel,
            _task: cx.spawn_in(window, async move |view, cx| {
                cx.background_executor().timer(EXIT_DURATION).await;
                let _ = view.update_in(cx, |this, window, cx| {
                    this.finish_panel_close(panel, window, cx)
                });
            }),
        });
        cx.notify();
    }

    fn finish_panel_close(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        self.panel_exit = None;
        match panel {
            Panel::Profile => self.dismiss_profile(window, cx),
            Panel::Settings => self.close_settings(window, cx),
            Panel::Picker => {
                self.close_picker();
                window.focus(&self.composer.focus_handle(cx));
                cx.notify();
            }
        }
    }

    pub(super) fn panel_transition<E: IntoElement + Styled + 'static>(
        &self,
        element: E,
        panel: Panel,
        enter: &'static str,
    ) -> AnyElement {
        if self.motion()
            && self
                .panel_exit
                .as_ref()
                .is_some_and(|exit| exit.panel == panel)
        {
            let exit = match panel {
                Panel::Profile => "profile-exit",
                Panel::Settings => "settings-exit",
                Panel::Picker => "picker-exit",
            };
            element
                .with_animation(exit, Animation::new(EXIT_DURATION), |s, t| {
                    s.opacity(1. - t)
                })
                .into_any_element()
        } else {
            fade_in(element, enter, self.motion())
        }
    }
}
