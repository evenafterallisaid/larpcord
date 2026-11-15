use super::*;

pub(super) const EMOJIS: &[(&str, &str)] = &[
    ("😀", "grinning happy"),
    ("😃", "smile happy"),
    ("😄", "smile"),
    ("😁", "grin"),
    ("😆", "laugh"),
    ("😂", "joy laugh"),
    ("🤣", "rofl laugh"),
    ("🥹", "happy tears"),
    ("😊", "blush happy"),
    ("🙂", "smile"),
    ("🙃", "upside down"),
    ("😉", "wink"),
    ("😍", "heart eyes love"),
    ("🥰", "love hearts"),
    ("😘", "kiss"),
    ("😋", "yum"),
    ("😎", "cool sunglasses"),
    ("🤓", "nerd"),
    ("🧐", "monocle"),
    ("🤔", "thinking"),
    ("🫡", "salute"),
    ("🤨", "raised eyebrow"),
    ("😐", "neutral"),
    ("😶", "speechless"),
    ("😔", "sad"),
    ("🥺", "pleading"),
    ("😢", "cry"),
    ("😭", "sob cry"),
    ("😤", "triumph"),
    ("😠", "angry"),
    ("🤬", "angry swear"),
    ("💀", "skull dead"),
    ("🤡", "clown"),
    ("👻", "ghost"),
    ("👽", "alien"),
    ("🤖", "robot"),
    ("😺", "cat"),
    ("😸", "cat grin"),
    ("😻", "cat love"),
    ("🙀", "cat scream"),
    ("👋", "wave hello"),
    ("🤚", "hand"),
    ("✋", "stop hand"),
    ("🖖", "vulcan"),
    ("👌", "okay"),
    ("🤌", "pinch"),
    ("✌️", "peace"),
    ("🤞", "crossed fingers"),
    ("👍", "thumbs up yes"),
    ("👎", "thumbs down no"),
    ("👏", "clap"),
    ("🙌", "raised hands"),
    ("🫶", "heart hands"),
    ("🤝", "handshake"),
    ("🙏", "pray thanks"),
    ("💪", "strong muscle"),
    ("❤️", "red heart love"),
    ("🧡", "orange heart"),
    ("💛", "yellow heart"),
    ("💚", "green heart"),
    ("💙", "blue heart"),
    ("💜", "purple heart"),
    ("🖤", "black heart"),
    ("🤍", "white heart"),
    ("✨", "sparkles"),
    ("⭐", "star"),
    ("🌙", "moon night"),
    ("☀️", "sun"),
    ("🔥", "fire"),
    ("💯", "hundred"),
    ("🎉", "party celebrate"),
    ("🎊", "confetti"),
    ("🐈", "cat animal"),
    ("🐕", "dog animal"),
    ("🐇", "rabbit bunny"),
    ("🦊", "fox"),
    ("🐻", "bear"),
    ("🐼", "panda"),
    ("🐸", "frog"),
    ("🦋", "butterfly"),
    ("🍕", "pizza food"),
    ("🍔", "burger"),
    ("🍟", "fries"),
    ("🍜", "ramen"),
    ("🍰", "cake"),
    ("🍩", "donut"),
    ("☕", "coffee"),
    ("🧋", "boba tea"),
    ("🎮", "game controller"),
    ("🎧", "headphones music"),
    ("🎵", "music note"),
    ("🎨", "art"),
    ("📚", "books study"),
    ("💻", "computer"),
    ("✅", "check yes"),
    ("❌", "cross no"),
];
#[derive(Clone)]
pub(super) struct Reply {
    pub id: String,
    pub channel: String,
    pub name: String,
    pub text: String,
}
impl Larpcord {
    pub(super) fn reply_to_message(
        &mut self,
        message: &Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reply_to = Some(Reply {
            id: message.id.clone(),
            channel: message.channel_id.clone(),
            name: message.name().to_string(),
            text: message.content.chars().take(100).collect(),
        });
        window.focus(&self.composer.focus_handle(cx));
        cx.notify();
    }
    pub(super) fn reply_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(reply) = &self.reply_to else {
            return div().into_any_element();
        };
        row()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .rounded_t_lg()
            .bg(rgb(0x242429))
            .child(icon("reply", 16.).text_color(self.color(MUTED)))
            .child(label("Replying to"))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(12.))
                    .child(reply.name.clone()),
            )
            .child(label(reply.text.clone()).truncate().flex_1().min_w_0())
            .child(
                button("cancel-reply", "")
                    .p_1()
                    .child(icon("close", 14.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.reply_to = None;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    pub(super) fn message_tools(&self, message: &Message, cx: &mut Context<Self>) -> AnyElement {
        let reply = message.clone();
        let content = message.content.clone();
        let menu = message.clone();
        row()
            .absolute()
            .right(px(16.))
            .top_0()
            .h(px(28.))
            .rounded_md()
            .border_1()
            .border_color(self.color(LINE))
            .bg(self.color(SURFACE))
            .shadow_md()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                toolbar_button("reply-message", "reply", "Reply")
                    .size(px(26.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reply_to_message(&reply, window, cx)
                    })),
            )
            .child(
                toolbar_button("copy-message", "copy", "Copy text")
                    .size(px(26.))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(content.clone()))
                    }),
            )
            .child(
                toolbar_button("message-more", "more", "More options")
                    .size(px(26.))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_message_menu(menu.clone(), event.position, window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn open_message_menu(
        &mut self,
        message: Message,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_tab.is_some()
            || self.profile_is_modal()
            || self.video.is_some()
            || self.login_open
        {
            return;
        }
        self.close_picker();
        self.previous_focus = window.focused(cx);
        self.message_menu = Some((message, position));
        window.focus(&self.root_focus);
        cx.notify();
    }
    pub(super) fn message_menu_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((message, position)) = &self.message_menu else {
            return div().into_any_element();
        };
        let mut menu = column()
            .id("message-context-menu")
            .debug_selector(|| "message-context-menu".into())
            .w(px(220.))
            .p_1()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(self.color(LINE))
            .bg(self.color(PANEL))
            .shadow_lg()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());
        for (index, name, glyph) in [
            (0, "Reply", "reply"),
            (1, "Copy Text", "copy"),
            (2, "Copy Message Link", "external-link"),
            (3, "Copy Message ID", "hash"),
            (4, "View Profile", "user"),
        ] {
            let message = message.clone();
            let guild = self.selected_guild.clone().unwrap_or_else(|| "@me".into());
            menu = menu.child(
                row()
                    .id(("message-action", index as usize))
                    .debug_selector(move || format!("message-action-{index}"))
                    .h(px(34.))
                    .px_2()
                    .gap_3()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x5865f2)).text_color(rgb(0xffffff)))
                    .child(div().flex_1().text_size(px(13.)).child(name))
                    .child(icon(glyph, 17.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.message_menu = None;
                        if let Some(focus) = this.previous_focus.take() {
                            window.focus(&focus);
                        }
                        match index {
                            0 => this.reply_to_message(&message, window, cx),
                            1 => cx.write_to_clipboard(ClipboardItem::new_string(
                                message.content.clone(),
                            )),
                            2 => cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                "https://discord.com/channels/{guild}/{}/{}",
                                message.channel_id, message.id
                            ))),
                            3 => {
                                cx.write_to_clipboard(ClipboardItem::new_string(message.id.clone()))
                            }
                            _ => this.open_profile(message.author.clone(), window, cx),
                        };
                        cx.notify();
                    })),
            );
        }
        div()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.message_menu = None;
                    if let Some(focus) = this.previous_focus.take() {
                        window.focus(&focus);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.message_menu = None;
                    cx.notify();
                }),
            )
            .child(anchored().position(*position).snap_to_window().child(menu))
            .into_any_element()
    }
}
