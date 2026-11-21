use super::*;
use crate::profile_edit::{Artwork, Fields, ImageEdit};

pub(super) const TAB: usize = 4;

pub(super) struct Editor {
    before: Option<Profile>,
    name: Entity<TextInput>,
    bio: Entity<TextInput>,
    pronouns: Entity<TextInput>,
    color: Entity<TextInput>,
    avatar: ImageEdit,
    banner: ImageEdit,
    pending: Option<bool>,
    _subscriptions: Vec<Subscription>,
    needs_reload: bool,
    artwork: Option<Task<()>>,
    notice: Option<(String, bool)>,
}

impl Editor {
    fn new(cx: &mut Context<Larpcord>) -> Self {
        let name = cx.new(|cx| TextInput::new("Use your username", false, cx));
        let bio = cx.new(|cx| {
            let mut field = TextInput::new("A little about you", false, cx);
            field.preserve_newlines = true;
            field
        });
        let pronouns = cx.new(|cx| TextInput::new("Optional", false, cx));
        let color = cx.new(|cx| TextInput::new("Default color", false, cx));
        let subscriptions = [&name, &bio, &pronouns, &color]
            .into_iter()
            .map(|input| cx.subscribe(input, |_, _, _: &Changed, cx| cx.notify()))
            .collect();
        Self {
            before: None,
            name,
            bio,
            pronouns,
            color,
            _subscriptions: subscriptions,
            avatar: ImageEdit::Keep,
            banner: ImageEdit::Keep,
            pending: None,
            needs_reload: false,
            artwork: None,
            notice: None,
        }
    }

    fn fields(&self, cx: &App) -> Fields {
        Fields {
            name: self.name.read(cx).content.to_string(),
            bio: self.bio.read(cx).content.to_string(),
            pronouns: self.pronouns.read(cx).content.to_string(),
            color: self.color.read(cx).content.to_string(),
        }
    }

    fn fill(&mut self, profile: Profile, cx: &mut Context<Larpcord>) {
        let fields = Fields::from_profile(&profile);
        for (input, value) in [
            (&self.name, fields.name),
            (&self.bio, fields.bio),
            (&self.pronouns, fields.pronouns),
            (&self.color, fields.color),
        ] {
            input.update(cx, |input, cx| input.set(&value, cx));
        }
        self.before = Some(profile);
        self.avatar = ImageEdit::Keep;
        self.banner = ImageEdit::Keep;
        self.needs_reload = false;
    }

    fn image(&mut self, kind: Artwork) -> &mut ImageEdit {
        match kind {
            Artwork::Avatar => &mut self.avatar,
            Artwork::Banner => &mut self.banner,
        }
    }
}

impl Larpcord {
    pub(super) fn select_settings_tab(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.settings_tab = Some(tab);
        if tab == TAB
            && self.profile_editor.is_none()
            && !self.preview
            && self.status == "Connected"
        {
            self.load_profile_editor(cx);
        }
        cx.notify();
    }

    fn load_profile_editor(&mut self, cx: &mut Context<Self>) {
        if self.preview
            || self.status != "Connected"
            || self
                .profile_editor
                .as_ref()
                .is_some_and(|e| e.pending.is_some())
        {
            return;
        }
        let mut editor = self
            .profile_editor
            .take()
            .unwrap_or_else(|| Editor::new(cx));
        editor.artwork = None;
        editor.notice = None;
        let command = Command::EditProfile {
            id: self.store.user.id.clone(),
            changes: None,
        };
        match self.backend.command(command) {
            Ok(()) => editor.pending = Some(false),
            Err(error) => editor.notice = Some((error, true)),
        }
        self.profile_editor = Some(editor);
        cx.notify();
    }

