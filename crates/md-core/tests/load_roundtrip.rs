use md_core::document::{editor_options, load_markdown};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

fn codes(source: &str) -> Vec<String> {
    Parser::new_ext(source, editor_options())
        .filter_map(|event| match event {
            Event::Code(text) => Some(text.into_string()),
            _ => None,
        })
        .collect()
}

fn html(source: &str) -> Vec<String> {
    Parser::new_ext(source, editor_options())
        .filter_map(|event| match event {
            Event::Html(text) | Event::InlineHtml(text) => Some(text.into_string()),
            _ => None,
        })
        .collect()
}

fn fenced_blocks(source: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut current = None;
    for event in Parser::new_ext(source, editor_options()) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                current = Some((info.to_string(), String::new()));
            }
            Event::Text(text) if current.is_some() => {
                current.as_mut().unwrap().1.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(block) = current.take() {
                    result.push(block);
                }
            }
            _ => {}
        }
    }
    result
}

fn leaf_texts(doc: &md_core::document::Document) -> Vec<String> {
    doc.text_leaves()
        .iter()
        .map(|&id| doc.text_of(id).unwrap().to_owned())
        .collect::<Vec<_>>()
}

#[test]
fn math_in_marks_roundtrips_stably() {
    for source in [
        "**before $$x$$ *middle* after**\n",
        "[before $$x$$ *middle* after](https://example.test)\n",
    ] {
        let before = load_markdown(source, editor_options());
        let saved = before.to_markdown();
        let after = load_markdown(&saved, editor_options());
        println!("input={source:?}\nsaved={saved:?}");
        assert_eq!(
            before.text_leaves().len(),
            after.text_leaves().len(),
            "saving without edits must not add or drop leaves: {source:?} -> {saved:?}"
        );
        assert_eq!(
            after.to_markdown(),
            saved,
            "saving must be idempotent: {source:?}"
        );
    }
}

#[test]
fn list_host_indent_preserves_code_content() {
    for source in [
        "> - `alpha\n>     beta`\n",
        "-   `alpha\n      beta`\n",
        "> - outer\n>   - `alpha\n>       beta`\n",
    ] {
        let before = load_markdown(source, editor_options());
        let saved = before.to_markdown();
        let after = load_markdown(&saved, editor_options());
        println!("input={source:?}\nsaved={saved:?}");
        assert_eq!(
            leaf_texts(&before),
            leaf_texts(&after),
            "saving without edits must not change inline code content: {source:?} -> {saved:?}"
        );
    }
}

#[test]
fn math_split_keeps_enclosing_marks() {
    let source = "**before $$x$$ after**\n";
    let before = load_markdown(source, editor_options());
    let saved = before.to_markdown();
    let after = load_markdown(&saved, editor_options());
    println!("input={source:?}\nsaved={saved:?}");
    assert_eq!(
        leaf_texts(&before),
        leaf_texts(&after),
        "saving without edits must not turn markers into literal text: {saved:?}"
    );
    assert_eq!(
        after.to_markdown(),
        saved,
        "saving must be idempotent: {saved:?}"
    );
}

#[test]
fn list_code_partial_tab_round_trip() {
    let source = "- `a\n\tb`\n";
    let d = load_markdown(source, editor_options());
    let saved = d.to_markdown();
    println!(
        "source={source:?}; saved={saved:?}; before={:?}; after={:?}",
        codes(source),
        codes(&saved)
    );
    assert_eq!(
        codes(source),
        codes(&saved),
        "a cross-column Tab must not leak into the code span on save"
    );
}

#[test]
fn unicode_space_before_display_math_stays_visible() {
    for (name, ws) in [("nbsp", "\u{00a0}"), ("ideographic", "\u{3000}")] {
        let source = format!("a{ws}$$x$$\n");
        let d = load_markdown(&source, editor_options());
        let id = d.first_text_leaf().unwrap();
        println!(
            "{name}: saved={:?}; display={:?}",
            d.to_markdown(),
            d.text_of(id)
        );
        assert_eq!(
            d.text_of(id),
            Some(&*format!("a{ws}")),
            "{name} must stay visible in the display"
        );
        assert!(
            d.to_markdown().contains(&format!("a{ws}")),
            "{name} must stay in the saved output"
        );
    }
}

