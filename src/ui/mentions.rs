use super::*;
use crate::content::{Mention, mention_ranges};
use std::ops::Range;

struct Choice {
    label: String,
    token: String,
    emoji: bool,
    image_url: Option<String>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct MentionScope<'a> {
    pub channel: Option<&'a str>,
    pub users: &'a [User],
    pub channels: &'a [Channel],
}

impl<'a> From<&'a Message> for MentionScope<'a> {
    fn from(message: &'a Message) -> Self {
        Self {
            channel: Some(&message.channel_id),
            users: &message.mentions,
            channels: &message.mention_channels,
        }
    }
}

#[derive(Clone)]
pub(super) enum TextLink {
    Url(String),
    User(Box<User>),
    Channel(String),
}

impl TextLink {
    pub fn open(&self, view: Option<&WeakEntity<Larpcord>>, window: &mut Window, cx: &mut App) {
        if let Self::Url(url) = self {
            open_https(url, cx);
        } else if let Some(view) = view.and_then(WeakEntity::upgrade) {
            view.update(cx, |this, cx| match self {
                Self::User(user) => this.open_profile((**user).clone(), window, cx),
                Self::Channel(id) => this.choose_channel(id.clone(), window, cx),
                Self::Url(_) => unreachable!(),
            });
        }
    }
}

impl Larpcord {
    pub(super) fn draft_mention_labels(&self, cx: &App) -> HashMap<String, String> {
        let draft = &self.composer.read(cx).content;
        let scope = MentionScope {
            channel: self.selected_channel.as_deref(),
            ..MentionScope::default()
        };
        mention_ranges(draft)
            .into_iter()
            .map(|(range, mention)| {
                (
                    draft[range].to_string(),
                    self.resolve_mention(scope, mention).0,
                )
            })
            .collect()
    }

    fn mention_choices(&self, cx: &App) -> Option<(Range<usize>, Vec<Choice>)> {
        let channel = self
            .attachment_channel()
            .and_then(|id| self.store.channels.get(&id))?;
        if self.mentions_hidden || self.emoji_open || self.reaction_target.is_some() {
            return None;
        }
        if let Some((range, choices)) = self.emoji_completions(cx) {
            return Some((
                range,
                choices
                    .into_iter()
                    .map(|choice| Choice {
                        label: format!(":{}:", choice.name),
                        token: choice.token,
                        emoji: true,
                        image_url: choice.image_url,
                    })
                    .collect(),
            ));
        }
        let input = self.composer.read(cx);
        let end = input.mention_cursor()?;
        let prefix = &input.content[..end];
        let start = prefix
            .rfind(char::is_whitespace)
            .map_or(0, |i| i + prefix[i..].chars().next().unwrap().len_utf8());
        let query = prefix[start..]
            .strip_prefix('@')
            .or_else(|| prefix[start..].strip_prefix('#'))?;
        if query.len() > 50 || query.contains(['<', '>', '`', '@', '#', '/', '\\']) {
            return None;
        }
        let channels = prefix[start..].starts_with('#');
        let query = query.to_lowercase();
        let mut choices = Vec::<Choice>::new();
        let mut offer = |label: String, token: String, alternative: &str| {
            if choices.len() < 7
                && (label
                    .trim_start_matches(['@', '#'])
                    .to_lowercase()
                    .contains(&query)
                    || (!alternative.is_empty() && alternative.to_lowercase().contains(&query)))
                && !choices.iter().any(|c| c.token == token)
            {
                choices.push(Choice {
                    label,
                    token,
                    emoji: false,
                    image_url: None,
                });
            }
        };
        let guild = channel
            .guild_id
            .as_ref()
            .and_then(|id| self.store.guilds.iter().find(|g| &g.id == id));
        if channels {
            let mut items = self
                .store
                .channels
                .values()
                .filter(|c| c.guild_id.is_some() && c.guild_id == channel.guild_id && c.readable())
                .collect::<Vec<_>>();
            items.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
            for c in items {
                offer(format!("#{}", c.label()), format!("<#{}>", c.id), "");
            }
        } else {
            if start == 0 && !query.is_empty() && "silent".starts_with(&query) {
                offer(
                    "@silent · Send without a desktop alert".into(),
                    "@silent".into(),
                    "",
                );
            }
            if guild.is_some() && !query.is_empty() {
                offer("@everyone".into(), "@everyone".into(), "");
                offer("@here".into(), "@here".into(), "");
            }
            for u in &channel.recipients {
                offer(
                    format!("@{}", u.name()),
                    format!("<@{}>", u.id),
                    &u.username,
                );
            }
            for m in self.store.channel_messages(&channel.id).iter().rev() {
                offer(
                    format!("@{}", m.name()),
                    format!("<@{}>", m.author.id),
                    &m.author.username,
                );
            }
            if let Some(guild) = guild {
                for row in self.members.rows.values() {
                    if let crate::members::Row::Member(member, _, _) = row
                        && let Some(u) = &member.user
                    {
                        offer(
                            format!("@{}", member.nick.as_deref().unwrap_or_else(|| u.name())),
                            format!("<@{}>", u.id),
                            &u.username,
                        );
                    }
                }
                for role in guild.roles.iter().filter(|r| r.id != guild.id) {
                    offer(format!("@{}", role.name), format!("<@&{}>", role.id), "");
                }
            }
        }
        (!choices.is_empty()).then_some((start..end, choices))
    }

