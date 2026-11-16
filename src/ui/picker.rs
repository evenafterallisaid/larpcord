use super::*;
use crate::expressions::{Emoji, Sticker, Tab};

#[derive(Clone)]
struct EmojiChoice {
    text: String,
    name: String,
    source: String,
    scope: String,
    emoji: Option<Emoji>,
    locked: Option<&'static str>,
}
#[derive(Clone)]
struct StickerChoice {
    sticker: Sticker,
    source: String,
    scope: String,
}

enum PickerRow {
    Heading(String, String),
    Emoji(String, Vec<EmojiChoice>),
    Stickers(String, Vec<StickerChoice>),
}
impl PickerRow {
    fn scope(&self) -> &str {
        match self {
            Self::Heading(scope, _) | Self::Emoji(scope, _) | Self::Stickers(scope, _) => scope,
        }
    }
}

impl Larpcord {
    fn picker_rows(&self, cx: &Context<Self>) -> Vec<PickerRow> {
        let mut rows = Vec::new();
        if self.picker.tab == Tab::Emoji {
            let choices = self.emoji_choices(cx);
            let mut start = 0;
            while start < choices.len() {
                let scope = choices[start].scope.clone();
                let end = start
                    + choices[start..]
                        .iter()
                        .take_while(|c| c.scope == scope)
                        .count();
                rows.push(PickerRow::Heading(
                    scope.clone(),
                    choices[start].source.clone(),
                ));
                for chunk in choices[start..end].chunks(8) {
                    rows.push(PickerRow::Emoji(scope.clone(), chunk.to_vec()));
                }
                start = end;
            }
        } else {
            let choices = self.sticker_choices(cx);
            let mut start = 0;
            while start < choices.len() {
                let scope = choices[start].scope.clone();
                let end = start
                    + choices[start..]
                        .iter()
                        .take_while(|c| c.scope == scope)
                        .count();
                rows.push(PickerRow::Heading(
                    scope.clone(),
                    choices[start].source.clone(),
                ));
                for chunk in choices[start..end].chunks(3) {
                    rows.push(PickerRow::Stickers(scope.clone(), chunk.to_vec()));
                }
                start = end;
            }
        }
        rows
    }

