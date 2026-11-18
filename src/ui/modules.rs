use super::*;
use crate::modules::{Catalog, Module};

pub(super) const THEMES_TAB: usize = 5;

impl Larpcord {
    pub(super) fn modules_settings(&self, themes: bool, cx: &mut Context<Self>) -> AnyElement {
        let mut body = column()
            .gap_4()
            .child(
                div()
                    .text_size(px(26.))
                    .child(if themes { "Themes" } else { "Modules" }),
            );
        body = body.child(
            row()
                .gap_2()
                .flex_wrap()
                .children(
                    [
                        ("discover", "Discover", true),
                        ("installed", "Installed", false),
                    ]
                    .into_iter()
                    .map(|(id, text, discover)| {
                        button(id, text)
                            .bg(self.color(if self.catalog_discover == discover {
                                SURFACE
                            } else {
                                PANEL
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.catalog_discover = discover;
                                cx.notify();
                            }))
                    }),
                )
                .child(if self.catalog_discover {
                    button(
                        "refresh-catalog",
                        if self.catalog_task.is_some() {
                            "Refreshing…"
                        } else {
                            "Refresh catalog"
                        },
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_catalog(cx)))
                } else {
                    button(
                        "reload-modules",
                        if self.modules_loading {
                            "Reloading…"
                        } else {
                            "Reload local files"
                        },
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.reload_modules(cx)))
                })
                .when(!self.catalog_discover && !self.preview_only, |s| {
                    s.child(button("open-modules", "Open folder").on_click(cx.listener(
                        |this, _, _, cx| {
                            let Some(path) = crate::modules::directory() else {
                                return;
                            };
                            match std::fs::create_dir_all(&path) {
                                Ok(()) => cx.reveal_path(&path),
                                Err(_) => {
                                    this.catalog_notice =
                                        Some(("Could not open the modules folder.".into(), true))
                                }
                            }
                            cx.notify();
                        },
                    )))
                }),
        );
        body = body.child(
            div()
                .px_3()
                .rounded_lg()
                .bg(self.color(PANEL))
                .child(self.catalog_search.clone()),
        );
        if let Some((notice, error)) = &self.catalog_notice {
            body = body.child(label(notice.clone()).when(*error, |e| e.text_color(rgb(0xf0b7c3))));
        }
        let query = self.catalog_search.read(cx).content.trim().to_lowercase();
        let matches = |module: &Module| {
            module.is_theme() == themes
                && (query.is_empty()
                    || module.name().to_lowercase().contains(&query)
                    || module.description().to_lowercase().contains(&query))
        };
        let mut found = false;
        if self.catalog_discover {
            for entry in &self.catalog.entries {
                let Ok(module) = entry.module() else {
                    continue;
                };
                if !matches(&module) {
                    continue;
                }
                found = true;
                let installed = self.modules.modules.iter().find(|m| m.id() == module.id());
                let action = match installed {
                    None => "Install",
                    Some(m) if !m.from_catalog => "Local copy installed",
                    Some(m) if m.digest == module.digest => "Installed",
                    Some(_) => "Update",
                };
                let id = module.id().to_string();
                let entry = entry.clone();
                let install = action == "Install" || action == "Update";
                body = body.child(
                    self.module_card(&module).child(
                        row()
                            .gap_2()
                            .child(
                                button(SharedString::from(format!("install-{id}")), action).when(
                                    install,
                                    |s| {
                                        s.on_click(cx.listener(move |this, _, _, cx| {
                                            this.install_catalog(&entry, cx)
                                        }))
                                    },
                                ),
                            )
                            .when(!install && installed.is_some(), |s| {
                                s.child(
                                    button(SharedString::from(format!("manage-{id}")), "Manage")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.catalog_discover = false;
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
                );
            }
        } else {
            if themes && query.is_empty() {
                found = true;
                let has_override = self
                    .modules
                    .modules
                    .iter()
                    .any(|m| m.enabled && m.is_theme());
                body =
                    body.child(column().gap_2().child(label("Built-in themes")).child(
                        row().gap_2().flex_wrap().children(
                            crate::theme::Theme::ALL.into_iter().map(|(theme, name)| {
                                let tokens = theme.tokens(self.settings.custom_theme);
                                button(SharedString::from(format!("theme-{name}")), name)
                                    .bg(rgb(tokens.surface))
                                    .text_color(rgb(tokens.text))
                                    .border_1()
                                    .border_color(rgb(
                                        if self.settings.theme == theme && !has_override {
                                            tokens.accent
                                        } else {
                                            tokens.border
                                        },
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.apply_builtin_theme(theme, cx)
                                    }))
                            }),
                        ),
                    ));
            }
            for module in self.modules.modules.iter().filter(|m| matches(m)) {
                found = true;
                let enabled = module.enabled;
                let id = module.id().to_string();
                let remove_id = id.clone();
                body = body.child(
                    self.module_card(module).child(
                        row()
                            .gap_2()
                            .child(
                                button(
                                    SharedString::from(format!("enable-module-{id}")),
                                    match (themes, enabled) {
                                        (true, true) => "Reset to built-in",
                                        (true, false) => "Apply theme",
                                        (false, true) => "Disable",
                                        (false, false) => "Enable with these permissions",
                                    },
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if this.modules_loading {
                                            return;
                                        }
                                        if themes && !enabled {
                                            this.disable_theme_modules();
                                        }
                                        this.modules.set_enabled(
                                            &id,
                                            !enabled,
                                            &mut this.settings.module_grants,
                                        );
                                        this.refresh_messages();
                                        this.persist(cx);
                                    },
                                )),
                            )
                            .when(module.from_catalog, |s| {
                                s.child(
                                    button(
                                        SharedString::from(format!("remove-{remove_id}")),
                                        "Uninstall",
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.uninstall_catalog(&remove_id, cx)
                                        },
                                    )),
                                )
                            }),
                    ),
                );
            }
            for error in self.modules.errors.iter().take(32) {
                body = body.child(label(error.clone()).text_color(rgb(0xf0b7c3)));
            }
        }
        body.when(!found, |s| {
            s.child(label(if query.is_empty() {
                "Nothing here yet."
            } else {
                "No matches."
            }))
        })
        .into_any_element()
    }

    fn module_card(&self, module: &Module) -> Div {
        let permissions = module
            .permissions()
            .iter()
            .map(|p| p.description())
            .collect::<Vec<_>>()
            .join(" · ");
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
            .when(!permissions.is_empty() && !module.is_theme(), |s| {
                s.child(label(permissions).text_size(px(11.)))
            })
            .when(module.is_theme(), |s| {
                let tokens =
                    module.preview_theme(self.settings.theme.tokens(self.settings.custom_theme));
                s.child(
                    row().gap_1().children(
                        [
                            tokens.background,
                            tokens.surface,
                            tokens.accent,
                            tokens.text,
                        ]
                        .into_iter()
                        .map(|color| {
                            div()
                                .h(px(18.))
                                .w(px(32.))
                                .rounded_md()
                                .bg(rgb(color))
                                .border_1()
                                .border_color(self.color(LINE))
                        }),
                    ),
                )
            })
    }

    fn disable_theme_modules(&mut self) {
        let themes: Vec<_> = self
            .modules
            .modules
            .iter()
            .filter(|m| m.enabled && m.is_theme())
            .map(|m| m.id().to_string())
            .collect();
        for theme in themes {
            self.modules
                .set_enabled(&theme, false, &mut self.settings.module_grants);
        }
    }

    pub(super) fn apply_builtin_theme(
        &mut self,
        theme: crate::theme::Theme,
        cx: &mut Context<Self>,
    ) {
        if self.modules_loading {
            return;
        }
        self.disable_theme_modules();
        self.settings.theme = theme;
        self.refresh_messages();
        self.persist(cx);
    }

    fn install_catalog(&mut self, entry: &crate::modules::CatalogEntry, cx: &mut Context<Self>) {
        if self.modules_loading {
            return;
        }
        match self.modules.install(entry, &[]) {
            Ok(()) => {
                self.settings
                    .catalog_installed
                    .retain(|old| old.id() != entry.id());
                self.settings.catalog_installed.push(entry.clone());
                self.settings
                    .module_grants
                    .retain(|grant| grant.id != entry.id());
                self.catalog_notice = Some((
                    "Installed. Open Installed to enable or apply it.".into(),
                    false,
                ));
                self.refresh_messages();
                self.persist(cx);
            }
            Err(error) => self.catalog_notice = Some((error, true)),
        }
        cx.notify();
    }

    fn uninstall_catalog(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.modules_loading {
            return;
        }
        self.settings
            .catalog_installed
            .retain(|entry| entry.id() != id);
        self.settings.module_grants.retain(|grant| grant.id != id);
        self.modules.uninstall(id);
        self.catalog_notice = None;
        self.refresh_messages();
        self.persist(cx);
    }

    fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        if self.catalog_task.is_some() {
            return;
        }
        if self.preview_only {
            self.catalog = Catalog::bundled();
            cx.notify();
            return;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Err(error) = self.backend.command(Command::RefreshCatalog(tx)) {
            self.catalog_notice = Some((error, true));
            cx.notify();
            return;
        }
        self.catalog_notice = None;
        self.catalog_task = Some(cx.spawn(async move |view, cx| {
            let result = rx
                .await
                .unwrap_or_else(|_| Err("Catalog refresh was interrupted.".into()));
            let _ = view.update(cx, |this, cx| {
                this.catalog_task = None;
                match result {
                    Ok(catalog) => {
                        this.catalog = catalog;
                        this.catalog_notice = Some(("Catalog is up to date.".into(), false));
                    }
                    Err(error) => this.catalog_notice = Some((error, true)),
                }
                cx.notify();
            });
        }));
        cx.notify();
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
        let installed = self.settings.catalog_installed.clone();
        self.modules_task = Some(cx.spawn(async move |view, cx| {
            let registry = cx
                .background_executor()
                .spawn(async move {
                    crate::modules::Registry::load_with_catalog(
                        root.as_deref(),
                        &grants,
                        &installed,
                    )
                })
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