    pub(super) fn edited_profile(
        &mut self,
        id: String,
        saved: bool,
        result: Result<Box<Profile>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.user.id != id {
            return;
        }
        let Some(editor) = self.profile_editor.as_mut() else {
            return;
        };
        editor.pending = None;
        match result {
            Ok(mut profile) => {
                profile.user.premium_type = profile
                    .premium_type
                    .or(profile.user.premium_type)
                    .or(self.store.user.premium_type);
                crate::media::retire(
                    [editor.avatar.image(), editor.banner.image()]
                        .into_iter()
                        .flatten()
                        .collect(),
                    window,
                    3,
                );
                self.store.user = profile.user.clone();
                if let Some(user) = self.store.users.get_mut(&id) {
                    *user = profile.user.clone();
                }
                self.profiles.retain(|(user, _), _| user != &id);
                if self.profile.as_ref().is_some_and(|p| p.user.id == id)
                    && self.profile_guild.is_none()
                {
                    self.profile = Some((*profile).clone());
                }
                self.profiles
                    .insert((id, None), ((*profile).clone(), std::time::Instant::now()));
                editor.fill(*profile, cx);
                editor.notice = saved.then(|| ("Profile saved on Discord.".into(), false));
            }
            Err(error) => {
                editor.needs_reload |= saved;
                editor.notice = Some((error, true));
            }
        }
        cx.notify();
    }

