use super::*;

impl Larpcord {
    pub(super) fn rich_text(&self, text: &str) -> AnyElement {
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
                        let start = text.len();
                        text.push_str(&span.text);
                        let range = start..text.len();
                        highlights.push((range.clone(), HighlightStyle {
                            font_weight:span.bold.then_some(FontWeight::BOLD),
                            font_style:span.italic.then_some(FontStyle::Italic),
                            background_color:span.code.then(|| rgb(PANEL).into()),
                            color:span.link.as_ref().map(|_| rgb(0x65b8f7).into()),
                            strikethrough:span.strike.then_some(StrikethroughStyle {thickness:px(1.),color:None}),
                            ..HighlightStyle::default()
                        }));
                        if let Some(url)=span.link {ranges.push(range);urls.push(url);}
                    }
                    let text=StyledText::new(text).with_highlights(highlights);
                    let element=if ranges.is_empty() {text.into_any_element()} else {
                        InteractiveText::new(("paragraph-links",index),text).on_click(ranges,move |index,_,cx| {
                            if let Some(url)=urls.get(index) {open_https(url,cx);}
                        }).into_any_element()
                    };
                    items.push(div().w_full().min_w_0().child(element).into_any_element());
                } else {
                    for span in block.spans {
                        let parts=if span.code {vec![crate::content::Part::Text(&span.text)]} else {crate::content::parts(&span.text)};
                        for part in parts {
                            match part {
                                crate::content::Part::Text(text) => for word in text.split_inclusive(char::is_whitespace) {
                                    let styled=div().max_w(px(360.)).flex_shrink_0().when(span.bold,|s|s.font_weight(FontWeight::BOLD)).when(span.italic,|s|s.italic()).when(span.strike,|s|s.line_through())
                                        .when(span.code,|s|s.font_family("monospace").bg(rgb(PANEL)).rounded_sm().px_1()).child(word.to_string());
                                    if let Some(url)=&span.link {let url=url.clone();items.push(styled.id(("text-link",items.len())).cursor_pointer().text_color(rgb(0x65b8f7)).on_click(move |_,_,cx|open_https(&url,cx)).into_any_element());}
                                    else {items.push(styled.into_any_element());}
                                },
                                crate::content::Part::Emoji {name,id} => {
                                    let image=self.media.get(&format!("https://cdn.discordapp.com/emojis/{id}.png?size=64&quality=lossless"));
                                    items.push(div().size(px(22.)).flex_shrink_0().overflow_hidden().map(|s|match image {Some(image)=>s.child(img(image).size_full()),None=>s.child(label(format!(":{name}:"))).text_size(px(8.))}).into_any_element());
                                }
                            }
                        }
                    }
                }
                row().id(("rich-block",index)).w_full().min_w_0().flex_wrap().min_h(px(22.))
                    .text_size(px(if block.heading>0 {22.-(block.heading.min(4)-1) as f32*2.}else{14.})).line_height(px(if block.heading>0{28.}else{22.}))
                    .when(block.heading>0,|s|s.font_weight(FontWeight::BOLD).mt_1())
                    .when(block.code,|s|s.font_family("monospace").p_2().rounded_md().bg(rgb(PANEL)))
                    .when(block.quote,|s|s.pl_3().border_l_2().border_color(rgb(MUTED))).children(items)
            })).into_any_element()
    }
}
