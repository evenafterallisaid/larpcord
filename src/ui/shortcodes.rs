use super::*;
use crate::expressions::{unicode_alias, unicode_aliases};
use std::ops::Range;

pub(super) struct Suggestion {
    pub name: String,
    pub token: String,
    pub image_url: Option<String>,
}

struct Shortcode<'a> {
    range: Range<usize>,
    query: &'a str,
}

fn alias_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+')
}

fn shortcode(text: &str, cursor: usize, complete: bool) -> Option<Shortcode<'_>> {
    let prefix = text.get(..cursor)?;
    let query_end = if complete {
        prefix.strip_suffix(':')?.len()
    } else {
        cursor
    };
    let len = text[..query_end]
        .bytes()
        .rev()
        .take_while(|b| alias_char(*b))
        .count();
    if !(1..=64).contains(&len) {
        return None;
    }
    let start = query_end.checked_sub(len + 1)?;
    if text.as_bytes()[start] != b':'
        || text[..start].chars().next_back().is_some_and(|ch| {
            !(ch.is_whitespace()
                || matches!(
                    ch,
                    '(' | '[' | '{' | ')' | ']' | '}' | '"' | '\'' | ',' | '.' | '!' | '?'
                )
                || (!ch.is_ascii() && !ch.is_alphanumeric()))
        })
    {
        return None;
    }
    // Leave inline code and fenced code verbatim.
    let mut fence = 0;
    let mut context = text.as_bytes()[..start].iter().copied().peekable();
    while let Some(byte) = context.next() {
        if byte == b'\\' {
            context.next();
        } else if byte == b'`' {
            let mut run = 1;
            while context.next_if_eq(&b'`').is_some() {
                run += 1;
            }
            if fence == 0 {
                fence = run;
            } else if run == fence {
                fence = 0;
            }
        }
    }
    if fence != 0 {
        return None;
    }
    let end = if complete {
        cursor
    } else {
        let suffix = &text[cursor..];
        let rest = suffix.bytes().take_while(|byte| alias_char(*byte)).count();
        cursor + rest + usize::from(suffix.as_bytes().get(rest) == Some(&b':'))
    };
    Some(Shortcode {
        range: start..end,
        query: &text[start + 1..query_end],
    })
}

fn match_grade(name: &str, query: &str) -> Option<u8> {
    if name == query {
        Some(0)
    } else if name.starts_with(query) {
        Some(1)
    } else if name.split(['_', '-']).any(|word| word.starts_with(query)) {
        Some(2)
    } else if name.contains(query) {
        Some(3)
    } else {
        None
    }
}

impl Larpcord {
    pub(super) fn sync_shortcode_history(&mut self, cx: &Context<Self>) {
        let input = self.composer.read(cx);
        if let Some(cursor) = input.mention_cursor()
            && (shortcode(&input.content, cursor, false).is_some()
                || shortcode(&input.content, cursor, true).is_some())
        {
            self.load_emoji_history();
        }
    }

    fn shortcode_suggestions(&self, query: &str, complete: bool) -> Vec<Suggestion> {
        let query = query.to_ascii_lowercase();
        let mut history = HashMap::new();
        for (rank, key) in self
            .picker
            .session_recent
            .iter()
            .chain(&self.picker.recent)
            .chain(&self.picker.account_recent)
            .chain(&self.picker.reaction_recent)
            .enumerate()
        {
            let normalized = match crate::content::parts(key).as_slice() {
                [crate::content::Part::Emoji { id, .. }] => (*id).to_owned(),
                _ => unicode_alias(key).unwrap_or(key).to_owned(),
            };
            history.entry(normalized).or_insert(rank);
        }
        let mut candidates = Vec::new();
        let mut offer = |name: &str,
                         lower: &str,
                         token: &str,
                         emoji: Option<&crate::expressions::Emoji>,
                         local: bool| {
            let Some(grade) = match_grade(lower, &query) else {
                return;
            };
            let usage = history
                .get(emoji.map_or(token, |emoji| emoji.id.as_str()))
                .copied();
            if complete
                && grade != 0
                && !(usage.is_some() && lower.split(['_', '-']).any(|word| word == query))
            {
                return;
            }
            candidates.push((
                (usage.unwrap_or(usize::MAX), grade, !local, name.len()),
                Suggestion {
                    name: name.to_string(),
                    token: token.to_string(),
                    image_url: emoji.map(|emoji| emoji.image(false)),
                },
            ));
        };
        for (name, unicode) in unicode_aliases() {
            offer(name, name, unicode, None, true);
        }
        for (guild, emoji) in self
            .store
            .guilds
            .iter()
            .flat_map(|guild| {
                guild
                    .emojis
                    .iter()
                    .flatten()
                    .map(move |emoji| (guild, emoji))
            })
            .take(4096)
        {
            let lower = emoji.name.to_ascii_lowercase();
            if match_grade(&lower, &query).is_some()
                && emoji.valid()
                && self.emoji_lock_reason(guild, emoji).is_none()
            {
                offer(
                    &emoji.name,
                    &lower,
                    &emoji.markup(),
                    Some(emoji),
                    self.selected_guild.as_ref() == Some(&guild.id),
                );
            }
        }
        // Only auto-expand complete names. History may disambiguate a valid name,
        // but a mistyped or wholly unknown name should remain as typed.
        if complete && !candidates.iter().any(|((_, grade, _, _), _)| *grade == 0) {
            return vec![];
        }
        candidates.sort_by_key(|(rank, _)| *rank);
        let mut seen = HashSet::new();
        candidates
            .into_iter()
            .map(|(_, choice)| choice)
            .filter(|choice| seen.insert(choice.token.clone()))
            .take(7)
            .collect()
    }

