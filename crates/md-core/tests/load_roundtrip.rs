use md_core::block::BlockKind;
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

fn has_kind(doc: &md_core::document::Document, kind: BlockKind) -> bool {
    doc.preorder()
        .into_iter()
        .any(|id| doc.kind(id.index) == Some(kind))
}

fn shape(doc: &md_core::document::Document) -> Vec<String> {
    doc.preorder()
        .into_iter()
        .map(|id| format!("{:?}|{:?}", doc.kind(id.index), doc.display(id)))
        .collect()
}

#[test]
fn shapes_the_editor_can_produce_are_fixed_points() {
    for source in [
        "> hi\n> \n",
        "\nhi\n",
        "- hi\n  ```\n  abc\n  ```\n- \n",
        "- hi\n  ```\n  abc\n  ```\n  z\n",
        "- hi\n  ```\n  abc\n  ```\n  z\n- w\n",
        "a\n\n\nb\n",
        "a\n\n\n",
    ] {
        let before = load_markdown(source, editor_options());
        let saved = before.to_markdown();
        assert_eq!(saved, source, "{source:?} must survive a save");
        let after = load_markdown(&saved, editor_options());
        assert_eq!(
            after.to_markdown(),
            source,
            "{source:?} must be a fixed point"
        );
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} must reload as the same tree"
        );
    }
}

#[test]
fn a_list_item_keeps_the_blank_lines_that_are_indented_to_its_content() {
    for source in [
        "- hi\n  ```\n  abc\n  ```\n  \n  \n",
        "- hi\n  \n",
        "- hi\n  \n  \n",
        "- hi\n  z\n  \n",
        "- \n  \n",
        "1. \n   \n",
        "- a\n  - b\n    \n",
        "> - hi\n>   \n",
    ] {
        let doc = load_markdown(source, editor_options());
        let saved = doc.to_markdown();
        assert_eq!(saved, source, "{source:?} must survive a save");
        let after = load_markdown(&saved, editor_options());
        assert_eq!(
            shape(&doc),
            shape(&after),
            "{source:?} must reload as the same tree"
        );
    }
}

#[test]
fn a_list_item_leaves_the_blank_lines_it_does_not_own_alone() {
    for (source, saved) in [
        ("- hi\n\n", "- hi\n\n"),
        ("- hi\n\n\n", "- hi\n\n\n"),
        (
            "- hi\n  ```\n  abc\n  ```\n\n\n",
            "- hi\n  ```\n  abc\n  ```\n\n\n",
        ),
        ("- hi\n  \n- b\n", "- hi\n\n- b\n"),
        ("- hi\n  \nz\n", "- hi\n\nz\n"),
        ("- hi\n  \n\n", "- hi\n\n\n"),
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), saved, "{source:?} changed shape");
    }
}

#[test]
fn a_list_keeps_the_blank_lines_that_follow_it() {
    for source in [
        "- a\n\n\np\n",
        "- a\n\n\n* b\n",
        "- a\n\n\n> q\n",
        "- a\n\n\n# h\n",
        "- a\n\n\n***\n",
        "- a\n\n\n\n\np\n",
        "* a\n\n\np\n",
        "1. a\n\n\np\n",
        "- [ ] a\n\n\np\n",
        "- a\n  - b\n\n\np\n",
        "- a\n\np\n",
        "> - a\n> \n> \n> p\n",
    ] {
        let before = load_markdown(source, editor_options());
        let saved = before.to_markdown();
        assert_eq!(saved, source, "{source:?} must survive a save");
        let after = load_markdown(&saved, editor_options());
        assert_eq!(
            after.to_markdown(),
            source,
            "{source:?} must be a fixed point"
        );
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} must reload as the same tree"
        );
    }
}

#[test]
fn a_footnote_definition_keeps_the_blank_lines_that_follow_it() {
    for source in [
        "[^1]: n\n\n\np\n",
        "[^1]: n\n\n\n",
        "[^1]: n\n\np\n",
        "p\n\n[^1]: n\n\n\n",
        "> [^1]: n\n> \n> \n> p\n",
    ] {
        let before = load_markdown(source, editor_options());
        let saved = before.to_markdown();
        assert_eq!(saved, source, "{source:?} must survive a save");
        let after = load_markdown(&saved, editor_options());
        assert_eq!(
            after.to_markdown(),
            source,
            "{source:?} must be a fixed point"
        );
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} must reload as the same tree"
        );
    }
}

