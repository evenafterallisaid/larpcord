use super::*;
use crate::modules::{ComposerTool, MAX_TOOL_DRAFT};

pub(super) struct Outcome {
    tool: ComposerTool,
    context: DraftContext,
    result: Result<String, &'static str>,
}

struct DraftContext {
    generation: u64,
    channel: String,
    text: String,
}

impl Larpcord {
    pub(super) fn close_composer_tools(&mut self) {
        self.composer_tools_open = false;
        self.composer_tool_result = None;
    }

    pub(super) fn composer_tools_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        toolbar_button("composer-tools", "sparkles", "Composer tools")
            .when(self.composer_tools_open, |s| s.bg(self.color(SIDEBAR)))
            .on_click(cx.listener(|this, _, window, cx| {
                if this.composer_tools_open {
                    this.close_composer_tools();
                } else {
                    this.close_picker();
                    this.composer_tools_open = true;
                }
                window.focus(&this.composer.focus_handle(cx));
                cx.notify();
            }))
    }

    pub(super) fn composer_tools(&self, cx: &mut Context<Self>) -> AnyElement {
        let text = &self.composer.read(cx).content;
        let ready =
            self.composer_tool_task.is_none() && !text.is_empty() && text.len() <= MAX_TOOL_DRAFT;
        let mut panel = column()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(self.color(LINE))
            .bg(self.color(SIDEBAR))
            .child(
                row().gap_2().child(icon("sparkles", 16.)).child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Composer tools"),
                ),
            )
            .child(
                row().gap_2().flex_wrap().children(
                    self.modules
                        .modules
                        .iter()
                        .filter(|m| m.enabled && m.is_composer_tool())
                        .map(|module| {
                            let id = module.id().to_string();
                            row()
                                .id(SharedString::from(format!("run-tool-{id}")))
                                .debug_selector({
                                    let id = id.clone();
                                    move || format!("run-tool-{id}")
                                })
                                .rounded_md()
                                .px_2()
                                .py_1()
                                .bg(self.color(SURFACE))
                                .text_size(px(12.))
                                .child(module.name().to_string())
                                .when(ready, |s| {
                                    s.cursor_pointer().on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.run_composer_tool(&id, cx);
                                        },
                                    ))
                                })
                                .when(!ready, |s| s.opacity(0.5))
                        }),
                ),
            );
        if let Some(outcome) = &self.composer_tool_result {
            panel = match &outcome.result {
                Ok(output) if output == &outcome.context.text => panel.child(label("No changes.")),
                Ok(output) => panel.child(
                    column()
                        .gap_2()
                        .child(label(format!("Preview from {}", outcome.tool.name)))
                        .child(
                            div()
                                .id("tool-output")
                                .max_h(px(106.))
                                .overflow_y_scroll()
                                .rounded_md()
                                .bg(self.color(PANEL))
                                .px_3()
                                .py_2()
                                .text_size(px(13.))
                                .child(if output.is_empty() {
                                    "Empty draft".to_string()
                                } else {
                                    output.clone()
                                }),
                        )
                        .child(
                            row()
                                .gap_2()
                                .child(
                                    button("apply-tool-output", "Apply to draft")
                                        .debug_selector(|| "apply-tool-output".into())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.apply_composer_tool(window, cx)
                                        })),
                                )
                                .child(button("discard-tool-output", "Discard").on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.composer_tool_result = None;
                                        cx.notify();
                                    }),
                                )),
                        ),
                ),
                Err(error) => panel.child(label(*error).text_color(rgb(0xf0b7c3))),
            };
        } else if !ready {
            panel = panel.child(label(if self.composer_tool_task.is_some() {
                "Running tool…"
            } else if text.is_empty() {
                "Type a draft to use a tool."
            } else {
                "Composer tools accept drafts up to 16 KiB."
            }));
        }
        panel.into_any_element()
    }

    fn run_composer_tool(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.composer_tools_open || self.composer_tool_task.is_some() || self.modules_loading {
            return;
        }
        let Some(channel) = self.selected_channel.clone() else {
            return;
        };
        let text = self.composer.read(cx).content.to_string();
        if text.is_empty() || text.len() > MAX_TOOL_DRAFT {
            return;
        }
        let Some(tool) = self
            .modules
            .modules
            .iter()
            .find(|m| m.id() == id)
            .and_then(|m| m.composer_tool())
        else {
            return;
        };
        let context = DraftContext {
            generation: self.generation,
            channel,
            text,
        };
        self.composer_tool_result = None;
        let pending = cx.background_executor().spawn(async move {
            let result = tool.transform(&context.text);
            Outcome {
                tool,
                context,
                result,
            }
        });
        self.composer_tool_task = Some(cx.spawn(async move |view, cx| {
            let outcome = pending.await;
            let _ = view.update(cx, |this, cx| {
                this.composer_tool_task = None;
                if this.composer_tools_open && this.tool_result_is_current(&outcome, cx) {
                    this.composer_tool_result = Some(outcome);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn tool_result_is_current(&self, outcome: &Outcome, cx: &App) -> bool {
        self.generation == outcome.context.generation
            && !self.home_page
            && self.selected_channel.as_deref() == Some(outcome.context.channel.as_str())
            && self.composer.read(cx).content.as_ref() == outcome.context.text
            && self
                .modules
                .modules
                .iter()
                .any(|m| m.enabled && m.id() == outcome.tool.id && m.digest == outcome.tool.digest)
    }

    fn apply_composer_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(outcome) = self.composer_tool_result.take()
            && self.tool_result_is_current(&outcome, cx)
            && let Ok(text) = outcome.result
        {
            self.composer.update(cx, |input, cx| input.set(&text, cx));
        }
        self.close_composer_tools();
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }
}