#[test]
fn styled_image_alt_is_one_atomic_run() {
    let d = load_markdown("prefix ![a *b*](image.png) suffix\n", editor_options());
    let id = d.live_id(d.first_text_leaf().unwrap()).unwrap();
    let images: Vec<_> = d.runs(id).iter().filter(|r| r.marks.is_image()).collect();
    println!("images={images:?}");
    assert_eq!(images.len(), 1, "one image must be one atomic run");
    let img = images[0];
    assert_eq!(img.display_range, 7..10, "the run must cover the whole alt");
    assert_eq!(d.link_dest(img.link.unwrap()), Some("image.png"));
    assert!(!img.marks.contains(md_core::inline::InlineMarks::EM));

    let d = load_markdown("pre *![a](i.png)* post\n", editor_options());
    let id = d.live_id(d.first_text_leaf().unwrap()).unwrap();
    let img: Vec<_> = d.runs(id).iter().filter(|r| r.marks.is_image()).collect();
    println!("outer={img:?}");
    assert_eq!(img.len(), 1);
    assert!(
        img[0].marks.contains(md_core::inline::InlineMarks::EM),
        "an emphasis wrapping the image must be preserved"
    );

    let d = load_markdown("![a](i1.png)![b](i2.png)\n", editor_options());
    let id = d.live_id(d.first_text_leaf().unwrap()).unwrap();
    let imgs: Vec<_> = d.runs(id).iter().filter(|r| r.marks.is_image()).collect();
    println!("adjacent={imgs:?}");
    assert_eq!(imgs.len(), 2, "two adjacent images must stay two runs");
    assert_ne!(imgs[0].link, imgs[1].link);

    let d = load_markdown("![](i.png) tail\n", editor_options());
    let id = d.live_id(d.first_text_leaf().unwrap()).unwrap();
    let imgs: Vec<_> = d.runs(id).iter().filter(|r| r.marks.is_image()).collect();
    assert_eq!(imgs.len(), 1, "an empty-alt image stays one run");
}

#[test]
fn saving_multiline_inline_code_preserves_backticks() {
    let source = "p `foo\n    ```\nbar`\n";
    let saved = load_markdown(source, editor_options()).to_markdown();
    println!(
        "source={source:?}; saved={saved:?}; before={:?}; after={:?}",
        codes(source),
        codes(&saved)
    );
    assert_eq!(codes(&saved), codes(source));
}

#[test]
fn saving_html_comment_preserves_fence_looking_interior() {
    let source = "<!--\n```js\n-->\n";
    let saved = load_markdown(source, editor_options()).to_markdown();
    println!(
        "source={source:?}; saved={saved:?}; before={:?}; after={:?}",
        html(source),
        html(&saved)
    );
    assert_eq!(
        saved, source,
        "raw HTML must survive the save byte-identically"
    );
    assert_eq!(html(&saved), html(source));
}

#[test]
fn saving_still_escapes_real_fence_starts_in_text() {
    let mut d = load_markdown("before\n\nafter\n", editor_options());
    let leaves = d.text_leaves();
    let _ = d.replace_text(leaves[0], 6..6, "\n\n```rust\nfn\n```");
    let saved = d.to_markdown();
    println!("real={saved:?}");
    assert_eq!(saved, "before\n\n\\```rust\nfn\n\\```\n\nafter\n");
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(
        reloaded.to_markdown(),
        saved,
        "escaping must be a fixed point"
    );
}

#[test]
fn fence_info_encoding_preserves_block_boundary() {
    let source = "```x&#96;y\nimportant code\n```\n\ntail\n";
    let saved = load_markdown(source, editor_options()).to_markdown();
    assert_eq!(
        fenced_blocks(source),
        vec![("x`y".into(), "important code\n".into())]
    );
    assert_eq!(
        fenced_blocks(&saved),
        fenced_blocks(source),
        "saved={saved:?}"
    );
}

#[test]
fn fence_info_ampersand_is_not_decoded_twice() {
    let source = "~~~a&amp;copy;\nimportant code\n~~~\n";
    let saved = load_markdown(source, editor_options()).to_markdown();
    assert_eq!(
        fenced_blocks(&saved),
        fenced_blocks(source),
        "saved={saved:?}"
    );
}

#[test]
fn html_projection_excludes_quoted_attributes() {
    let doc = load_markdown("<div data-note=\"1 > 2\">Hello</div>\n", editor_options());
    assert_eq!(doc.text_of(doc.text_leaves()[0]), Some("Hello"));
}

#[test]
fn html_projection_excludes_single_quoted_attributes() {
    let doc = load_markdown("<div data-note='1 > 2'>Hello</div>\n", editor_options());
    assert_eq!(doc.text_of(doc.text_leaves()[0]), Some("Hello"));
}

#[test]
fn html_projection_excludes_comment_contents() {
    let doc = load_markdown("<!-- > private -->\n<div>Hello</div>\n", editor_options());
    let visible: String = doc
        .text_leaves()
        .iter()
        .map(|&block| doc.text_of(block).unwrap())
        .collect();
    assert_eq!(visible, "Hello");
}

#[test]
fn html_projection_plain_attribute_stays_excluded() {
    let doc = load_markdown("<div data-note=\"plain\">Hello</div>\n", editor_options());
    assert_eq!(doc.text_of(doc.text_leaves()[0]), Some("Hello"));
}

#[test]
fn html_projection_adjacent_text_keeps_spacing() {
    let doc = load_markdown("<div>a<br>b</div>\n", editor_options());
    assert_eq!(doc.text_of(doc.text_leaves()[0]), Some("a b"));
}

#[test]
fn html_projection_does_not_touch_serialized_source() {
    let doc = load_markdown(
        "<!-- note -->\n<div data-note=\"1 > 2\">Hello</div>\n",
        editor_options(),
    );
    let saved = doc.to_markdown();
    println!("saved={saved:?}");
    assert!(saved.contains("<!-- note -->"), "saved={saved:?}");
    assert!(
        saved.contains("<div data-note=\"1 > 2\">Hello</div>"),
        "saved={saved:?}"
    );
}
