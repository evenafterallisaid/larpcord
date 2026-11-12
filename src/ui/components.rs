use super::*;

impl Larpcord {
    fn interact(&mut self, message: &Message, data: Value, cx: &mut Context<Self>) {
        if self.settings_tab.is_some()
            || self.login_open
            || self.profile.is_some()
            || self.interaction.is_some()
        {
            return;
        }
        if self.preview {
            self.error = Some("Sample controls stay offline.".into());
            cx.notify();
            return;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
            .to_string();
        let command = Command::Interact {
            message: message.id.clone(),
            channel: message.channel_id.clone(),
            guild: self
                .store
                .channels
                .get(&message.channel_id)
                .and_then(|c| c.guild_id.clone()),
            application: message
                .application_id
                .clone()
                .unwrap_or_else(|| message.author.id.clone()),
            flags: message.flags,
            data,
            nonce,
        };
        match self.backend.command(command) {
            Ok(()) => self.interaction = Some(message.id.clone()),
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn open_component_menu(
        &mut self,
        message: Message,
        component: crate::components::Component,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if component.disabled {
            return;
        }
        self.previous_focus = window.focused(cx);
        window.focus(&self.modal_focus);
        self.component_values = component
            .options
            .iter()
            .filter(|o| o.default)
            .map(|o| o.value.clone())
            .collect();
        self.component_menu = Some((message, component));
        cx.notify();
    }

    fn component_media(
        &self,
        message: &Message,
        media: &crate::components::Media,
        spoiler: bool,
        thumbnail: bool,
    ) -> AnyElement {
        let attachment = media
            .url
            .strip_prefix("attachment://")
            .and_then(|name| message.attachments.iter().find(|a| a.filename == name));
        let url = attachment
            .map(|a| a.url.as_str())
            .unwrap_or(&media.url)
            .to_string();
        let source = media
            .proxy_url
            .as_deref()
            .or_else(|| attachment.and_then(|a| a.proxy_url.as_deref()))
            .filter(|u| crate::media::cdn_url(u))
            .unwrap_or(&url);
        let visual = if spoiler {
            label("Spoiler image").into_any_element()
        } else if thumbnail {
            div()
                .size(px(80.))
                .rounded_lg()
                .overflow_hidden()
                .when_some(self.media.get(source), |s, image| {
                    s.child(img(image).size_full().object_fit(ObjectFit::Cover))
                })
                .into_any_element()
        } else {
            self.media_image(
                source,
                media.width.or(attachment.and_then(|a| a.width)),
                media.height.or(attachment.and_then(|a| a.height)),
            )
        };
        column()
            .gap_1()
            .child(visual)
            .child(
                button("open-media", "Open ↗")
                    .py_1()
                    .text_size(px(10.))
                    .on_click(move |_, _, cx| open_https(&url, cx)),
            )
            .into_any_element()
    }

    pub(super) fn component(
        &self,
        message: &Message,
        component: &crate::components::Component,
        path: String,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if depth > 8 {
            return label("More content is not supported yet.").into_any_element();
        }
        let id = SharedString::from(format!(
            "component-{}-{path}-{}",
            message.id,
            component.id.unwrap_or(0)
        ));
        let mut children = || {
            component
                .components
                .iter()
                .take(40)
                .enumerate()
                .map(|(i, c)| self.component(message, c, format!("{path}-{i}"), depth + 1, cx))
                .collect::<Vec<_>>()
        };
        match component.kind {
            1 => row()
                .id(id)
                .gap_2()
                .flex_wrap()
                .children(children())
                .into_any_element(),
            2 => {
                let disabled =
                    component.disabled || component.style == Some(6) || self.interaction.is_some();
                let mut control = button(id, "")
                    .gap_1()
                    .min_h(px(32.))
                    .text_size(px(12.))
                    .bg(rgb(match component.style {
                        Some(1) => 0x5865f2,
                        Some(3) => 0x248046,
                        Some(4) => 0xda373c,
                        _ => SURFACE,
                    }))
                    .opacity(if disabled { 0.5 } else { 1. });
                if let Some(emoji) = &component.emoji {
                    if let Some(id) = &emoji.id {
                        control = control.child(self.rich_text(&format!(
                            "<:{}:{id}>",
                            emoji.name.as_deref().unwrap_or("emoji")
                        )));
                    } else if let Some(name) = &emoji.name {
                        control = control.child(name.clone());
                    }
                }
                let url = component.url.clone();
                let custom = component.custom_id.clone();
                let message = message.clone();
                control
                    .child(component.label.clone().unwrap_or_else(|| "Open".into()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if disabled {
                            return;
                        }
                        if let Some(url) = &url {
                            open_https(url, cx);
                        } else if let Some(custom) = &custom {
                            this.interact(
                                &message,
                                serde_json::json!({"component_type":2,"custom_id":custom}),
                                cx,
                            );
                        }
                    }))
                    .into_any_element()
            }
            3 => {
                let message = message.clone();
                let component = component.clone();
                let label = component
                    .placeholder
                    .clone()
                    .unwrap_or_else(|| "Choose an option".into());
                button(id, "")
                    .w_full()
                    .justify_between()
                    .child(label)
                    .child("⌄")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_component_menu(message.clone(), component.clone(), window, cx)
                    }))
                    .into_any_element()
            }
            5..=8 => label(
                component
                    .placeholder
                    .clone()
                    .unwrap_or_else(|| "This selector is not available yet.".into()),
            )
            .into_any_element(),
            9 => row()
                .id(id)
                .items_start()
                .gap_3()
                .child(column().flex_1().min_w_0().gap_2().children(children()))
                .when_some(component.accessory.as_deref(), |s, accessory| {
                    s.child(self.component(
                        message,
                        accessory,
                        format!("{path}-accessory"),
                        depth + 1,
                        cx,
                    ))
                })
                .into_any_element(),
            10 => div()
                .id(id)
                .w_full()
                .min_w_0()
                .child(self.rich_text(component.content.as_deref().unwrap_or_default()))
                .into_any_element(),
            11 => component.media.as_ref().map_or_else(
                || div().into_any_element(),
                |media| self.component_media(message, media, component.spoiler, true),
            ),
            12 => column()
                .id(id)
                .gap_2()
                .children(
                    component
                        .items
                        .iter()
                        .take(10)
                        .enumerate()
                        .map(|(i, item)| {
                            column()
                                .id(("gallery-item", i))
                                .gap_1()
                                .child(self.component_media(
                                    message,
                                    &item.media,
                                    item.spoiler,
                                    false,
                                ))
                                .when_some(item.description.clone(), |s, text| s.child(label(text)))
                        }),
                )
                .into_any_element(),
            13 => component.file.as_ref().map_or_else(
                || label("File attachment").into_any_element(),
                |media| {
                    let url = media
                        .url
                        .strip_prefix("attachment://")
                        .and_then(|name| message.attachments.iter().find(|a| a.filename == name))
                        .map(|a| a.url.clone())
                        .unwrap_or_else(|| media.url.clone());
                    button(id, "Open file ↗")
                        .on_click(move |_, _, cx| open_https(&url, cx))
                        .into_any_element()
                },
            ),
            14 => div()
                .id(id)
                .my(px(if component.spacing == Some(2) {
                    12.
                } else {
                    6.
                }))
                .h(px(1.))
                .when(component.divider.unwrap_or(true), |s| s.bg(rgb(0x4e5058)))
                .into_any_element(),
            17 => column()
                .id(id)
                .w_full()
                .max_w(px(520.))
                .min_w_0()
                .gap_3()
                .p_4()
                .rounded_lg()
                .bg(rgb(0x2b2d31))
                .when_some(component.accent_color, |s, color| {
                    s.border_l_4().border_color(rgb(color & 0xffffff))
                })
                .children(children())
                .into_any_element(),
            _ => {
                label("This message contains content that is not supported yet.").into_any_element()
            }
        }
    }

    pub(super) fn component_menu_panel(
        &self,
        message: &Message,
        component: &crate::components::Component,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let multi = component.max_values.unwrap_or(1) > 1;
        let minimum = component.min_values.unwrap_or(1);
        let maximum = component.max_values.unwrap_or(1).min(25);
        let valid = (minimum..=maximum).contains(&self.component_values.len());
        let custom = component.custom_id.clone().unwrap_or_default();
        let selected_message = message.clone();
        column().id("component-menu").track_focus(&self.modal_focus).occlude().absolute().inset_0().top(px(32.)).bg(rgba(0x00000099)).items_center().justify_center()
            .child(column().w(px(400.)).max_h(relative(0.8)).p_4().gap_3().rounded_xl().bg(rgb(PANEL)).shadow_lg()
                .child(row().justify_between().child(div().font_weight(FontWeight::SEMIBOLD).child(component.placeholder.clone().unwrap_or_else(|| "Choose an option".into())))
                    .child(button("close-options","×").on_click(cx.listener(|this,_,window,cx| {this.component_menu=None;if let Some(focus)=this.previous_focus.take(){window.focus(&focus);}cx.notify();}))))
                .child(column().id("select-options").min_h_0().overflow_y_scroll().gap_1().children(component.options.iter().take(25).enumerate().map(|(i,option)| {
                    let value=option.value.clone();let custom=custom.clone();let message=message.clone();
                    column().id(("select-option",i)).p_3().gap_1().rounded_md().cursor_pointer().bg(rgb(if self.component_values.contains(&option.value){SURFACE}else{PANEL})).hover(|s|s.bg(rgb(SURFACE)))
                        .child(div().text_size(px(13.)).child(option.label.clone())).when_some(option.description.clone(),|s,text|s.child(label(text)))
                        .on_click(cx.listener(move |this,_,window,cx| {
                            if multi {if !this.component_values.remove(&value)&&this.component_values.len()<maximum {this.component_values.insert(value.clone());}}
                            else {this.component_menu=None;if let Some(focus)=this.previous_focus.take(){window.focus(&focus);}this.interact(&message,serde_json::json!({"component_type":3,"custom_id":custom,"values":[value]}),cx);}
                            cx.notify();
                        }))
                })))
                .when(multi,|s|s.child(button("submit-options","Apply selection").opacity(if valid{1.}else{0.5}).on_click(cx.listener(move |this,_,window,cx| {
                    if !valid{return;}
                    let mut values=this.component_values.iter().cloned().collect::<Vec<_>>();values.sort();
                    this.component_menu=None;if let Some(focus)=this.previous_focus.take(){window.focus(&focus);}
                    this.interact(&selected_message,serde_json::json!({"component_type":3,"custom_id":custom,"values":values}),cx);
                })))))
            .into_any_element()
    }
}