    fn expression_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.picker_rows(cx);
        if rows.is_empty() {
            return self.picker_empty(
                if self.picker.packs_loading {
                    "Loading stickers…"
                } else {
                    "Nothing here yet"
                },
                self.picker
                    .packs_error
                    .as_deref()
                    .unwrap_or("Try another search. Your recent selections will appear here too."),
            );
        }
        if self.picker_sections.item_count() != rows.len() {
            self.picker_sections.reset(rows.len());
        }
        let scopes = rows
            .iter()
            .map(|row| row.scope().to_string())
            .collect::<Vec<_>>();
        let weak = cx.weak_entity();
        self.picker_sections
            .set_scroll_handler(move |event, _, cx| {
                if let Some(view) = weak.upgrade() {
                    view.update(cx, |this, cx| {
                        let scope = scopes.get(event.visible_range.start).cloned();
                        if this.picker.scope != scope {
                            this.picker.scope = scope;
                            cx.notify();
                        }
                    });
                }
            });
        let weak = cx.weak_entity();
        list(self.picker_sections.clone(), move |index, _, cx| {
            let Some(view) = weak.upgrade() else {
                return div().into_any_element();
            };
            view.update(cx, |this, cx| match &rows[index] {
                PickerRow::Heading(_, title) => row()
                    .id(("expression-heading", index))
                    .debug_selector(move || format!("expression-heading-{index}"))
                    .h(px(28.))
                    .px_2()
                    .text_color(this.color(MUTED))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.clone())
                    .into_any_element(),
                PickerRow::Emoji(_, choices) => {
                    this.emoji_row(choices, index.saturating_sub(1), cx)
                }
                PickerRow::Stickers(_, choices) => this.sticker_row(choices, index, cx),
            })
        })
        .size_full()
        .into_any_element()
    }
    pub(super) fn picker_catalog_ready(&mut self, cx: &mut Context<Self>) {
        if self.emoji_open && self.picker.tab != Tab::Gifs {
            let rows = self.picker_rows(cx);
            self.picker_sections.reset(rows.len());
            if let Some(scope) = self.picker.pending_scope.as_ref()
                && let Some(index) = rows.iter().position(|r| r.scope() == scope)
            {
                self.picker_sections.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: px(0.),
                });
                self.picker.pending_scope = None;
            }
        }
    }
    pub(super) fn close_picker(&mut self) {
        self.emoji_open = false;
        self.picker_debounce = None;
        self.picker.request += 1;
        self.picker.loading = false;
        self.picker.hovered = None;
        let _ = self.backend.command(Command::CancelGifs);
    }
    pub(super) fn toggle_picker(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        if self.emoji_open && self.picker.tab == tab {
            self.close_picker();
            window.focus(&self.composer.focus_handle(cx));
        } else {
            self.emoji_open = true;
            self.picker.tab = tab;
            self.picker.scope = None;
            self.picker_sections.reset(0);
            self.picker.hovered = None;
            self.emoji_search.update(cx, |input, cx| {
                input.placeholder = match tab {
                    Tab::Gifs => "Search GIFs",
                    Tab::Stickers => "Search stickers",
                    Tab::Emoji => "Find the perfect emoji",
                }
                .into();
                input.set("", cx);
            });
            self.load_picker_scope(cx);
            self.picker_changed(cx);
            window.focus(&self.emoji_search.focus_handle(cx));
        }
        cx.notify();
    }
    fn load_picker_scope(&mut self, cx: &mut Context<Self>) {
        if self.preview {
            return;
        }
        if self.picker.tab == Tab::Gifs
            && !self.picker.favorites_loaded
            && !self.picker.favorites_loading
        {
            self.picker.favorites_loading = true;
            if let Err(error) = self.backend.command(Command::GifFavorites(None)) {
                self.picker.favorites_loading = false;
                self.picker.favorites_error = Some(error);
            }
        }
        if self.picker.tab == Tab::Stickers
            && !self.picker.packs_loaded
            && !self.picker.packs_loading
        {
            self.picker.packs_loading = true;
            self.picker.packs_error = None;
            if let Err(error) = self.backend.command(Command::StickerPacks) {
                self.picker.packs_loading = false;
                self.picker.packs_error = Some(error);
            }
        }
        let id = self
            .picker
            .scope
            .as_ref()
            .filter(|id| crate::expressions::id(id))
            .cloned()
            .or_else(|| self.selected_guild.clone());
        if let Some(id) = id
            && self
                .store
                .guilds
                .iter()
                .find(|g| g.id == id)
                .is_some_and(|g| g.emojis.is_none() || g.stickers.is_none())
            && self.expression_attempts.insert(id.clone())
        {
            let _ = self.backend.command(Command::GuildExpressions(id));
        }
        cx.notify();
    }
    pub(super) fn picker_changed(&mut self, cx: &mut Context<Self>) {
        self.picker_sections.reset(0);
        self.picker.pending_scope = None;
        if self.picker.tab != Tab::Gifs || !self.emoji_search.read(cx).content.trim().is_empty() {
            self.picker.scope = None;
        }
        self.picker_scroll
            .scroll_to_item_strict(0, ScrollStrategy::Top);
        self.picker_debounce = None;
        if !self.emoji_open || self.picker.tab != Tab::Gifs {
            return;
        }
        self.picker.request += 1;
        let request = self.picker.request;
        let query = self
            .emoji_search
            .read(cx)
            .content
            .trim()
            .chars()
            .take(100)
            .collect::<String>();
        self.picker.query = query.clone();
        self.picker.gifs.clear();
        self.picker.error = None;
        if self.preview {
            self.picker.loading = false;
            return;
        }
        if self.emoji_search.read(cx).content.trim().is_empty()
            && matches!(self.picker.scope.as_deref(), Some("recent" | "favorites"))
        {
            self.picker.loading = false;
            return;
        }
        self.picker.loading = true;
        let _ = self.backend.command(Command::CancelGifs);
        self.picker_debounce = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(350))
                .await;
            let _ = view.update(cx, |this, cx| {
                if this.emoji_open && this.picker.tab == Tab::Gifs && this.picker.request == request
                {
                    if let Err(error) = this.backend.command(Command::Gifs { request, query }) {
                        this.picker.loading = false;
                        this.picker.error = Some(error);
                    }
                    cx.notify();
                }
            });
        }));
    }
    fn insert_expression(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.close_picker();
        self.composer.update(cx, |input, cx| input.insert(text, cx));
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }
    fn emoji_choices(&self, cx: &Context<Self>) -> Vec<EmojiChoice> {
        let query = self
            .emoji_search
            .read(cx)
            .content
            .trim()
            .trim_matches(':')
            .to_lowercase();
        let mut choices = Vec::new();
        for guild in &self.store.guilds {
            for emoji in guild.emojis.iter().flatten().take(1000) {
                if choices.len() >= 4096 {
                    break;
                }
                if !query.is_empty()
                    && !emoji.name.to_lowercase().contains(&query)
                    && !guild.name.to_lowercase().contains(&query)
                {
                    continue;
                }
                let premium = self.store.user.premium_type;
                let locked = if emoji.available == Some(false) {
                    Some("This emoji is unavailable")
                } else if premium == Some(0)
                    && (emoji.animated || self.selected_guild.as_ref() != Some(&guild.id))
                {
                    Some("Nitro is required to use this emoji here")
                } else if !emoji.roles.is_empty()
                    && self
                        .store
                        .own_roles
                        .get(&guild.id)
                        .is_some_and(|own| !emoji.roles.iter().any(|role| own.contains(role)))
                {
                    Some("This emoji is restricted to server roles")
                } else {
                    None
                };
                choices.push(EmojiChoice {
                    text: emoji.markup(),
                    name: emoji.name.clone(),
                    source: guild.name.clone(),
                    scope: guild.id.clone(),
                    emoji: Some(emoji.clone()),
                    locked,
                });
            }
        }
        choices.extend(
            composer::EMOJIS
                .iter()
                .filter(|(e, n)| query.is_empty() || n.contains(&query) || e.contains(&query))
                .map(|(e, n)| EmojiChoice {
                    text: e.to_string(),
                    name: n.to_string(),
                    source: "Unicode".into(),
                    scope: "unicode".into(),
                    emoji: None,
                    locked: None,
                }),
        );
        let mut recent = self
            .picker
            .recent
            .iter()
            .filter_map(|key| choices.iter().find(|c| &c.text == key).cloned())
            .collect::<Vec<_>>();
        for choice in &mut recent {
            choice.source = "Recently used".into();
            choice.scope = "recent".into();
        }
        recent.extend(choices);
        recent
    }
    fn sticker_choices(&self, cx: &Context<Self>) -> Vec<StickerChoice> {
        let query = self.emoji_search.read(cx).content.to_lowercase();
        let mut choices = Vec::new();
        for guild in &self.store.guilds {
            for sticker in guild.stickers.iter().flatten().take(500) {
                if choices.len() >= 4096 {
                    break;
                }
                if query.is_empty()
                    || sticker.name.to_lowercase().contains(&query)
                    || sticker.tags.to_lowercase().contains(&query)
                {
                    choices.push(StickerChoice {
                        sticker: sticker.clone(),
                        source: guild.name.clone(),
                        scope: guild.id.clone(),
                    });
                }
            }
        }
        for pack in &self.picker.packs {
            for sticker in &pack.stickers {
                if choices.len() >= 4096 {
                    break;
                }
                if query.is_empty()
                    || sticker.name.to_lowercase().contains(&query)
                    || sticker.tags.to_lowercase().contains(&query)
                {
                    choices.push(StickerChoice {
                        sticker: sticker.clone(),
                        source: pack.name.clone(),
                        scope: format!("pack:{}", pack.id),
                    });
                }
            }
        }
        let mut recent = self
            .settings
            .recent_stickers
            .iter()
            .filter(|s| {
                query.is_empty()
                    || s.name.to_lowercase().contains(&query)
                    || s.tags.to_lowercase().contains(&query)
            })
            .map(|sticker| StickerChoice {
                sticker: choices
                    .iter()
                    .find(|c| c.sticker.id == sticker.id)
                    .map(|c| c.sticker.clone())
                    .unwrap_or_else(|| sticker.clone()),
                source: "Recently used".into(),
                scope: "recent".into(),
            })
            .collect::<Vec<_>>();
        recent.extend(choices);
        recent
    }
    fn picker_scope(
        &self,
        id: Option<String>,
        name: String,
        image: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.picker.scope == id;
        let hint = name.clone();
        row()
            .id(SharedString::from(format!(
                "scope-{}",
                id.as_deref().unwrap_or("all")
            )))
            .size(px(32.))
            .justify_center()
            .rounded_lg()
            .cursor_pointer()
            .bg(if selected {
                self.color(SURFACE)
            } else {
                rgba(0x00000000)
            })
            .hover(|s| s.bg(self.color(SURFACE)))
            .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
            .child(
                image
                    .as_ref()
                    .and_then(|url| self.media.get(url))
                    .map(|image| img(image).size(px(24.)).rounded_md().into_any_element())
                    .unwrap_or_else(|| match id.as_deref() {
                        None => icon("grid", 20.).into_any_element(),
                        Some("recent") => icon("clock", 20.).into_any_element(),
                        Some("favorites") => icon("star", 20.).into_any_element(),
                        Some("unicode") => icon("smile", 20.).into_any_element(),
                        Some(scope) if scope.starts_with("pack:") => {
                            icon("sticker", 20.).into_any_element()
                        }
                        _ => div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name.chars().take(2).collect::<String>())
                            .into_any_element(),
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.picker.scope = id.clone();
                if this.picker.tab == Tab::Gifs {
                    this.picker_scroll
                        .scroll_to_item_strict(0, ScrollStrategy::Top);
                    this.picker_changed(cx);
                    this.picker.scope = id.clone();
                } else {
                    let rows = this.picker_rows(cx);
                    this.picker.pending_scope = id
                        .clone()
                        .filter(|id| !rows.iter().any(|row| row.scope() == id));
                    let target = id
                        .as_ref()
                        .and_then(|id| rows.iter().position(|row| row.scope() == id))
                        .unwrap_or(0);
                    this.picker_sections.scroll_to(ListOffset {
                        item_ix: target,
                        offset_in_item: px(0.),
                    });
                    this.load_picker_scope(cx);
                }
                cx.notify();
            }))
            .into_any_element()
    }
    fn picker_rail(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut scopes = vec![(None, "All".to_string(), None)];
        if self.picker.tab != Tab::Gifs {
            scopes.push((Some("recent".into()), "Recent".into(), None));
            if self.picker.tab == Tab::Emoji {
                scopes.push((Some("unicode".into()), "Unicode".into(), None));
            }
        }
        for guild in self.store.guilds.iter().take(250) {
            scopes.push((
                Some(guild.id.clone()),
                guild.name.clone(),
                crate::media::guild_icon(&guild.id, guild.icon.as_deref()),
            ));
        }
        if self.picker.tab == Tab::Stickers {
            for pack in &self.picker.packs {
                scopes.push((
                    Some(format!("pack:{}", pack.id)),
                    pack.name.clone(),
                    pack.stickers
                        .first()
                        .and_then(|s| crate::media::sticker(&s.id, s.format_type, false)),
                ));
            }
        }
        column()
            .w(px(48.))
            .h_full()
            .flex_shrink_0()
            .bg(self.color(PANEL))
            .child(
                uniform_list(
                    "expression-scopes",
                    scopes.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let (id, name, image) = &scopes[index];
                                row()
                                    .h(px(40.))
                                    .w_full()
                                    .justify_center()
                                    .child(this.picker_scope(
                                        id.clone(),
                                        name.clone(),
                                        image.clone(),
                                        cx,
                                    ))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .size_full(),
            )
            .into_any_element()
    }
    fn emoji_row(
        &self,
        choices: &[EmojiChoice],
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        row()
            .h(px(42.))
            .px(px(4.))
            .children(choices.iter().enumerate().map(|(n, c)| {
                let choice = c.clone();
                let hint = format!(
                    ":{}: · {}{}",
                    c.name,
                    c.source,
                    c.locked
                        .map(|reason| format!(" · {reason}"))
                        .unwrap_or_default()
                );
                let hover_key = c.text.clone();
                let image = c.emoji.as_ref().and_then(|e| {
                    self.media.get(
                        &e.image(self.motion() && self.picker.hovered.as_deref() == Some(&c.text)),
                    )
                });
                let image = image.or_else(|| {
                    c.emoji
                        .as_ref()
                        .and_then(|e| self.media.get(&e.image(false)))
                });
                row()
                    .id(("emoji-choice", index * 8 + n))
                    .debug_selector(move || format!("emoji-choice-{}", index * 8 + n))
                    .size(px(42.))
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .opacity(if c.locked.is_some() { 0.35 } else { 1. })
                    .hover(|s| s.bg(self.color(SURFACE)))
                    .child(if let Some(image) = image {
                        img(image.clone())
                            .id(("emoji-image", image.id.0))
                            .size(px(30.))
                            .object_fit(ObjectFit::Contain)
                            .into_any_element()
                    } else if c.emoji.is_some() {
                        label(format!(":{}:", c.name))
                            .truncate()
                            .text_size(px(9.))
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(25.))
                            .child(c.text.clone())
                            .into_any_element()
                    })
                    .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        this.picker.hovered = hovered.then(|| hover_key.clone());
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if choice.locked.is_none() {
                            this.picker.remember(choice.text.clone());
                            this.settings.recent_emoji = this.picker.recent.clone();
                            this.persist(cx);
                            this.insert_expression(&choice.text, window, cx);
                        }
                    }))
            }))
            .into_any_element()
    }
    fn sticker_row(
        &self,
        choices: &[StickerChoice],
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        row()
            .h(px(112.))
            .px_1()
            .children(choices.iter().enumerate().map(|(n, c)| {
                let sticker = c.sticker.clone();
                let hint = format!("{} · {}", sticker.name, c.source);
                let key = sticker.id.clone();
                let source = crate::media::sticker(
                    &sticker.id,
                    sticker.format_type,
                    self.motion() && self.picker.hovered.as_deref() == Some(&sticker.id),
                );
                let image = source.as_deref().and_then(|url| self.media.get(url));
                let image = image.or_else(|| {
                    crate::media::sticker(&sticker.id, sticker.format_type, false)
                        .and_then(|url| self.media.get(&url))
                });
                column()
                    .id(("sticker-choice", index * 3 + n))
                    .w(px(112.))
                    .h(px(108.))
                    .p_1()
                    .gap_1()
                    .items_center()
                    .rounded_lg()
                    .cursor_pointer()
                    .opacity(if sticker.available == Some(false) {
                        0.35
                    } else {
                        1.
                    })
                    .hover(|s| s.bg(self.color(SURFACE)))
                    .child(
                        row().size(px(78.)).justify_center().child(
                            image
                                .map(|image| {
                                    img(image.clone())
                                        .id(("sticker-preview", image.id.0))
                                        .size_full()
                                        .object_fit(ObjectFit::Contain)
                                        .into_any_element()
                                })
                                .unwrap_or_else(|| icon("sticker", 32.).into_any_element()),
                        ),
                    )
                    .child(
                        label(sticker.name.clone())
                            .max_w_full()
                            .truncate()
                            .text_size(px(10.)),
                    )
                    .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        this.picker.hovered = hovered.then(|| key.clone());
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if sticker.available != Some(false) {
                            this.settings.recent_stickers.retain(|s| s.id != sticker.id);
                            this.settings.recent_stickers.insert(0, sticker.clone());
                            this.settings.sanitize_recents();
                            this.persist(cx);
                            this.selected_sticker = Some(sticker.clone());
                            this.close_picker();
                            window.focus(&this.composer.focus_handle(cx));
                            cx.notify();
                        }
                    }))
            }))
            .into_any_element()
    }
    fn gif_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let recent = matches!(self.picker.scope.as_deref(), Some("recent" | "favorites"))
            && self.emoji_search.read(cx).content.trim().is_empty();
        if self.picker.loading && !recent {
            return self.picker_empty("Finding GIFs…", "Results appear here shortly.");
        }
        if let Some(error) = self.picker.error.as_ref().filter(|_| !recent) {
            return self.picker_empty("Couldn’t load GIFs", error);
        }
        let gifs = if self.picker.scope.as_deref() == Some("favorites")
            && self.emoji_search.read(cx).content.trim().is_empty()
        {
            self.picker.favorites.clone()
        } else if recent {
            self.settings.recent_gifs.clone()
        } else {
            self.picker.gifs.clone()
        };
        if gifs.is_empty() {
            return self.picker_empty(
                if self.preview {
                    "Find the right reaction"
                } else {
                    "No GIFs found"
                },
                if self.preview {
                    "Connect your account to browse and search GIFs."
                } else {
                    "Try another search."
                },
            );
        }

        uniform_list(
            "gif-picker-grid",
            gifs.len().div_ceil(2),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|index| {
                        row()
                            .h(px(132.))
                            .px_2()
                            .gap_2()
                            .children(gifs.iter().skip(index * 2).take(2).enumerate().map(
                                |(n, gif)| {
                                    let gif = gif.clone();
                                    let hint = gif.title.clone();
                                    let image = gif.preview.as_deref().and_then(|url| {
                                        let moving = if this.motion() {
                                            format!("{url}#motion")
                                        } else {
                                            url.to_string()
                                        };
                                        this.media.get(&moving).or_else(|| this.media.get(url))
                                    });
                                    let favorite = gif.clone();
                                    let saved =
                                        this.picker.favorites.iter().any(|g| g.url == gif.url);
                                    div()
                                        .id(("gif-choice", index * 2 + n))
                                        .relative()
                                        .w(px(196.))
                                        .h(px(124.))
                                        .rounded_lg()
                                        .overflow_hidden()
                                        .bg(this.color(SURFACE))
                                        .cursor_pointer()
                                        .child(
                                            image
                                                .map(|image| {
                                                    img(image)
                                                        .size_full()
                                                        .object_fit(ObjectFit::Cover)
                                                        .into_any_element()
                                                })
                                                .unwrap_or_else(|| {
                                                    row()
                                                        .size_full()
                                                        .justify_center()
                                                        .child(label("GIF"))
                                                        .into_any_element()
                                                }),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .bottom_0()
                                                .w_full()
                                                .px_2()
                                                .py_1()
                                                .bg(rgba(0x00000088))
                                                .child(
                                                    label(gif.title.clone())
                                                        .truncate()
                                                        .text_color(this.color(TEXT))
                                                        .text_size(px(10.)),
                                                ),
                                        )
                                        .child(this.gif_favorite_button(favorite, saved, cx))
                                        .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.settings.recent_gifs.retain(|g| g.url != gif.url);
                                            this.settings.recent_gifs.insert(0, gif.clone());
                                            this.settings.sanitize_recents();
                                            this.persist(cx);
                                            let text = if this.composer.read(cx).content.is_empty()
                                            {
                                                gif.url.clone()
                                            } else {
                                                format!(" {} ", gif.url)
                                            };
                                            this.insert_expression(&text, window, cx);
                                        }))
                                },
                            ))
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.picker_scroll.clone())
        .size_full()
        .into_any_element()
    }
    fn gif_favorite_button(
        &self,
        gif: crate::expressions::Gif,
        saved: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        row()
            .id("favorite-gif")
            .absolute()
            .top(px(6.))
            .right(px(6.))
            .size(px(26.))
            .rounded_full()
            .justify_center()
            .bg(rgba(0x111216d8))
            .cursor_pointer()
            .child(icon("star", 16.).text_color(self.color(if saved { 0xf0b232 } else { TEXT })))
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    Hint(
                        if saved {
                            "Remove favorite"
                        } else {
                            "Favorite GIF"
                        }
                        .into(),
                    )
                })
                .into()
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if this.picker.favorites_loading {
                    return;
                }
                if this.preview {
                    if saved {
                        this.picker.favorites.retain(|g| g.url != gif.url);
                    } else {
                        this.picker.favorites.insert(0, gif.clone());
                    }
                } else {
                    this.picker.favorites_loading = true;
                    this.picker.favorites_error = None;
                    if let Err(error) = this
                        .backend
                        .command(Command::GifFavorites(Some((gif.clone(), !saved))))
                    {
                        this.picker.favorites_loading = false;
                        this.picker.favorites_error = Some(error);
                    }
                }
                cx.notify();
            }))
            .into_any_element()
    }
    fn picker_empty(&self, title: &str, detail: &str) -> AnyElement {
        column()
            .size_full()
            .items_center()
            .justify_center()
            .px_4()
            .gap_2()
            .child(
                icon(
                    match self.picker.tab {
                        Tab::Gifs => "gif",
                        Tab::Stickers => "sticker",
                        Tab::Emoji => "smile",
                    },
                    32.,
                )
                .text_color(self.color(MUTED)),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_string()),
            )
            .child(label(detail.to_string()).text_size(px(11.)).text_center())
            .into_any_element()
    }
    pub(super) fn emoji_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.picker.tab;
        div()
            .id("emoji-backdrop")
            .absolute()
            .inset_0()
            .top(px(36.))
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_picker();
                    window.focus(&this.composer.focus_handle(cx));
                    cx.notify();
                }),
            )
            .child(
                column()
                    .id("emoji-picker")
                    .debug_selector(|| "expression-picker".into())
                    .absolute()
                    .right(px(if self.profile_inspector {
                        336.
                    } else if self.members_open
                        && (self.selected_guild.is_some()
                            || self
                                .selected_channel
                                .as_ref()
                                .and_then(|id| self.store.channels.get(id))
                                .is_some_and(|c| c.kind == 3))
                    {
                        256.
                    } else {
                        16.
                    }))
                    .bottom(px(76.))
                    .w(px(432.))
                    .h(px(440.))
                    .max_h(relative(0.85))
                    .rounded_xl()
                    .border_1()
                    .border_color(self.color(LINE))
                    .bg(self.color(SIDEBAR))
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        row()
                            .h(px(44.))
                            .flex_shrink_0()
                            .px_3()
                            .gap_2()
                            .border_b_1()
                            .border_color(self.color(LINE))
                            .children(
                                [
                                    (Tab::Gifs, "GIFs"),
                                    (Tab::Stickers, "Stickers"),
                                    (Tab::Emoji, "Emoji"),
                                ]
                                .into_iter()
                                .map(|(kind, title)| {
                                    button(SharedString::from(format!("picker-{title}")), title)
                                        .h(px(28.))
                                        .py_0()
                                        .px_3()
                                        .bg(if kind == tab {
                                            self.color(SURFACE)
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .text_color(self.color(if kind == tab {
                                            TEXT
                                        } else {
                                            MUTED
                                        }))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if this.picker.tab != kind {
                                                this.toggle_picker(kind, window, cx)
                                            }
                                        }))
                                }),
                            )
                            .child(div().flex_1())
                            .child(
                                toolbar_button("close-expression-picker", "close", "Close picker")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_picker();
                                        window.focus(&this.composer.focus_handle(cx));
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        row()
                            .h(px(44.))
                            .mx_3()
                            .my_2()
                            .px_2()
                            .bg(self.color(PANEL))
                            .rounded_md()
                            .gap_2()
                            .child(icon("search", 16.))
                            .child(self.emoji_search.clone())
                            .when(tab != Tab::Emoji, |s| {
                                s.child(
                                    toolbar_button(
                                        "retry-expression-search",
                                        "refresh",
                                        "Refresh results",
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            if this.picker.tab == Tab::Gifs {
                                                if this.picker.scope.as_deref() == Some("favorites")
                                                    && !this.picker.favorites_loading
                                                {
                                                    this.picker.favorites_loaded = false;
                                                    this.load_picker_scope(cx);
                                                } else {
                                                    this.picker_changed(cx);
                                                }
                                            } else {
                                                this.picker.packs_loaded = false;
                                                this.expression_attempts.clear();
                                                this.load_picker_scope(cx);
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                            }),
                    )
                    .when(tab == Tab::Gifs, |s| {
                        s.child(
                            row()
                                .px_3()
                                .pb_2()
                                .gap_2()
                                .child(self.picker_scope(None, "Trending".into(), None, cx))
                                .child(self.picker_scope(
                                    Some("recent".into()),
                                    "Recently used".into(),
                                    None,
                                    cx,
                                ))
                                .child(self.picker_scope(
                                    Some("favorites".into()),
                                    "Favorites".into(),
                                    None,
                                    cx,
                                )),
                        )
                    })
                    .child(
                        row()
                            .flex_1()
                            .min_h_0()
                            .items_start()
                            .when(tab != Tab::Gifs, |s| s.child(self.picker_rail(cx)))
                            .child(div().flex_1().min_w_0().h_full().child(match tab {
                                Tab::Emoji | Tab::Stickers => self.expression_grid(cx),
                                Tab::Gifs => self.gif_grid(cx),
                            })),
                    )
                    .when_some(
                        self.picker
                            .favorites_error
                            .clone()
                            .filter(|_| tab == Tab::Gifs),
                        |s, error| {
                            s.child(
                                label(error)
                                    .text_size(px(10.))
                                    .px_3()
                                    .py_1()
                                    .text_color(rgb(0xf0b7c3)),
                            )
                        },
                    )
                    .child(
                        row()
                            .h(px(32.))
                            .flex_shrink_0()
                            .px_3()
                            .border_t_1()
                            .border_color(self.color(LINE))
                            .child(
                                label(match tab {
                                    Tab::Emoji => "Choose an emoji to insert at your cursor",
                                    Tab::Stickers => "Add a sticker, then send when you’re ready",
                                    Tab::Gifs => {
                                        "GIFs by KLIPY · choose one to add to your message"
                                    }
                                })
                                .text_size(px(10.)),
                            ),
                    )
                    .map(|s| {
                        if self.motion() {
                            s.with_animation(
                                "picker-enter",
                                Animation::new(std::time::Duration::from_millis(180))
                                    .with_easing(ease_out_quint()),
                                |s, t| s.bottom(px(76. - 8. * (1. - t))).opacity(t),
                            )
                            .into_any_element()
                        } else {
                            s.into_any_element()
                        }
                    }),
            )
            .into_any_element()
    }
    pub(super) fn sticker_draft(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(sticker) = &self.selected_sticker else {
            return div().into_any_element();
        };
        row()
            .debug_selector(|| "sticker-draft".into())
            .gap_3()
            .p_2()
            .rounded_lg()
            .bg(self.color(PANEL))
            .child(
                row().size(px(64.)).justify_center().child(
                    crate::media::sticker(&sticker.id, sticker.format_type, false)
                        .and_then(|url| self.media.get(&url))
                        .map(|image| {
                            img(image)
                                .size_full()
                                .object_fit(ObjectFit::Contain)
                                .into_any_element()
                        })
                        .unwrap_or_else(|| icon("sticker", 28.).into_any_element()),
                ),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .child(sticker.name.clone()),
                    )
                    .child(label("Sticker · ready to send").text_size(px(11.))),
            )
            .child(
                toolbar_button("remove-sticker-draft", "close", "Remove sticker").on_click(
                    cx.listener(|this, _, _, cx| {
                        this.selected_sticker = None;
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Emoji, Larpcord, Sticker, Tab};
    use crate::network::{Envelope, Event};
    use gpui::{Modifiers, TestAppContext, point, px, size};

    #[gpui::test]
    fn picker_sections_follow_scrolling_and_keep_recent_groups_separate(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.settings.animations = false;
            v.emoji_open = true;
            v.store.user.premium_type = Some(2);
            v.store.guilds[0].emojis = Some(
                (1..=80)
                    .map(|n| Emoji {
                        id: n.to_string(),
                        name: format!("cat_{n}"),
                        ..Default::default()
                    })
                    .collect(),
            );
            v.picker.remember("<:cat_1:1>".into());
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let first = cx.debug_bounds("expression-heading-0").unwrap();
        view.update(cx, |v, cx| {
            let rows = v.picker_rows(cx);
            assert_eq!(rows[0].scope(), "recent");
            assert_eq!(rows[2].scope(), v.store.guilds[0].id);
        });
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: first.center() + point(px(50.), px(70.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-250.))),
            modifiers: Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert_eq!(v.picker.scope.as_ref(), Some(&v.store.guilds[0].id));
            assert!(v.picker_sections.logical_scroll_top().item_ix > 1);
        });
    }

    #[gpui::test]
    fn clicking_custom_emoji_inserts_at_the_cursor_without_sending(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |view, cx| {
            view.store.user.premium_type = Some(2);
            view.store.guilds[0].emojis = Some(vec![Emoji {
                id: "123".into(),
                name: "wave".into(),
                ..Default::default()
            }]);
            view.composer
                .update(cx, |input, cx| input.set("hello ", cx));
            view.emoji_open = true;
            view.emoji_search
                .update(cx, |input, cx| input.set("wave", cx));
            cx.notify();
        });
        cx.draw(point(px(0.), px(0.)), size(px(1100.), px(730.)), |_, _| {
            view.clone()
        });
        let tile = cx.debug_bounds("emoji-choice-0").unwrap();
        cx.simulate_click(tile.center(), Modifiers::default());
        cx.update(|_, cx| {
            let state = view.read(cx);
            assert_eq!(
                state.composer.read(cx).content.as_ref(),
                "hello <:wave:123>"
            );
            assert!(!state.emoji_open);
            assert!(state.sending.is_none());
            assert_eq!(state.picker.recent[0], "<:wave:123>");
        });
    }

    #[gpui::test]
    fn stickers_survive_channel_switches_and_can_send_without_text(cx: &mut TestAppContext) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.selected_sticker = Some(Sticker {
                    id: "123".into(),
                    name: "wave".into(),
                    format_type: 1,
                    ..Default::default()
                });
                view.choose_channel("build-log".into(), window, cx);
                assert!(view.selected_sticker.is_none());
                view.choose_channel("campfire".into(), window, cx);
                assert_eq!(view.selected_sticker.as_ref().unwrap().id, "123");
                view.send(window, cx);
                let sent = view.store.channel_messages("campfire").last().unwrap();
                assert!(sent.content.is_empty());
                assert_eq!(sent.sticker_items[0].id, "123");
                assert!(view.selected_sticker.is_none());
            })
            .unwrap();
    }

    #[gpui::test]
    fn stale_gif_searches_cannot_replace_results_or_reopen_a_closed_picker(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|window, cx| Larpcord::with_preview(window, cx, true));
        window
            .update(cx, |view, window, cx| {
                view.emoji_open = true;
                view.picker.tab = Tab::Gifs;
                view.picker.request = 2;
                let result = || {
                    crate::expressions::gifs(
                        serde_json::json!([{"url":"https://klipy.com/gifs/cat","title":"cat"}]),
                    )
                };
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::Gifs {
                            request: 1,
                            result: Ok(result()),
                        },
                    },
                    window,
                    cx,
                );
                assert!(view.picker.gifs.is_empty());
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::Gifs {
                            request: 2,
                            result: Ok(result()),
                        },
                    },
                    window,
                    cx,
                );
                assert_eq!(view.picker.gifs.len(), 1);
                view.close_picker();
                view.picker.gifs.clear();
                view.receive(
                    Envelope {
                        generation: 0,
                        event: Event::Gifs {
                            request: 2,
                            result: Ok(result()),
                        },
                    },
                    window,
                    cx,
                );
                assert!(view.picker.gifs.is_empty());
                assert!(!view.emoji_open);
            })
            .unwrap();
    }

    #[gpui::test]
    fn picker_fits_a_small_window_and_profile_column(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        cx.simulate_resize(size(px(960.), px(580.)));
        for tab in [Tab::Emoji, Tab::Gifs, Tab::Stickers] {
            view.update(cx, |view, cx| {
                view.emoji_open = true;
                view.picker.tab = tab;
                view.profile_inspector = true;
                cx.notify();
            });
            cx.draw(point(px(0.), px(0.)), size(px(960.), px(580.)), |_, _| {
                view.clone()
            });
            let panel = cx.debug_bounds("expression-picker").unwrap();
            assert!(
                panel.origin.x >= px(0.) && panel.right() <= px(960.),
                "picker outside window: {panel:?}"
            );
            assert!(
                panel.origin.y >= px(36.) && panel.bottom() <= px(580.),
                "picker outside window: {panel:?}"
            );
        }
    }
}
