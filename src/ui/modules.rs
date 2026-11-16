use super::*;
impl Larpcord {
    pub(super) fn modules_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = column()
            .gap_4()
            .child(div().text_size(px(26.)).child("Modules"))
            .child(label(
                "Small native extensions. Enable each module after reviewing its permissions.",
            ))
            .child(
                row()
                    .gap_2()
                    .child(
                        button("reload-modules", "Reload modules")
                            .on_click(cx.listener(|this, _, _, cx| this.reload_modules(cx))),
                    )
                    .child(button("open-modules", "Open folder").on_click(cx.listener(
                        |this, _, _, cx| {
                            if this.preview_only {
                                return;
                            }
                            if let Some(path) = crate::modules::directory() {
                                match std::fs::create_dir_all(&path) {
                                    Ok(()) => cx.reveal_path(&path),
                                    Err(_) => {
                                        this.error =
                                            Some("Could not open the modules folder".into());
                                        cx.notify();
                                    }
                                }
                            }
                        },
                    ))),
            )
            .child(label(if self.modules_loading {
                "Reloading…"
            } else {
                "API 1 · local declarative hooks · no background polling"
            }));
        for module in &self.modules.modules {
            let id = module.id().to_string();
            let enabled = module.enabled;
            let permissions = module
                .permissions()
                .iter()
                .map(|p| p.description())
                .collect::<Vec<_>>()
                .join(" · ");
            body = body.child(
                column()
                    .p_4()
                    .gap_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(self.color(LINE))
                    .child(
                        row()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(module.name().to_string()),
                            )
                            .child(label(format!("v{}", module.version()))),
                    )
                    .child(label(module.description().to_string()))
                    .child(label(format!("Permissions: {permissions}")).text_size(px(11.)))
                    .child(
                        button(
                            SharedString::from(format!("enable-module-{id}")),
                            if enabled {
                                "Disable"
                            } else {
                                "Enable with these permissions"
                            },
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.modules_loading {
                                this.modules.set_enabled(
                                    &id,
                                    !enabled,
                                    &mut this.settings.module_grants,
                                );
                                this.refresh_messages();
                                this.persist(cx);
                            }
                        })),
                    ),
            );
        }
        for error in self.modules.errors.iter().take(32) {
            body = body.child(label(error.clone()).text_color(rgb(0xf0b7c3)));
        }
        body.child(label("Updated manifests require enabling again. These modules can label messages and set theme colors. WASM execution and existing JavaScript plugin compatibility are not part of API 1."))
            .into_any_element()
    }
    fn reload_modules(&mut self, cx: &mut Context<Self>) {
        if self.modules_loading {
            return;
        }
        self.modules_loading = true;
        let root = if self.preview_only {
            None
        } else {
            crate::modules::directory()
        };
        let grants = self.settings.module_grants.clone();
        self.modules_task = Some(cx.spawn(async move |view, cx| {
            let registry = cx
                .background_executor()
                .spawn(async move { crate::modules::Registry::load(root.as_deref(), &grants) })
                .await;
            let _ = view.update(cx, |this, cx| {
                this.modules = registry;
                this.modules_loading = false;
                this.modules_task = None;
                this.refresh_messages();
                cx.notify();
            });
        }));
        cx.notify();
    }
}
