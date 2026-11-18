use super::*;
use crate::attachments::{DraftFile, MAX_FILES, Source};
use std::path::PathBuf;

impl Larpcord {
    pub(super) fn attachment_channel(&self) -> Option<String> {
        if self.home_page
            || self.settings_tab.is_some()
            || self.login_open
            || self.profile_is_modal()
            || self.component_menu.is_some()
            || self.video.is_some()
        {
            return None;
        }
        self.selected_channel.clone()
    }

    pub(super) fn choose_attachments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(channel) = self.attachment_channel() else {
            return;
        };
        let generation = self.generation;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach files".into()),
        });
        cx.spawn_in(window, async move |view, cx| match picker.await {
            Ok(Ok(Some(paths))) => {
                let _ = view.update_in(cx, |this, _, cx| {
                    if this.generation == generation {
                        this.prepare_attachments(channel, paths, cx);
                    }
                });
            }
            Ok(Ok(None)) => {}
            _ => {
                let _ = view.update_in(cx, |this, _, cx| {
                    if this.generation == generation {
                        this.error = Some("Could not open the attachment picker.".into());
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    pub(super) fn prepare_attachments(
        &mut self,
        channel: String,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let generation = self.generation;
        let result = cx.background_executor().spawn(async move {
            paths
                .into_iter()
                .take(MAX_FILES + 1)
                .map(DraftFile::from_path)
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |view, cx| {
            let results = result.await;
            let _ = view.update(cx, |this, cx| {
                if this.generation == generation && this.store.channels.contains_key(&channel) {
                    for result in results {
                        match result {
                            Ok(file) => this.queue_attachment(&channel, file, cx),
                            Err(error) => this.error = Some(error),
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn queue_attachment(
        &mut self,
        channel: &str,
        file: DraftFile,
        cx: &mut Context<Self>,
    ) {
        if self.upload_channel.as_deref() == Some(channel) {
            self.error = Some("Wait for this upload to finish before adding more files.".into());
        } else {
            let drafts = self
                .attachment_drafts
                .entry(channel.to_string())
                .or_default();
            if drafts.len() >= MAX_FILES {
                self.error = Some("You can attach up to 10 files to a message.".into());
            } else if !drafts.iter().any(|existing| {
                matches!((&existing.source, &file.source), (Source::Path { path: a, .. }, Source::Path { path: b, .. }) if a == b)
            }) {
                drafts.push(file);
            }
        }
        cx.notify();
    }

    pub(super) fn attachment_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(channel) = self.selected_channel.as_ref() else {
            return div().into_any_element();
        };
        let Some(files) = self.attachment_drafts.get(channel) else {
            return div().into_any_element();
        };
        let uploading = self.upload_channel.as_ref() == Some(channel);
        column()
            .gap_1()
            .when(uploading, |s| s.child(label("Uploading attachments…")))
            .child(
                row()
                    .gap_2()
                    .flex_wrap()
                    .children(files.iter().enumerate().map(|(index, file)| {
                        let channel = channel.clone();
                        let size = if file.size >= 1_048_576 {
                            format!("{:.1} MB", file.size as f64 / 1_048_576.)
                        } else if file.size >= 1024 {
                            format!("{:.1} KB", file.size as f64 / 1024.)
                        } else {
                            format!("{} B", file.size)
                        };
                        row()
                            .id(("attachment-draft", index))
                            .gap_2()
                            .min_w_0()
                            .max_w(px(240.))
                            .py_1()
                            .px_2()
                            .rounded_md()
                            .bg(self.color(SURFACE))
                            .child(icon("file", 17.))
                            .child(
                                column()
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .truncate()
                                            .child(file.name.clone()),
                                    )
                                    .child(label(size).text_size(px(10.))),
                            )
                            .when(!uploading, |s| {
                                s.child(
                                    toolbar_button(
                                        "remove-attachment",
                                        "close",
                                        "Remove attachment",
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if let Some(files) =
                                                this.attachment_drafts.get_mut(&channel)
                                                && index < files.len()
                                            {
                                                files.remove(index);
                                                if files.is_empty() {
                                                    this.attachment_drafts.remove(&channel);
                                                }
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                            })
                    })),
            )
            .into_any_element()
    }
}