    pub(super) fn select_mention(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some((range, choices)) = self.mention_choices(cx) else {
            return false;
        };
        let Some(choice) = choices.get(index.min(choices.len() - 1)) else {
            return false;
        };
        let token = if choice.emoji {
            choice.token.clone()
        } else {
            format!("{} ", choice.token)
        };
        self.composer
            .update(cx, |input, cx| input.replace_draft_range(range, &token, cx));
        if choice.emoji {
            self.remember_emoji(token, cx);
        }
        self.mention_choice = 0;
        cx.notify();
        true
    }

    pub(super) fn hide_mention_picker(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.composer.focus_handle(cx).is_focused(window) && self.mention_choices(cx).is_some() {
            self.mentions_hidden = true;
            return true;
        }
        false
    }

    pub(super) fn mention_keyboard(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.composer.focus_handle(cx).is_focused(window)
            || event.keystroke.modifiers.modified()
        {
            return;
        }
        match event.keystroke.key.as_str() {
            "up" | "down" => {
                if let Some((_, choices)) = self.mention_choices(cx) {
                    let change = if event.keystroke.key == "up" {
                        choices.len() - 1
                    } else {
                        1
                    };
                    self.mention_choice = (self.mention_choice + change) % choices.len();
                    cx.notify();
                    cx.stop_propagation();
                }
            }
            "tab" if self.select_mention(self.mention_choice, cx) => cx.stop_propagation(),
            _ => {}
        }
    }

