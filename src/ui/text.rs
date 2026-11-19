use super::*;
use mentions::{MentionScope, TextLink};

impl Larpcord {
    fn text_pieces<'a>(
        &self,
        scope: MentionScope<'_>,
        span: &crate::content::Span,
        text: &'a str,
    ) -> Vec<(std::borrow::Cow<'a, str>, bool, Option<TextLink>)> {
        let mut pieces = Vec::new();
        let mut start = 0;
        if !span.code && span.link.is_none() {
            for (range, mention) in crate::content::mention_ranges(text) {
                if start < range.start {
                    pieces.push((text[start..range.start].into(), false, None));
                }
                let (label, link) = self.resolve_mention(scope, mention);
                pieces.push((label.into(), true, link));
                start = range.end;
            }
        }
        if start < text.len() {
            pieces.push((
                text[start..].into(),
                false,
                span.link.clone().map(TextLink::Url),
            ));
        }
        pieces
    }
}

impl Larpcord {
    pub(super) fn rich_text(&self, text: &str) -> AnyElement {
        self.rich_text_at(text, 14., MentionScope::default(), None)
    }
    pub(super) fn rich_message_text(
        &self,
        message: &Message,
        text: &str,
        font_size: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.rich_text_at(text, font_size, message.into(), Some(cx.weak_entity()))
    }
    fn rich_text_at(
        &self,
        text: &str,
        font_size: f32,
        scope: MentionScope<'_>,
        view: Option<WeakEntity<Self>>,
    ) -> AnyElement {
        let jumbo = font_size >= 16. && crate::content::emoji_only(text);
        let emoji_size = if jumbo { 32. } else { font_size + 2. };
        let blocks = crate::content::markdown(text);
        column().debug_selector(|| "rich-text".into()).w_full().min_w_0().gap_1()
            .children(blocks.into_iter().enumerate().map(|(index, block)| {
                let has_emoji = block.spans.iter().any(|span| !span.code && crate::content::parts(&span.text).iter().any(|p| matches!(p,crate::content::Part::Emoji {..})));
                let mut items = Vec::<AnyElement>::new();
                if !has_emoji {
                    // let gpui shape a whole paragraph instead of laying out a node per word.
                    let mut text = String::new();
                    let mut highlights = Vec::new();
                    let mut ranges = Vec::new();
                    let mut urls = Vec::new();
                    for span in block.spans {
                        for (piece, mention, link) in self.text_pieces(scope, &span, &span.text) {
                            let start = text.len();
                            text.push_str(&piece);
                            let range = start..text.len();
                            highlights.push((range.clone(), HighlightStyle {
                                font_weight:span.bold.then_some(FontWeight::BOLD),
                                font_style:span.italic.then_some(FontStyle::Italic),
                                background_color:if mention { Some(rgba(0x858aff33).into()) } else { span.code.then(|| self.color(PANEL).into()) },
                                color:if mention { Some(rgb(0xc0c4ff).into()) } else { link.as_ref().map(|_| rgb(0x65b8f7).into()) },
                                strikethrough:span.strike.then_some(StrikethroughStyle {thickness:px(1.),color:None}),
                                ..HighlightStyle::default()
                            }));
                            if let Some(link)=link { ranges.push(range); urls.push(link); }
                        }
                    }
                    let text=StyledText::new(text).with_highlights(highlights);
                    let view = view.clone();
                    let element=if ranges.is_empty() {text.into_any_element()} else {
                        InteractiveText::new(("paragraph-links",index),text).on_click(ranges,move |index,window,cx| {
                            if let Some(link)=urls.get(index) {link.open(view.as_ref(), window, cx);}
                        }).into_any_element()
                    };
                    items.push(div().w_full().min_w_0().child(element).into_any_element());
                } else {
                    for span in block.spans {
                        let parts=if span.code {vec![crate::content::Part::Text(&span.text)]} else {crate::content::parts(&span.text)};
                        for part in parts {
                            match part {
                                crate::content::Part::Text(text) => {
                                    for (piece, mention, link) in self.text_pieces(scope, &span, text) {
                                        let words: Vec<&str> = if mention { vec![piece.as_ref()] } else { piece.split_inclusive(char::is_whitespace).collect() };
                                        for word in words {
                                            let styled=div().max_w(px(360.)).flex_shrink_0().when(span.bold,|s|s.font_weight(FontWeight::BOLD)).when(span.italic,|s|s.italic()).when(span.strike,|s|s.line_through())
                                                .when(span.code,|s|s.font_family("monospace").bg(self.color(PANEL)).rounded_sm().px_1())
                                                .when(mention, |s| s.rounded_sm().bg(rgba(0x858aff33)).text_color(rgb(0xc0c4ff))).child(word.to_string());
                                            if let Some(link)=link.clone() {
                                                let view = view.clone();
                                                items.push(styled.id(("text-link",items.len())).cursor_pointer().when(!mention, |s| s.text_color(rgb(0x65b8f7))).on_click(move |_,window,cx|link.open(view.as_ref(),window,cx)).into_any_element());
                                            } else {
                                                items.push(styled.into_any_element());
                                            }
                                        }
                                    }
                                },
                                crate::content::Part::Emoji {name,id} => {
                                    let image=self.media.get(&format!("https://cdn.discordapp.com/emojis/{id}.webp?size=64&quality=lossless"));
                                    items.push(div().size(px(emoji_size)).flex_shrink_0().overflow_hidden().map(|s|match image {Some(image)=>s.child(img(image).size_full().object_fit(ObjectFit::Contain)),None=>s.child(label(format!(":{name}:"))).text_size(px(8.))}).into_any_element());
                                }
                            }
                        }
                    }
                }
                div().id(("rich-block",index)).w_full().min_w_0().min_h(px(22.))
                    .when(has_emoji, |s| s.flex().items_center().flex_wrap())
                    .text_size(px(if block.heading>0 {22.-(block.heading.min(4)-1) as f32*2.}else if jumbo {30.} else{font_size})).line_height(px(if block.heading>0{28.}else if jumbo {36.} else{22.}))
                    .when(block.heading>0,|s|s.font_weight(FontWeight::BOLD).mt_1())
                    .when(block.code,|s|s.font_family("monospace").p_2().rounded_md().bg(self.color(PANEL)))
                    .when(block.quote,|s|s.pl_3().border_l_2().border_color(self.color(MUTED))).children(items)
            })).into_any_element()
    }
}