    fn save_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview || self.status != "Connected" {
            return;
        }
        let Some(editor) = self.profile_editor.as_mut() else {
            return;
        };
        let Some(before) = &editor.before else {
            return;
        };
        if editor.pending.is_some()
            || editor.needs_reload
            || editor.artwork.is_some()
            || before.user.id != self.store.user.id
        {
            return;
        }
        match editor
            .fields(cx)
            .patch(before, &editor.avatar, &editor.banner)
        {
            Ok(patch) if patch.is_empty() => {
                editor.notice = Some(("No changes to save.".into(), false))
            }
            Ok(patch) => match self.backend.command(Command::EditProfile {
                id: before.user.id.clone(),
                changes: Some(patch),
            }) {
                Ok(()) => {
                    window.focus(&self.modal_focus);
                    editor.pending = Some(true);
                    editor.notice = None;
                }
                Err(error) => editor.notice = Some((error, true)),
            },
            Err(error) => editor.notice = Some((error, true)),
        }
        cx.notify();
    }

    fn select_artwork(&mut self, kind: Artwork, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.profile_editor.as_mut() else {
            return;
        };
        if editor.pending.is_some() || editor.needs_reload || editor.artwork.is_some() {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(format!("Choose {}", kind.label().to_lowercase()).into()),
        });
        editor.artwork = Some(cx.spawn_in(window, async move |view, cx| {
            let result = match selection.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        Some(
                            cx.background_executor()
                                .spawn(
                                    async move { crate::profile_edit::prepare_image(&path, kind) },
                                )
                                .await,
                        )
                    } else {
                        None
                    }
                }
                Ok(Ok(None)) => None,
                _ => Some(Err("Could not open the image picker.".into())),
            };
            let _ = view.update_in(cx, |this, window, cx| {
                let Some(editor) = this.profile_editor.as_mut() else {
                    return;
                };
                editor.artwork = None;
                match result {
                    Some(Ok(upload)) => {
                        let old = editor.image(kind);
                        crate::media::retire(old.image().into_iter().collect(), window, 3);
                        *old = ImageEdit::Replace(upload);
                        editor.notice = None;
                    }
                    Some(Err(error)) => editor.notice = Some((error, true)),
                    None => {}
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn edit_profile_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = column().gap_4().child(
            div()
                .text_size(px(23.))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Edit profile"),
        );
        if self.preview || self.status != "Connected" {
            return panel
                .child(label("Connect Discord to edit your account profile."))
                .into_any_element();
        }
        let Some(editor) = &self.profile_editor else {
            return panel
                .child(
                    button("load-account-profile", "Load profile")
                        .on_click(cx.listener(|this, _, _, cx| this.load_profile_editor(cx))),
                )
                .into_any_element();
        };
        if let Some(saving) = editor.pending {
            return panel
                .child(label(if saving {
                    "Saving profile…"
                } else {
                    "Loading profile…"
                }))
                .into_any_element();
        }
        if let Some((notice, warning)) = &editor.notice {
            panel =
                panel.child(label(notice.clone()).when(*warning, |e| e.text_color(rgb(0xf0b7c3))));
        }
        let Some(before) = &editor.before else {
            return panel
                .child(
                    button("retry-account-profile", "Try again")
                        .on_click(cx.listener(|this, _, _, cx| this.load_profile_editor(cx))),
                )
                .into_any_element();
        };
        let fields = editor.fields(cx);
        let busy = editor.artwork.is_some();
        let banner = editor.banner.image().or_else(|| {
            if matches!(editor.banner, ImageEdit::Keep) {
                before
                    .banner()
                    .and_then(|(hash, _)| crate::media::banner(&before.user.id, None, hash))
                    .and_then(|url| self.media.get(&url))
            } else {
                None
            }
        });
        let avatar = editor.avatar.image().or_else(|| {
            if matches!(editor.avatar, ImageEdit::Keep) {
                crate::media::avatar(&before.user.id, before.user.avatar.as_deref())
                    .and_then(|url| self.media.get(&url))
            } else {
                None
            }
        });
        let accent = fields
            .color()
            .ok()
            .flatten()
            .unwrap_or_else(|| before.accent());
        let backdrop = self.color(SIDEBAR);
        let preview = column()
            .flex_basis(px(216.))
            .flex_grow()
            .min_w(px(200.))
            .gap_2()
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(self.color(MUTED))
                    .child("Profile preview"),
            )
            .child(
                column()
                    .relative()
                    .overflow_hidden()
                    .rounded_xl()
                    .border_1()
                    .border_color(self.color(LINE))
                    .bg(backdrop)
                    .child(
                        div()
                            .relative()
                            .h(px(100.))
                            .flex_shrink_0()
                            .bg(linear_gradient(
                                112.,
                                linear_color_stop(backdrop.blend(rgba((accent << 8) | 0x80)), 0.),
                                linear_color_stop(backdrop.blend(rgba((accent << 8) | 0x32)), 1.),
                            ))
                            .when_some(banner, |s, image| {
                                s.child(
                                    img(image)
                                        .absolute()
                                        .inset_0()
                                        .size_full()
                                        .object_fit(ObjectFit::Cover),
                                )
                            }),
                    )
                    .child(
                        column()
                            .p_3()
                            .pt(px(40.))
                            .gap_1()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(18.))
                                    .child(if fields.name.trim().is_empty() {
                                        before.user.username.clone()
                                    } else {
                                        fields.name.clone()
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(self.color(MUTED))
                                    .child(if fields.pronouns.trim().is_empty() {
                                        format!("@{}", before.user.username)
                                    } else {
                                        format!("@{} · {}", before.user.username, fields.pronouns)
                                    }),
                            )
                            .when(!fields.bio.trim().is_empty(), |s| {
                                s.child(
                                    column()
                                        .mt_2()
                                        .p_2()
                                        .gap_1()
                                        .bg(self.color(PANEL))
                                        .rounded_md()
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(self.color(MUTED))
                                                .child("About me"),
                                        )
                                        .child(self.rich_text(&fields.bio)),
                                )
                            }),
                    )
                    .child(
                        row()
                            .absolute()
                            .top(px(66.))
                            .left(px(12.))
                            .size(px(68.))
                            .rounded_full()
                            .justify_center()
                            .bg(backdrop)
                            .child(
                                row()
                                    .size(px(60.))
                                    .rounded_full()
                                    .overflow_hidden()
                                    .justify_center()
                                    .bg(self.color(SURFACE))
                                    .map(|s| match avatar {
                                        Some(image) => s.child(
                                            img(image)
                                                .size_full()
                                                .rounded_full()
                                                .object_fit(ObjectFit::Cover),
                                        ),
                                        None => s.child(
                                            before
                                                .user
                                                .username
                                                .chars()
                                                .next()
                                                .unwrap_or('?')
                                                .to_string(),
                                        ),
                                    }),
                            ),
                    ),
            );
        if editor.needs_reload {
            return panel
                .child(preview)
                .child(
                    button("reload-account-profile", "Reload from Discord")
                        .on_click(cx.listener(|this, _, _, cx| this.load_profile_editor(cx))),
                )
                .into_any_element();
        }
        let mut form = column()
            .gap_3()
            .flex_basis(px(300.))
            .flex_grow()
            .min_w(px(240.));
        for (caption, input) in [
            ("Display name", &editor.name),
            ("Pronouns", &editor.pronouns),
            ("About me", &editor.bio),
            ("Profile color (hex)", &editor.color),
        ] {
            form = form.child(
                column()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(caption),
                    )
                    .child(
                        div()
                            .rounded_md()
                            .border_1()
                            .border_color(self.color(LINE))
                            .px_3()
                            .overflow_hidden()
                            .bg(self.color(PANEL))
                            .child(input.clone()),
                    ),
            );
        }
        for (kind, can_change, edit, has_image) in [
            (
                Artwork::Avatar,
                true,
                &editor.avatar,
                before.user.avatar.is_some(),
            ),
            (
                Artwork::Banner,
                before
                    .premium_type
                    .or(before.user.premium_type)
                    .or(self.store.user.premium_type)
                    .is_some_and(|tier| tier == 2),
                &editor.banner,
                before.banner().is_some(),
            ),
        ] {
            let name = kind.label();
            form = form.child(
                row()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        div()
                            .w(px(44.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .when(can_change, |s| {
                        s.child(
                            button(SharedString::from(format!("choose-{name}")), "Choose image")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_artwork(kind, window, cx)
                                })),
                        )
                        .when(has_image && !matches!(edit, ImageEdit::Remove), |s| {
                            s.child(
                                button(SharedString::from(format!("remove-{name}")), "Remove")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(editor) = this.profile_editor.as_mut()
                                            && editor.artwork.is_none()
                                        {
                                            let old = editor.image(kind);
                                            crate::media::retire(
                                                old.image().into_iter().collect(),
                                                window,
                                                3,
                                            );
                                            *old = ImageEdit::Remove;
                                            cx.notify();
                                        }
                                    })),
                            )
                        })
                        .when(!matches!(edit, ImageEdit::Keep), |s| {
                            s.child(
                                button(SharedString::from(format!("undo-{name}")), "Undo")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(editor) = this.profile_editor.as_mut()
                                            && editor.artwork.is_none()
                                        {
                                            let old = editor.image(kind);
                                            crate::media::retire(
                                                old.image().into_iter().collect(),
                                                window,
                                                3,
                                            );
                                            *old = ImageEdit::Keep;
                                            cx.notify();
                                        }
                                    })),
                            )
                        })
                    })
                    .when(!can_change, |s| {
                        s.child(label("Custom banners require Discord Nitro."))
                    }),
            );
        }
        panel
            .child(
                row()
                    .items_start()
                    .flex_wrap()
                    .gap_4()
                    .child(form)
                    .child(preview),
            )
            .child(
                row()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(label("Image formats: PNG, JPEG or WebP.").text_size(px(11.)))
                    .child(
                        button(
                            "save-account-profile",
                            if busy {
                                "Preparing image…"
                            } else {
                                "Save profile"
                            },
                        )
                        .bg(self.color(SURFACE))
                        .on_click(cx.listener(|this, _, window, cx| this.save_profile(window, cx))),
                    ),
            )
            .into_any_element()
    }
}