    pub(super) fn mention_picker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, choices) = self.mention_choices(cx)?;
        let selected = self.mention_choice.min(choices.len() - 1);
        Some(
            column()
                .id("mention-completions")
                .debug_selector(|| "mention-completions".into())
                .border_1()
                .border_color(self.color(LINE))
                .rounded_lg()
                .p_1()
                .gap_1()
                .bg(self.color(PANEL))
                .children(choices.into_iter().enumerate().map(|(index, choice)| {
                    row()
                        .id(("mention-choice", index))
                        .rounded_md()
                        .px_3()
                        .h(px(30.))
                        .gap_2()
                        .debug_selector(move || format!("completion-choice-{index}"))
                        .cursor_pointer()
                        .when(index == selected, |s| s.bg(self.color(SURFACE)))
                        .hover(|s| s.bg(self.color(SURFACE)))
                        .when(choice.emoji, |s| {
                            s.child(row().size(px(22.)).justify_center().child(
                                if let Some(url) = &choice.image_url {
                                    self.media
                                        .get(url)
                                        .map(|image| {
                                            img(image)
                                                .size(px(19.))
                                                .object_fit(ObjectFit::Contain)
                                                .into_any_element()
                                        })
                                        .unwrap_or_else(|| icon("smile", 17.).into_any_element())
                                } else {
                                    div()
                                        .text_size(px(18.))
                                        .child(choice.token)
                                        .into_any_element()
                                },
                            ))
                        })
                        .child(div().truncate().child(choice.label))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_mention(index, cx);
                            window.focus(&this.composer.focus_handle(cx));
                        }))
                }))
                .into_any_element(),
        )
    }

    pub(super) fn resolve_mention(
        &self,
        scope: MentionScope<'_>,
        mention: Mention<'_>,
    ) -> (String, Option<TextLink>) {
        let channel = scope.channel.and_then(|id| self.store.channels.get(id));
        let guild = channel.and_then(|c| c.guild_id.as_deref());
        match mention {
            Mention::User(id) => {
                let member = (guild.is_some() && guild == self.members.guild.as_deref())
                    .then(|| {
                        self.members.rows.values().find_map(|row| match row {
                            crate::members::Row::Member(member, user, _) if user == id => {
                                Some(member.as_ref())
                            }
                            _ => None,
                        })
                    })
                    .flatten();
                let user = scope
                    .users
                    .iter()
                    .find(|user| user.id == id)
                    .or_else(|| (self.store.user.id == id).then_some(&self.store.user))
                    .or_else(|| {
                        channel.and_then(|c| c.recipients.iter().find(|user| user.id == id))
                    })
                    .or_else(|| self.store.users.get(id))
                    .or_else(|| member.and_then(|m| m.user.as_ref()));
                let label = user
                    .and_then(|u| u.member.as_ref().and_then(|m| m.nick.as_deref()))
                    .or_else(|| member.and_then(|m| m.nick.as_deref()))
                    .or_else(|| user.map(User::name).filter(|n| !n.is_empty()))
                    .unwrap_or("unknown-user");
                (
                    format!("@{label}"),
                    user.map(|user| TextLink::User(Box::new(user.clone()))),
                )
            }
            Mention::Role(id) => {
                let name = guild
                    .and_then(|id| self.store.guilds.iter().find(|g| g.id == id))
                    .and_then(|g| g.roles.iter().find(|r| r.id == id))
                    .map_or("unknown-role", |r| r.name.as_str());
                (format!("@{name}"), None)
            }
            Mention::Channel(id) => {
                let known = self.store.channels.get(id);
                let label = known
                    .or_else(|| scope.channels.iter().find(|c| c.id == id))
                    .map_or_else(|| "unknown-channel".into(), Channel::label);
                (
                    format!("#{label}"),
                    known.map(|_| TextLink::Channel(id.to_string())),
                )
            }
            Mention::Everyone => ("@everyone".into(), None),
            Mention::Here => ("@here".into(), None),
        }
    }

    pub(super) fn mention_excerpt(&self, scope: MentionScope<'_>, text: &str) -> String {
        crate::content::markdown(text)
            .first()
            .map_or_else(String::new, |block| {
                let mut output = String::new();
                for span in &block.spans {
                    let refs = if span.code || span.link.is_some() {
                        Vec::new()
                    } else {
                        mention_ranges(&span.text)
                    };
                    let mut start = 0;
                    for (range, mention) in refs {
                        output.push_str(&span.text[start..range.start]);
                        output.push_str(&self.resolve_mention(scope, mention).0);
                        start = range.end;
                    }
                    output.push_str(&span.text[start..]);
                }
                output
            })
    }

    pub(super) fn message_excerpt(&self, message: &Message, text: &str) -> String {
        self.mention_excerpt(message.into(), text)
    }
}
