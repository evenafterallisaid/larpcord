#[derive(Debug, PartialEq)]
pub enum Part<'a> {
    Text(&'a str),
    Emoji { name: &'a str, id: &'a str },
}

pub fn silent_content(text: &str) -> (&str, bool) {
    if let Some(rest) = text.strip_prefix("@silent")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        (rest.trim_start(), true)
    } else {
        (text, false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mention<'a> {
    User(&'a str),
    Role(&'a str),
    Channel(&'a str),
    Everyone,
    Here,
}

pub fn mention_ranges(text: &str) -> Vec<(std::ops::Range<usize>, Mention<'_>)> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut result = Vec::new();
    let mut end_of_last = 0;
    for (start, ch) in text.match_indices(['<', '@']) {
        if start < end_of_last
            || text[..start]
                .bytes()
                .rev()
                .take_while(|b| *b == b'\\')
                .count()
                % 2
                != 0
        {
            continue;
        }
        let tail = &text[start..];
        let parsed = if ch == "<" {
            tail.find('>')
                .filter(|end| (3..=23).contains(end))
                .and_then(|end| {
                    let value = &tail[1..end];
                    let (id, mention) = if let Some(id) = value.strip_prefix("@&") {
                        (id, Mention::Role(id))
                    } else if let Some(id) =
                        value.strip_prefix("@!").or_else(|| value.strip_prefix('@'))
                    {
                        (id, Mention::User(id))
                    } else {
                        let id = value.strip_prefix('#')?;
                        (id, Mention::Channel(id))
                    };
                    (!id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit()))
                        .then_some((end + 1, mention))
                })
        } else if text[..start].chars().next_back().is_some_and(word) {
            None
        } else {
            [("@everyone", Mention::Everyone), ("@here", Mention::Here)]
                .into_iter()
                .find_map(|(prefix, mention)| {
                    tail.strip_prefix(prefix)
                        .filter(|rest| !rest.starts_with(word))
                        .map(|_| (prefix.len(), mention))
                })
        };
        if let Some((length, mention)) = parsed {
            end_of_last = start + length;
            result.push((start..end_of_last, mention));
        }
    }
    result
}

pub fn parts(text: &str) -> Vec<Part<'_>> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut scan = 0;
    while let Some(offset) = text[scan..].find('<') {
        let begin = scan + offset;
        let Some(end) = text[begin..].find('>').map(|i| i + begin) else {
            break;
        };
        let value = &text[begin + 1..end];
        let body = value.strip_prefix(':').or_else(|| value.strip_prefix("a:"));
        if let Some((name, id)) = body.and_then(|s| s.split_once(':'))
            && !name.is_empty()
            && name.len() <= 64
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && (1..=20).contains(&id.len())
            && id.bytes().all(|b| b.is_ascii_digit())
        {
            if start < begin {
                result.push(Part::Text(&text[start..begin]));
            }
            result.push(Part::Emoji { name, id });
            start = end + 1;
        }
        scan = end + 1;
        if scan >= text.len() {
            break;
        }
    }
    if start < text.len() {
        result.push(Part::Text(&text[start..]));
    }
    result
}

#[derive(Clone, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}
#[derive(Default)]
pub struct Block {
    pub spans: Vec<Span>,
    pub heading: u8,
    pub code: bool,
    pub quote: bool,
}

pub fn markdown(text: &str) -> Vec<Block> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Span::default();
    let mut quote = 0;
    fn flush(block: &mut Block, out: &mut Vec<Block>, quote: usize) {
        if !block.spans.is_empty() {
            out.push(std::mem::take(block));
        }
        block.quote = quote > 0;
    }
    for event in
        pulldown_cmark::TextMergeStream::new(Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH))
    {
        match event {
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut block, &mut blocks, quote);
                block.heading = level as u8;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut block, &mut blocks, quote);
                block.code = true;
                style.code = true;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                quote += 1;
                block.quote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                quote = quote.saturating_sub(1);
                flush(&mut block, &mut blocks, quote);
            }
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => style.link = None,
            Event::Start(Tag::Item) => {
                flush(&mut block, &mut blocks, quote);
                block.spans.push(Span {
                    text: "• ".into(),
                    ..Span::default()
                });
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item) => {
                flush(&mut block, &mut blocks, quote)
            }
            Event::End(TagEnd::CodeBlock) => {
                flush(&mut block, &mut blocks, quote);
                style.code = false;
            }
            Event::Text(value) | Event::Html(value) | Event::InlineHtml(value) => {
                for (index, line) in value.split('\n').enumerate() {
                    if index > 0 {
                        let code = block.code;
                        let heading = block.heading;
                        flush(&mut block, &mut blocks, quote);
                        block.code = code;
                        block.heading = heading;
                    }
                    if !line.is_empty() {
                        block.spans.push(Span {
                            text: line.to_string(),
                            ..style.clone()
                        });
                    }
                }
            }
            Event::Code(value) => block.spans.push(Span {
                text: value.to_string(),
                code: true,
                ..style.clone()
            }),
            Event::SoftBreak | Event::HardBreak => flush(&mut block, &mut blocks, quote),
            Event::Rule => {
                flush(&mut block, &mut blocks, quote);
                block.spans.push(Span {
                    text: "────────".into(),
                    ..Span::default()
                });
                flush(&mut block, &mut blocks, quote);
            }
            _ => {}
        }
    }
    flush(&mut block, &mut blocks, quote);
    blocks
}