    pub(super) fn emoji_completions(&self, cx: &App) -> Option<(Range<usize>, Vec<Suggestion>)> {
        let input = self.composer.read(cx);
        let code = shortcode(&input.content, input.mention_cursor()?, false)?;
        let choices = self.shortcode_suggestions(code.query, false);
        (!choices.is_empty()).then_some((code.range, choices))
    }

    pub(super) fn expand_shortcode(&mut self, cx: &mut Context<Self>) {
        if self.attachment_channel().is_none() {
            return;
        }
        let input = self.composer.read(cx);
        let Some(code) = input
            .mention_cursor()
            .and_then(|cursor| shortcode(&input.content, cursor, true))
        else {
            return;
        };
        let Some(choice) = self
            .shortcode_suggestions(code.query, true)
            .into_iter()
            .next()
        else {
            return;
        };
        let range = code.range;
        self.composer.update(cx, |input, cx| {
            input.replace_draft_range(range, &choice.token, cx)
        });
        self.remember_emoji(choice.token, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Larpcord, shortcode};
    use gpui::TestAppContext;

    #[test]
    fn shortcode_does_not_parse_links_markup_or_code() {
        for text in [
            "https://host/:sob:",
            "<:sob:123>",
            "`:sob:`",
            "```\n:sob:",
            "\\:sob:",
        ] {
            assert!(shortcode(text, text.len(), true).is_none(), "{text}");
            assert!(shortcode(text, text.len() - 1, false).is_none(), "{text}");
        }
        let text = "say :sob: please";
        let editable = shortcode(text, 7, false).unwrap();
        assert_eq!(editable.query, "so");
        assert_eq!(&text[editable.range], ":sob:");
    }

    #[gpui::test]
    fn shortcode_completion_uses_synced_history_and_learns_a_new_choice(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            assert_eq!(v.shortcode_suggestions("rose", true)[0].token, "🌹");
            v.picker.account_recent = vec!["wilted_flower".into()];
            v.composer.update(cx, |input, cx| input.set(":ros", cx));
            let (_, choices) = v.emoji_completions(cx).unwrap();
            assert_eq!(choices[0].token, "🥀");
            let regular = choices
                .iter()
                .position(|choice| choice.token == "🌹")
                .unwrap();
            assert!(v.select_mention(regular, cx));
            assert_eq!(v.composer.read(cx).content.as_ref(), "🌹");
            assert_eq!(v.shortcode_suggestions("rose", true)[0].token, "🌹");
            assert_eq!(v.settings.recent_emoji[0], "🌹");
        });
    }

    #[gpui::test]
    fn typing_a_closed_shortcode_replaces_it_before_sending(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| Larpcord::with_preview(window, cx, true));
        view.update(cx, |v, cx| {
            v.composer.update(cx, |input, cx| input.set("oh :sob", cx));
            v.composer.update(cx, |input, cx| input.insert(":", cx));
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let v = view.read(cx);
            assert_eq!(v.composer.read(cx).content.as_ref(), "oh 😭");
            assert_eq!(v.settings.recent_emoji[0], "😭");
        });
    }
}
