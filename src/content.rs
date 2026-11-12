#[derive(Debug, PartialEq)]
pub enum Part<'a> {
    Text(&'a str),
    Emoji { name: &'a str, id: &'a str },
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
