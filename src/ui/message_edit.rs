use super::*;

pub(super) struct Editor {
    channel: String,
    message: String,
    original: String,
    saving: Option<u64>,
    error: Option<String>,
}

impl Larpcord {
    pub(super) fn can_edit_message(&self, message: &Message) -> bool {
        !self.preview
            && self.status == "Connected"
            && message.author.id == self.store.user.id
            && self.selected_channel.as_ref() == Some(&message.channel_id)
            && message.webhook_id.is_none()
            && !message.content.is_empty()
            && matches!(message.kind, 0 | 19)
    }

    pub(super) fn open_message_edit(
        &mut self,
        message: &Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_edit_message(message) || self.message_edit.is_some() {
            return;
        }
        self.close_picker();
        self.close_composer_tools();
        self.message_edit = Some(Editor {
            channel: message.channel_id.clone(),
            message: message.id.clone(),
            original: message.content.clone(),
            saving: None,
            error: None,
        });
        self.edit_input
            .update(cx, |input, cx| input.set(&message.content, cx));
        window.focus(&self.edit_input.focus_handle(cx));
        cx.notify();
    }

    pub(super) fn submit_message_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.message_edit else {
            return;
        };
        if editor.saving.is_some() {
            return;
        }
        let content = self.edit_input.read(cx).content.trim().to_string();
        if content == editor.original {
            self.close_message_edit(window, cx);
            return;
        }
        if content.is_empty() || content.chars().count() > 2000 {
            editor.error = Some("Enter between 1 and 2,000 characters.".into());
        } else if self.status != "Connected" {
            editor.error = Some("Reconnect before saving this edit.".into());
        } else {
            self.edit_sequence = self.edit_sequence.wrapping_add(1);
            let request = self.edit_sequence;
            match self.backend.command(Command::EditMessage {
                channel: editor.channel.clone(),
                message: editor.message.clone(),
                author: self.store.user.id.clone(),
                content,
                request,
            }) {
                Ok(()) => {
                    editor.saving = Some(request);
                    editor.error = None;
                    window.focus(&self.root_focus);
                }
                Err(error) => editor.error = Some(error),
            }
        }
        cx.notify();
    }

    pub(super) fn edit_last_message(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.composer.read(cx).content.is_empty() || self.message_edit.is_some() {
            return false;
        }
        let Some(channel) = &self.selected_channel else {
            return false;
        };
        let Some(message) = self
            .store
            .channel_messages(channel)
            .iter()
            .rev()
            .find(|message| self.can_edit_message(message))
            .cloned()
        else {
            return false;
        };
        self.open_message_edit(&message, window, cx);
        true
    }

    pub(super) fn reset_message_edit(&mut self, cx: &mut Context<Self>) {
        if self.message_edit.take().is_some() {
            self.edit_input.update(cx, |input, cx| input.set("", cx));
        }
    }

    pub(super) fn close_message_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .message_edit
            .as_ref()
            .is_some_and(|editor| editor.saving.is_some())
        {
            return;
        }
        self.reset_message_edit(cx);
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }

    pub(super) fn receive_message_edit(
        &mut self,
        request: u64,
        result: Result<Value, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Ok(reply) = &result
            && let (Some(channel), Some(id)) = (reply["channel_id"].as_str(), reply["id"].as_str())
            && let Some(message) = self
                .store
                .channel_messages(channel)
                .iter()
                .find(|m| m.id == id)
        {
            let timestamp = |value: &str| chrono::DateTime::parse_from_rfc3339(value).ok();
            let incoming = reply["edited_timestamp"].as_str().and_then(timestamp);
            if incoming.is_some()
                && incoming >= message.edited_timestamp.as_deref().and_then(timestamp)
            {
                self.store.update(&serde_json::json!({
                    "channel_id": channel, "id": id, "content": reply["content"],
                    "edited_timestamp": reply["edited_timestamp"],
                }));
                if self.selected_channel.as_deref() == Some(channel) {
                    self.refresh_messages();
                }
            }
        }
        if let Some(editor) = self
            .message_edit
            .as_mut()
            .filter(|e| e.saving == Some(request))
        {
            editor.saving = None;
            match result {
                Ok(_) => self.close_message_edit(window, cx),
                Err(error) => {
                    editor.error = Some(error);
                    window.focus(&self.edit_input.focus_handle(cx));
                }
            }
        }
    }

    pub(super) fn message_edit_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self
            .message_edit
            .as_ref()
            .expect("editing panel has an editor");
        let saving = editor.saving.is_some();
        column()
            .id("edit-message-panel")
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(self.color(LINE))
            .bg(self.color(PANEL))
            .child(label("Editing your message"))
            .child(
                div()
                    .w_full()
                    .when(saving, |s| s.opacity(0.65))
                    .child(self.edit_input.clone()),
            )
            .when_some(editor.error.clone(), |s, error| s.child(label(error)))
            .child(
                row()
                    .gap_2()
                    .child(
                        button(
                            "save-message-edit",
                            if saving { "Saving…" } else { "Save changes" },
                        )
                        .when(!saving, |s| {
                            s.on_click(cx.listener(|this, _, window, cx| {
                                this.submit_message_edit(window, cx)
                            }))
                        }),
                    )
                    .when(!saving, |s| {
                        s.child(button("cancel-message-edit", "Cancel").on_click(
                            cx.listener(|this, _, window, cx| this.close_message_edit(window, cx)),
                        ))
                    }),
            )
    }
}