pub fn emoji_only(text: &str) -> bool {
    use unicode_segmentation::UnicodeSegmentation;
    let parts = parts(text);
    let mut count = 0;
    for part in parts {
        match part {
            Part::Emoji { .. } => count += 1,
            Part::Text(text) => {
                for grapheme in text.graphemes(true).filter(|g| !g.trim().is_empty()) {
                    if !grapheme
                        .chars()
                        .any(|c| matches!(c as u32,0x1f000..=0x1faff|0x2600..=0x27bf))
                        || !grapheme.chars().all(
                            |c| matches!(c as u32,0x1f000..=0x1faff|0x2600..=0x27bf|0xfe0f|0x200d),
                        )
                    {
                        return false;
                    }
                    count += 1;
                }
            }
        }
        if count > 3 {
            return false;
        }
    }
    (1..=3).contains(&count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_mention_tokens_without_mistaking_an_email_or_word_for_a_ping() {
        let source = "<@42> <@!109> <@&12> <#123> @here @everyone hola@here @everyone_else \\<@5>";
        let tokens = mention_ranges(source)
            .into_iter()
            .map(|(range, kind)| (&source[range], kind))
            .collect::<Vec<_>>();
        assert_eq!(
            tokens,
            vec![
                ("<@42>", Mention::User("42")),
                ("<@!109>", Mention::User("109")),
                ("<@&12>", Mention::Role("12")),
                ("<#123>", Mention::Channel("123")),
                ("@here", Mention::Here),
                ("@everyone", Mention::Everyone)
            ]
        );
        assert_eq!(silent_content("@silent\t<@42>"), ("<@42>", true));
        assert_eq!(silent_content("@silently test"), ("@silently test", false));
    }

    #[test]
    fn standalone_emoji_get_room_without_enlarging_ordinary_messages() {
        assert!(emoji_only("😭"));
        assert!(emoji_only("❤️ 🫶"));
        assert!(emoji_only("<:hello:123>"));
        assert!(!emoji_only("hello 😭"));
        assert!(!emoji_only("😀 😀 😀 😀"));
        assert!(!emoji_only(""));
        assert!(!emoji_only("123"));
    }
    #[test]
    fn emoji_tokens_preserve_text_and_leave_invalid_markup_alone() {
        assert_eq!(
            parts("hi <:wave:123> <a:dance:456>!"),
            vec![
                Part::Text("hi "),
                Part::Emoji {
                    name: "wave",
                    id: "123"
                },
                Part::Text(" "),
                Part::Emoji {
                    name: "dance",
                    id: "456"
                },
                Part::Text("!")
            ]
        );
        let literal = "🙂 <@123> <:bad:abc> <https://test>";
        assert_eq!(parts(literal), vec![Part::Text(literal)]);
    }
    #[test]
    fn markdown_keeps_help_headings_emphasis_links_and_emoji_tokens() {
        let blocks =
            markdown("# help\n**commands** [docs](https://example.test) <:wave:123>\n`code`");
        assert_eq!(blocks[0].heading, 1);
        assert!(
            blocks[1]
                .spans
                .iter()
                .any(|s| s.bold && s.text == "commands")
        );
        assert!(
            blocks[1]
                .spans
                .iter()
                .any(|s| s.link.as_deref() == Some("https://example.test"))
        );
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.spans)
                .any(|s| s.text.contains("<:wave:123>"))
        );
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.spans)
                .any(|s| s.code && s.text == "code")
        );
    }
}
