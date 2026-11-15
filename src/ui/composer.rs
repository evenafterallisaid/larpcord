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
            .child(icon("reply", 16.).text_color(rgb(MUTED)))
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
        let channel = message.channel_id.clone();
        let id = message.id.clone();
        let guild = self.selected_guild.clone().unwrap_or_else(|| "@me".into());
        row()
            .absolute()
            .right(px(16.))
            .top(px(-12.))
            .gap_1()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0x232428))
            .shadow_sm()
            .child(
                button("reply-message", "")
                    .p_1()
                    .child(icon("reply", 18.))
                    .tooltip(|_, cx| cx.new(|_| Hint("Reply".into())).into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reply_to_message(&reply, window, cx)
                    })),
            )
            .child(
                button("copy-message", "")
                    .p_1()
                    .child(icon("copy", 18.))
                    .tooltip(|_, cx| cx.new(|_| Hint("Copy text".into())).into())
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(content.clone()))
                    }),
            )
            .child(
                button("copy-message-link", "")
                    .p_1()
                    .child(icon("external-link", 18.))
                    .tooltip(|_, cx| cx.new(|_| Hint("Copy message link".into())).into())
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(format!(
                            "https://discord.com/channels/{guild}/{channel}/{id}"
                        )))
                    }),
            )
            .into_any_element()
    }
}