#[test]
fn a_list_item_keeps_the_blank_lines_between_its_blocks() {
    for source in [
        "- p\n  \n  \n  p\n",
        "- p\n  \n  \n  \n  p\n",
        "- p\n  q\n  \n  \n  r\n",
        "1. a\n   \n   \n   b\n",
        "- # h\n  \n  \n  p\n",
        "- p\n  \n  p\n",
        "- > p\n  > \n  > \n  > p\n",
        "- a\n  - p\n    \n    \n    p\n",
    ] {
        let before = load_markdown(source, editor_options());
        let saved = before.to_markdown();
        assert_eq!(saved, source, "{source:?} must survive a save");
        let after = load_markdown(&saved, editor_options());
        assert_eq!(
            after.to_markdown(),
            source,
            "{source:?} must be a fixed point"
        );
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} must reload as the same tree"
        );
    }
}

#[test]
fn a_list_does_not_claim_a_blank_paragraph_between_its_items() {
    for (source, saved) in [
        ("- a\n\n\n- b\n", "- a\n\n- b\n"),
        ("- a\n\n\n\n- b\n", "- a\n\n- b\n"),
        ("> - a\n> \n> \n> - b\n", "> - a\n> \n> - b\n"),
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), saved, "{source:?} changed shape");
    }
}

#[test]
fn the_block_after_a_display_math_does_not_keep_the_seam_whitespace() {
    for (source, tail) in [
        ("$$x$$\nz\n", "z"),
        ("$$x$$ b\n", "b"),
        ("a $$x$$\nz\n", "z"),
        ("a $$x$$ b\n", "b"),
        ("$$x$$\nz\nw\n", "z\nw"),
        ("$$x$$ *e*\n", "e"),
        ("- hi\n  $$x$$\n  z\n", "z"),
        ("- hi\n  $$x$$ b\n", "b"),
        ("> $$x$$\n> z\n", "z"),
    ] {
        let doc = load_markdown(source, editor_options());
        let after = doc
            .preorder()
            .into_iter()
            .skip_while(|&id| doc.kind(id.index) != Some(BlockKind::Math))
            .skip(1)
            .find(|&id| doc.kind(id.index) == Some(BlockKind::Paragraph))
            .unwrap_or_else(|| panic!("{source:?} must keep a paragraph after the math"));
        assert_eq!(doc.display(after), tail, "{source:?}");
    }
}

#[test]
fn a_display_math_stops_handing_its_line_break_to_the_next_block() {
    for (source, saved) in [
        ("$$x$$\nz\n", "$$x$$\n\nz\n"),
        ("$$x$$ b\n", "$$x$$\n\nb\n"),
        ("a $$x$$\nz\n", "a\n\n$$x$$\n\nz\n"),
        ("$$x$$\nz\nw\n", "$$x$$\n\nz\nw\n"),
        ("- hi\n  $$x$$\n  z\n", "- hi\n  $$x$$\n  z\n"),
        ("- hi\n  $$x$$ b\n", "- hi\n  $$x$$\n  b\n"),
        ("> $$x$$\n> z\n", "> $$x$$\n> \n> z\n"),
    ] {
        let before = load_markdown(source, editor_options());
        assert_eq!(before.to_markdown(), saved, "{source:?} must save");
        let after = load_markdown(&before.to_markdown(), editor_options());
        assert_eq!(
            after.to_markdown(),
            saved,
            "{source:?} must be a fixed point"
        );
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} must reload as the same tree"
        );
    }
}

#[test]
fn an_empty_front_matter_at_the_top_loads_as_a_metadata_block() {
    for source in [
        "---\n\n---\n",
        "---\n\n\n---\n",
        "---\n \n---\n",
        "-----\n\n-----\n",
        "---\n\n---\n\nbody\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert!(
            has_kind(&doc, BlockKind::MetadataBlock),
            "{source:?} must open an empty front matter"
        );
        assert_eq!(
            doc.text_leaves().len(),
            1 + source.contains("body") as usize,
            "{source:?} must not invent blocks"
        );
        assert_eq!(doc.to_markdown(), source, "{source:?} must survive a save");
        let again = load_markdown(&doc.to_markdown(), editor_options());
        assert_eq!(
            again.to_markdown(),
            source,
            "{source:?} must be a fixed point"
        );
        assert_eq!(
            shape(&doc),
            shape(&again),
            "{source:?} must reload as the same tree"
        );
        assert!(has_kind(&again, BlockKind::MetadataBlock), "{source:?}");
    }
}

#[test]
fn two_adjacent_dash_rules_gain_the_line_that_a_front_matter_needs() {
    for (source, saved) in [
        ("---\n---\n", "---\n\n---\n"),
        ("---\n---\nbody\n", "---\n\n---\nbody\n"),
    ] {
        let doc = load_markdown(source, editor_options());
        assert!(
            has_kind(&doc, BlockKind::MetadataBlock),
            "{source:?} must open an empty front matter"
        );
        assert_eq!(doc.to_markdown(), saved, "{source:?}");
        let again = load_markdown(saved, editor_options());
        assert_eq!(
            again.to_markdown(),
            saved,
            "{source:?} must be a fixed point"
        );
        assert_eq!(shape(&doc), shape(&again), "{source:?}");
    }
}

#[test]
fn a_non_empty_front_matter_is_untouched_by_the_empty_rule() {
    for source in [
        "---\ntitle: hello\n---\n",
        "---\ntitle: hello\n---\n# Body\n",
        "+++\ntitle = \"hello\"\n+++\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(
            doc.kind(doc.text_leaves()[0]),
            Some(BlockKind::MetadataBlock),
            "{source:?}"
        );
        assert_eq!(doc.to_markdown(), source, "{source:?}");
    }
}

#[test]
fn only_dash_delimiters_that_open_the_document_make_front_matter() {
    for source in [
        "***\n\n***\n",
        "___\n___\n",
        "- - -\n\n- - -\n",
        "  ---\n\n  ---\n",
        "body\n\n---\n\n---\n",
        "---\n",
        "***\n",
        "---\n\n***\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert!(
            !has_kind(&doc, BlockKind::MetadataBlock),
            "{source:?} must not open a front matter"
        );
    }
}

#[test]
fn an_image_that_a_math_fence_precedes_keeps_its_source_span() {
    for (source, saved) in [
        ("$$\nx\n$$\n![a](u)\n", "$$\nx\n$$\n\n![a](u)\n"),
        ("$$\nx\n$$\n\n![a](u)\n", "$$\nx\n$$\n\n![a](u)\n"),
        ("$$\nx\n$$\n  ![cap](u)  \n", "$$\nx\n$$\n\n  ![cap](u)  \n"),
        ("p\n$$\nx\n$$\n![a](u)\n", "p\n\n$$\nx\n$$\n\n![a](u)\n"),
        ("# h\n![a](u)\n", "# h\n\n![a](u)\n"),
        ("***\n![a](u)\n", "***\n\n![a](u)\n"),
        ("```\nc\n```\n![a](u)\n", "```\nc\n```\n\n![a](u)\n"),
    ] {
        let before = load_markdown(source, editor_options());
        assert_eq!(before.to_markdown(), saved, "{source:?}");
        let after = load_markdown(&before.to_markdown(), editor_options());
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} must keep the image"
        );
        assert!(
            after.to_markdown().contains("![a](u)") || after.to_markdown().contains("![cap](u)"),
            "{source:?} must not lose the image: {:?}",
            after.to_markdown()
        );
    }
}

#[test]
fn an_image_only_paragraph_that_a_math_fence_cuts_keeps_its_kind() {
    for source in [
        "![a](u)\n$$\nx\n$$\n",
        "![a](u)\n\n$$\nx\n$$\n",
        "> ![a](u)\n> $$\n> x\n> $$\n",
        "![a](u)\n$$\nx\n$$\n\nz\n",
    ] {
        let before = load_markdown(source, editor_options());
        let saved = before.to_markdown();
        let after = load_markdown(&saved, editor_options());
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} saved as {saved:?} must reload as the same tree"
        );
        assert_eq!(
            after.to_markdown(),
            saved,
            "{source:?} must be a fixed point"
        );
    }
}

#[test]
fn a_paragraph_that_a_math_fence_cuts_keeps_being_a_paragraph_when_it_is_not_image_only() {
    for source in [
        "p ![a](u)\n$$\nx\n$$\n",
        "![a](u)![b](v)\n$$\nx\n$$\n",
        "![a](u)\n![b](v)\n$$\nx\n$$\n",
        "- ![a](u)\n  $$\n  x\n  $$\n",
        "- ![a](u)\n- b\n",
    ] {
        let before = load_markdown(source, editor_options());
        assert!(
            !has_kind(&before, BlockKind::Image),
            "{source:?} must keep a paragraph: {:?}",
            shape(&before)
        );
        let saved = before.to_markdown();
        let after = load_markdown(&saved, editor_options());
        assert_eq!(
            shape(&before),
            shape(&after),
            "{source:?} saved as {saved:?} must reload as the same tree"
        );
        assert_eq!(
            after.to_markdown(),
            saved,
            "{source:?} must be a fixed point"
        );
    }
}
