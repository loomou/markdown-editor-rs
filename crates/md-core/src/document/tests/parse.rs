use super::support::kind_count;
use crate::document::change::ChangeSet;
use crate::document::load::display_matches_pieces;
use crate::document::{PasteIntent, dump_structure, editor_options, load_markdown};

#[test]
fn tiny_open_has_replace_changeset() {
    let doc = load_markdown("# hi\n\npara\n", editor_options());
    assert_eq!(doc.revision, 1);
    assert_eq!(doc.changes, ChangeSet::document_replaced(1));
    assert_eq!(
        doc.arena.get(doc.root).unwrap().kind,
        crate::block::BlockKind::DocRoot
    );
    assert!(doc.arena.live_count() >= 2);
}

#[test]
fn empty_markdown_has_paragraph() {
    let doc = load_markdown("", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 1);
    assert_eq!(
        doc.kind(leaves[0]),
        Some(crate::block::BlockKind::Paragraph)
    );
    assert_eq!(doc.text_of(leaves[0]), Some(""));
    assert_eq!(
        doc.arena
            .get(doc.root)
            .and_then(|n| n.first_child)
            .map(|id| id.index),
        Some(leaves[0])
    );
    let id = doc.live_id(leaves[0]).expect("leaf");
    assert_eq!(doc.leaf_source(id), "");
}

#[test]
fn phrasing_soft_break_is_newline() {
    let doc = load_markdown("hello\nworld\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 1);
    let id = doc.live_id(leaves[0]).expect("leaf");
    assert_eq!(
        doc.kind(leaves[0]),
        Some(crate::block::BlockKind::Paragraph)
    );
    assert_eq!(doc.display(id), "hello\nworld");
    assert_eq!(doc.leaf_source(id), "hello\nworld");
}

#[test]
fn phrasing_leaf_source_keeps_inline_delimiters() {
    let doc = load_markdown("hello `d` and *em*\n", editor_options());
    let id = doc.live_id(doc.text_leaves()[0]).expect("leaf");
    assert_eq!(doc.display(id), "hello d and em");
    let src = doc.leaf_source(id);
    assert!(src.contains('`'), "{src:?}");
    assert!(src.contains('*'), "{src:?}");
    assert!(src.contains('d'), "{src:?}");
    assert!(src.contains("em"), "{src:?}");
}

#[test]
fn loaded_phrasing_leaf_has_a_source_display_cache() {
    let doc = load_markdown("hello `code` and *em*\n", editor_options());
    let id = doc.live_id(doc.text_leaves()[0]).expect("leaf");
    let text = doc
        .arena
        .get(id)
        .and_then(|node| node.text)
        .and_then(|text| doc.texts.get(text))
        .expect("text");
    assert_eq!(text.s2d.len(), doc.leaf_source(id).len() + 1);
    assert_eq!(text.s2d.last().copied(), Some(doc.display(id).len()));
}

#[test]
fn code_block_source_matches_display() {
    let doc = load_markdown("```rust\nfn x() {}\n```\n", editor_options());
    let id = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(crate::block::BlockKind::CodeBlock))
        .expect("code");
    assert_eq!(doc.display(id), "fn x() {}");
    assert_eq!(doc.leaf_source(id), doc.display(id));
}

#[test]
fn pieces_rebuild_display() {
    let doc = load_markdown("hello *em* world\n", editor_options());
    let mut found = false;
    for id in doc.preorder() {
        if let Some(tid) = doc.arena.get(id).and_then(|n| n.text)
            && let Some(leaf) = doc.texts.get(tid)
        {
            assert!(display_matches_pieces(&doc, leaf));
            found = found || !leaf.pieces.is_empty();
        }
    }
    assert!(found);
}

#[test]
fn inline_marks_and_links_load_and_inherit_on_edit() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown(
        "**a** *b* ~~c~~ `d` [e](https://ex) ^s^ ~u~\n",
        editor_options(),
    );
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let runs = doc.runs(id);
    assert!(runs.iter().any(|r| r.marks.contains(InlineMarks::STRONG)));
    assert!(runs.iter().any(|r| r.marks.contains(InlineMarks::EM)));
    assert!(runs.iter().any(|r| r.marks.contains(InlineMarks::STRIKE)));
    assert!(runs.iter().any(|r| r.marks.contains(InlineMarks::CODE)));
    assert!(runs.iter().any(|r| r.marks.contains(InlineMarks::SUPER)));
    assert!(runs.iter().any(|r| r.marks.contains(InlineMarks::SUB)));
    let link = runs.iter().find(|r| r.link.is_some()).expect("link run");
    assert_eq!(doc.link_dest(link.link.unwrap()), Some("https://ex"));
    assert_eq!(
        doc.link_at(id, link.display_range.start as usize),
        Some("https://ex")
    );
    let _ = doc.replace_text(leaf, 0..0, "X");
    assert!(doc.leaf_source(id).contains('X'));
    assert!(doc.display(id).contains('X'));
    let runs = doc.runs(id);
    assert!(runs.iter().any(|r| r.marks.contains(InlineMarks::STRONG)));
}

#[test]
fn gfm_block_kinds_round_trip() {
    let doc = load_markdown(
        "1. a\n\n- [ ] t\n\n---\n\n> [!NOTE]\n> n\n\nterm\n: def\n\n![solo](x.png)\n\nSee [^1]\n\n[^1]: foot\n",
        editor_options(),
    );
    assert!(kind_count(&doc, crate::block::BlockKind::ThematicBreak) >= 1);
    assert!(kind_count(&doc, crate::block::BlockKind::Image) >= 1);
    assert!(kind_count(&doc, crate::block::BlockKind::List) >= 1);
    assert!(kind_count(&doc, crate::block::BlockKind::FootnoteDefinition) >= 1);
    let ordered = doc.preorder().into_iter().any(|id| {
        matches!(
            doc.extra(id),
            crate::block::NodeExtra::List { start: Some(_), .. }
        )
    });
    assert!(ordered);
    let alert = doc
        .preorder()
        .into_iter()
        .any(|id| doc.extra(id).quote_alert().is_some());
    assert!(alert);
}

#[test]
fn mermaid_fence_is_mermaid_kind() {
    let doc = load_markdown(
        "```mermaid\nflowchart TD\nA-->B\n```\n\n```rust\nfn x() {}\n```\n",
        editor_options(),
    );
    assert_eq!(kind_count(&doc, crate::block::BlockKind::Mermaid), 1);
    assert_eq!(kind_count(&doc, crate::block::BlockKind::CodeBlock), 1);
    let mermaid = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(crate::block::BlockKind::Mermaid))
        .expect("mermaid node");
    assert_eq!(doc.display(mermaid), "flowchart TD\nA-->B");
}

#[test]
fn rust_fence_keeps_lang() {
    use crate::block::BlockKind;
    let doc = load_markdown("```Rust\nfn x() {}\n```\n", editor_options());
    let id = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
        .expect("code");
    let lang = doc.extra(id).code_fence_lang().expect("lang");
    assert_eq!(doc.lang(lang), Some("Rust"));
    assert_eq!(doc.display(id), "fn x() {}");
}

#[test]
fn indented_and_bare_fence_have_no_lang() {
    use crate::block::BlockKind;
    let indented = load_markdown("    code\n", editor_options());
    let code = indented
        .preorder()
        .into_iter()
        .find(|id| indented.arena.get(*id).map(|n| n.kind) == Some(BlockKind::CodeBlock));
    if let Some(id) = code {
        assert!(indented.extra(id).code_fence_lang().is_none());
        assert!(indented.extra(id).code_is_indented());
    }
    let bare = load_markdown("```\nplain\n```\n", editor_options());
    let id = bare
        .preorder()
        .into_iter()
        .find(|id| bare.arena.get(*id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
        .expect("code");
    assert!(bare.extra(id).code_fence_lang().is_none());
    assert!(!bare.extra(id).code_is_indented());
}

#[test]
fn paste_remaps_code_fence_lang() {
    use crate::block::BlockKind;
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.paste(
        leaf,
        5..5,
        "\n\n```py\nprint(1)\n```\n",
        PasteIntent::IndependentFragment,
    );
    let id = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
        .expect("code");
    let lang = doc.extra(id).code_fence_lang().expect("lang");
    assert_eq!(doc.lang(lang), Some("py"));
}

#[test]
fn image_block_keeps_dest_and_caption() {
    use crate::block::BlockKind;
    let doc = load_markdown("![cap](./a.png)\n", editor_options());
    let id = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(BlockKind::Image))
        .expect("image");
    let dest = doc.extra(id).image_dest().expect("dest");
    assert_eq!(doc.link_dest(dest), Some("./a.png"));
    assert_eq!(doc.display(id), "cap");
}

#[test]
fn mixed_image_stays_paragraph_with_link() {
    use crate::block::BlockKind;
    use crate::inline::InlineMarks;
    let doc = load_markdown("x ![a](u) y\n", editor_options());
    let para = doc
        .preorder()
        .into_iter()
        .find(|id| {
            doc.arena.get(*id).map(|n| n.kind) == Some(BlockKind::Paragraph)
                && doc.display(*id).contains('x')
        })
        .expect("para");
    assert_eq!(kind_count(&doc, BlockKind::Image), 0);
    assert_eq!(doc.display(para), "x a y");
    let run = doc
        .runs(para)
        .iter()
        .find(|r| r.marks.contains(InlineMarks::IMAGE))
        .expect("image run");
    assert_eq!(doc.link_dest(run.link.expect("link")), Some("u"));
}

#[test]
fn editing_keeps_two_adjacent_images_with_the_same_destination() {
    use crate::doc::Doc;
    use crate::document::edit::{Caret, Command, Sel};
    let mut doc = Doc::new(load_markdown(
        "a ![one](img)![two](img) z\n",
        editor_options(),
    ));
    let block = doc.text_leaves()[0];
    let id = doc.document.live_id(block).unwrap();
    let count = |doc: &Doc| {
        doc.document
            .leaf_snapshot(id)
            .unwrap()
            .runs
            .iter()
            .filter(|run| run.marks.is_image())
            .count()
    };
    assert_eq!(count(&doc), 2);
    let end = doc.text(block).unwrap().len();
    let _ = doc.apply(
        Sel::collapsed(Caret { block, offset: end }),
        Command::Insert { text: "!".into() },
    );
    assert_eq!(count(&doc), 2);
}

#[test]
fn inline_math_and_display_math_kinds() {
    use crate::inline::InlineMarks;
    let doc = load_markdown(
        "see $a+b$ here\n\n$$x^2$$\n\n# Title $E=mc^2$\n",
        editor_options(),
    );
    assert_eq!(kind_count(&doc, crate::block::BlockKind::Math), 1);
    let para = doc
        .preorder()
        .into_iter()
        .find(|id| {
            doc.arena.get(*id).map(|n| n.kind) == Some(crate::block::BlockKind::Paragraph)
                && doc.display(*id).contains("see")
        })
        .expect("para");
    assert!(
        doc.runs(para)
            .iter()
            .any(|r| r.marks.contains(InlineMarks::MATH_INLINE))
    );
    assert!(
        !doc.runs(para)
            .iter()
            .any(|r| r.marks.contains(InlineMarks::MATH_DISPLAY))
    );
    let math = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(crate::block::BlockKind::Math))
        .expect("display math");
    assert_eq!(doc.display(math), "x^2");
    let heading = doc
        .preorder()
        .into_iter()
        .find(|id| {
            matches!(
                doc.arena.get(*id).map(|n| n.kind),
                Some(crate::block::BlockKind::Heading(_))
            )
        })
        .expect("heading");
    assert!(
        doc.runs(heading)
            .iter()
            .any(|r| r.marks.contains(InlineMarks::MATH_INLINE))
    );
    assert_eq!(kind_count(&doc, crate::block::BlockKind::Heading(1)), 1);
}

#[test]
fn lone_display_math_is_math_block() {
    let doc = load_markdown("$$a+b$$\n", editor_options());
    let kinds: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .map(|b| (b, doc.kind(b), doc.text_of(b).unwrap_or("").to_string()))
        .collect();
    assert_eq!(kinds.len(), 1, "leaves={kinds:?}\n{}", dump_structure(&doc));
    assert_eq!(kinds[0].1, Some(crate::block::BlockKind::Math));
    assert_eq!(kinds[0].2, "a+b");
}

#[test]
fn glued_display_math_is_a_sibling_math_block() {
    use crate::block::BlockKind;
    use crate::inline::InlineMarks;
    let doc = load_markdown("line1\nline2\n$$\n\\frac{a}{b}\n$$\n", editor_options());
    assert_eq!(kind_count(&doc, BlockKind::Math), 1);
    let para = doc
        .preorder()
        .into_iter()
        .find(|id| {
            doc.arena.get(*id).map(|n| n.kind) == Some(BlockKind::Paragraph)
                && doc.display(*id).contains("line1")
        })
        .expect("para");
    assert_eq!(doc.display(para), "line1\nline2");
    assert!(
        !doc.runs(para)
            .iter()
            .any(|r| r.marks.contains(InlineMarks::MATH_DISPLAY)),
        "the math must not remain in the paragraph above"
    );
    let math = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(BlockKind::Math))
        .expect("math");
    assert_eq!(
        doc.display(math),
        "\\frac{a}{b}",
        "the newline after $$ is not part of the LaTeX"
    );
    assert_eq!(doc.leaf_source(math), doc.display(math));
}

#[test]
fn multiline_display_math_drops_delimiter_newline() {
    let doc = load_markdown("$$\n\\frac{a}{b}\n$$\n", editor_options());
    let math = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(crate::block::BlockKind::Math))
        .expect("math");
    assert_eq!(doc.display(math), "\\frac{a}{b}");
    assert!(!doc.display(math).starts_with('\n'));
}

#[test]
fn loose_colon_lines_do_not_wrap_a_leaf() {
    let samples = [
        "role\n\n: the parse-scope mode.\n",
        "- role\n\n  : the parse-scope mode.\n",
        "term\n: desc\n",
    ];
    for md in samples {
        let doc = load_markdown(md, editor_options());
        for id in doc.preorder() {
            let n = doc.arena.get(id).expect("live");
            if n.kind.is_vertical_container() || n.kind == crate::block::BlockKind::TableRow {
                continue;
            }
            assert!(
                n.first_child.is_none(),
                "{md:?}: leaf {:?} {} has children",
                n.kind,
                id.index
            );
        }
        assert!(
            doc.to_markdown().contains(": "),
            "{md:?}: the `: ` was eaten"
        );
    }
    let rustc = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/rustc.md");
    if let Ok(md) = std::fs::read_to_string(rustc) {
        let doc = load_markdown(&md, editor_options());
        for id in doc.preorder() {
            let n = doc.arena.get(id).expect("live");
            if n.kind.is_vertical_container() || n.kind == crate::block::BlockKind::TableRow {
                continue;
            }
            assert!(
                n.first_child.is_none(),
                "rustc.md: leaf {:?} {} has children",
                n.kind,
                id.index
            );
        }
    }
}

#[test]
fn load_shrinks_run_and_piece_capacity() {
    let doc = load_markdown(
        "**bold** *em* `code` ~~strike~~ [a](https://example.com)\n\nplain para\n",
        editor_options(),
    );
    let mut saw_text = false;
    for block in doc.text_leaves() {
        let nid = doc.live_id(block).expect("live");
        let tid = doc.arena.get(nid).and_then(|n| n.text).expect("text");
        let leaf = doc.texts.get(tid).expect("leaf");
        assert_eq!(
            leaf.snapshot.runs.capacity(),
            leaf.runs().len(),
            "runs still have spare cap on block {block}"
        );
        assert_eq!(
            leaf.pieces.capacity(),
            leaf.pieces.len(),
            "pieces still have spare cap on block {block}"
        );
        saw_text = true;
    }
    assert!(saw_text);
}

#[test]
fn load_packs_text_store_densely() {
    let doc = load_markdown("# h\n\n- a\n- b\n\n> q\n", editor_options());
    let leaves = doc.text_leaves().len();
    assert!(
        doc.texts.dense_len() >= leaves,
        "dense {} < text_leaves {}",
        doc.texts.dense_len(),
        leaves
    );
    assert!(
        doc.texts.index_len() > doc.texts.dense_len(),
        "index {} should exceed dense {} (containers share the sparse map)",
        doc.texts.index_len(),
        doc.texts.dense_len()
    );
    assert_eq!(
        doc.texts.occupied() + doc.texts.free_len(),
        doc.texts.dense_len()
    );
}

#[test]
fn load_phrasing_source_is_a_span_into_document_source() {
    let doc = load_markdown("hello **x**\n", editor_options());
    let id = doc.live_id(doc.text_leaves()[0]).expect("live");
    let tid = doc.arena.get(id).and_then(|n| n.text).expect("text");
    let leaf = doc.texts.get(tid).expect("leaf");
    assert!(
        matches!(leaf.source, crate::document::text::LeafSource::Span(_)),
        "phrasing should keep a span into doc.source, not a copy"
    );
    assert_eq!(doc.leaf_source(id), "hello **x**");
    assert!(doc.source.contains(doc.leaf_source(id)));
}

#[test]
fn load_fence_source_shares_display() {
    let doc = load_markdown("```\ncode\n```\n", editor_options());
    let id = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|n| n.kind) == Some(crate::block::BlockKind::CodeBlock))
        .expect("fence");
    let tid = doc.arena.get(id).and_then(|n| n.text).expect("text");
    let leaf = doc.texts.get(tid).expect("leaf");
    assert!(matches!(
        leaf.source,
        crate::document::text::LeafSource::SameAsDisplay
    ));
    assert_eq!(doc.leaf_source(id), doc.display(id));
}

#[test]
fn editing_then_reading_source_sees_the_new_text() {
    let mut doc = load_markdown("ab\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.leaf_source(id), "ab");
    let _ = doc.replace_text(leaf, 0..2, "cd");
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.leaf_source(id), "cd");
    let tid = doc.arena.get(id).and_then(|n| n.text).expect("text");
    let kind = &doc.texts.get(tid).expect("leaf").source;
    assert!(
        matches!(
            kind,
            crate::document::text::LeafSource::Owned(_)
                | crate::document::text::LeafSource::SameAsDisplay
        ),
        "an edit must drop the load-time span"
    );
}

#[test]
fn editing_a_fence_keeps_source_aligned_with_display() {
    let mut doc = load_markdown("```\nab\n```\n", editor_options());
    let block = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(crate::block::BlockKind::CodeBlock))
        .expect("fence");
    let _ = doc.replace_text(block, 0..2, "cd");
    let id = doc.live_id(block).expect("live");
    assert_eq!(doc.leaf_source(id), doc.display(id));
    assert_eq!(doc.display(id), "cd");
}

#[test]
fn emptied_headings_keep_their_marker_and_retake_input() {
    use crate::doc::Doc;
    use crate::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown("# heading\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let caret = |offset: usize| Caret {
        block: leaf,
        offset,
    };

    let _ = doc.apply(
        Sel {
            anchor: caret(0),
            head: caret(7),
        },
        Command::DeleteBackward,
    );
    let id = doc.document.live_id(leaf).expect("live");
    assert_eq!(
        doc.document.leaf_source(id),
        "# ",
        "clearing the text must not eat the marker prefix"
    );

    let _ = doc.apply(
        Sel::collapsed(caret(0)),
        Command::Insert { text: "€".into() },
    );
    assert_eq!(
        doc.document.leaf_source(id),
        "# €",
        "typing into the emptied heading must land after the marker"
    );
    assert_eq!(doc.document.to_markdown(), "# €\n");
}

#[test]
fn heading_edits_undo_redo_without_eating_the_marker_prefix() {
    use crate::doc::Doc;
    use crate::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown("# heading\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let caret = |offset: usize| Caret {
        block: leaf,
        offset,
    };
    let atx_ok = |source: &str| {
        let hashes = source.chars().take_while(|c| *c == '#').count();
        source[hashes..].is_empty() || source[hashes..].starts_with(' ')
    };

    let _ = doc.apply(
        Sel::collapsed(caret(0)),
        Command::Insert {
            text: "🙂 ".into()
        },
    );
    let _ = doc.apply(Sel::collapsed(caret(5)), Command::DeleteBackward);
    let id = doc.document.live_id(leaf).expect("live");
    assert!(
        atx_ok(doc.document.leaf_source(id)),
        "mid-edit source: {:?}",
        doc.document.leaf_source(id)
    );
    let edited_markdown = doc.document.to_markdown();

    while doc.undo().is_some() {}
    assert_eq!(
        doc.document.leaf_source(id),
        "# heading",
        "undo-to-bottom must restore the loaded source verbatim"
    );
    while doc.redo().is_some() {}
    assert!(
        atx_ok(doc.document.leaf_source(id)),
        "redo-to-bottom source: {:?}",
        doc.document.leaf_source(id)
    );
    assert_eq!(doc.document.to_markdown(), edited_markdown);
}

#[test]
fn bare_hash_heading_takes_input_after_a_space() {
    use crate::doc::Doc;
    use crate::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown("#\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert { text: "x".into() },
    );
    let id = doc.document.live_id(leaf).expect("live");
    assert_eq!(doc.document.leaf_source(id), "# x");
    assert_eq!(doc.document.to_markdown(), "# x\n");
}

#[test]
fn merging_into_a_bare_hash_heading_keeps_the_delimiter() {
    use crate::doc::Doc;
    use crate::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown("#\n\nalpha beta\n", editor_options()));
    let leaves = doc.text_leaves();
    let paragraph = leaves[leaves.len() - 1];
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: paragraph,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    assert_eq!(
        doc.document.to_markdown(),
        "# alpha beta\n",
        "backspacing a paragraph into a bare `#` heading must keep valid ATX"
    );

    while doc.undo().is_some() {}
    assert_eq!(
        doc.document.to_markdown(),
        "#\n\nalpha beta\n",
        "undo must restore the loaded document verbatim"
    );
    while doc.redo().is_some() {}
    assert_eq!(
        doc.document.to_markdown(),
        "# alpha beta\n",
        "redo must replay the merge onto the delimiter, not into the hashes"
    );

    let mut doc = Doc::new(load_markdown("#\n\nalpha beta\n", editor_options()));
    let heading = doc.text_leaves()[0];
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: heading,
            offset: 0,
        }),
        Command::DeleteForward,
    );
    assert_eq!(
        doc.document.to_markdown(),
        "# alpha beta\n",
        "delete-forward into a bare `#` heading must keep valid ATX"
    );
}

#[test]
fn typing_into_a_bare_hash_heading_undo_restores_the_verbatim_source() {
    use crate::doc::Doc;
    use crate::document::{Caret, Command, PasteIntent, Sel};

    for (load, typed) in [("#\n", "x"), ("###\n", "xyz")] {
        let mut doc = Doc::new(load_markdown(load, editor_options()));
        let heading = doc.text_leaves()[0];
        let _ = doc.apply(
            Sel::collapsed(Caret {
                block: heading,
                offset: 0,
            }),
            Command::Insert { text: typed.into() },
        );
        let hashes = load.trim_end();
        assert_eq!(
            doc.document.to_markdown(),
            format!("{hashes} {typed}\n"),
            "first keystroke must pad a delimiter"
        );
        let _ = doc.undo().expect("undo");
        assert_eq!(
            doc.document.to_markdown(),
            load,
            "undo must restore the loaded document verbatim, space included"
        );
        let _ = doc.redo().expect("redo");
        assert_eq!(
            doc.document.to_markdown(),
            format!("{hashes} {typed}\n"),
            "redo must replay onto the delimiter"
        );
    }

    let mut doc = Doc::new(load_markdown("#\n", editor_options()));
    let heading = doc.text_leaves()[0];
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: heading,
            offset: 0,
        }),
        Command::Paste {
            text: "abc".into(),
            intent: PasteIntent::PlainText,
        },
    );
    assert_eq!(doc.document.to_markdown(), "# abc\n");
    let _ = doc.undo().expect("undo");
    assert_eq!(doc.document.to_markdown(), "#\n");
}

#[test]
fn quote_continuation_prefix_stays_out_of_inline_source() {
    let doc = load_markdown("> a **b\n> c** d\n", editor_options());
    let id = doc.live_id(doc.text_leaves()[0]).expect("live");
    assert_eq!(doc.leaf_source(id), "a **b\nc** d");
    assert_eq!(doc.display(id), "a b\nc d");

    let plain = load_markdown("> a\n> b\n", editor_options());
    let pid = plain.live_id(plain.text_leaves()[0]).expect("live");
    assert_eq!(plain.leaf_source(pid), "a\nb");

    let mid = load_markdown("> x > y\n", editor_options());
    let mid_id = mid.live_id(mid.text_leaves()[0]).expect("live");
    assert_eq!(mid.leaf_source(mid_id), "x > y");
}

#[test]
fn plain_text_paste_splits_on_blank_lines_and_stays_literal() {
    use crate::doc::Doc;
    use crate::document::{Caret, Command, PasteIntent, Sel};

    let mut doc = Doc::new(load_markdown("hello world\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.take_changes();
    let (_, last_block, caret) =
        doc.document
            .paste(leaf, 5..5, " a\n\n**b**\n\n# c", PasteIntent::PlainText);
    assert_eq!(
        doc.document.to_markdown(),
        "hello a\n\n\\*\\*b\\*\\*\n\n\\# c world\n"
    );
    let leaves = doc.text_leaves();
    let last = leaves[leaves.len() - 1];
    assert_eq!(doc.document.text_of(last), Some("# c world"));
    assert_eq!(
        last_block, last,
        "the caret must land in the tail paragraph"
    );
    assert_eq!(caret, 3);

    let mut doc = Doc::new(load_markdown("hello world\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.take_changes();
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 5,
        }),
        Command::Insert {
            text: " a\n\n**b**\n\n# c".into(),
        },
    );
    assert_eq!(
        doc.document.to_markdown(),
        "hello a\n\n\\*\\*b\\*\\*\n\n\\# c world\n"
    );

    let mut doc = Doc::new(load_markdown("x\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.take_changes();
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 1,
        }),
        Command::Insert {
            text: "\n\n\n\nnext".into(),
        },
    );
    assert_eq!(doc.document.to_markdown(), "x\n\nnext\n");

    let mut doc = Doc::new(load_markdown("hello world\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.take_changes();
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 5,
        }),
        Command::Insert {
            text: " a\n\nb".into(),
        },
    );
    assert!(doc.undo().is_some(), "the paste must be undoable");
    assert_eq!(
        doc.document.to_markdown(),
        "hello world\n",
        "undo must remove the grafted paragraphs too"
    );
}

#[test]
fn reprojection_does_not_grow_the_link_table_without_new_links() {
    let mut doc = load_markdown("[go](u) end\n", editor_options());
    let block = doc.text_leaves()[0];
    assert_eq!(doc.links.len(), 1);
    for _ in 0..100 {
        let end = doc.text_of(block).unwrap_or("").len();
        doc.replace_text(block, end..end, "x");
    }
    assert_eq!(
        doc.links.len(),
        1,
        "the link destination did not change, so the table must not grow with reparse count"
    );
    assert_eq!(doc.link_at(doc.live_id(block).unwrap(), 0), Some("u"));
}

#[test]
fn tab_list_hosts_measure_indent_in_columns() {
    for (source, display) in [
        ("1.\t`a\n\t    b`\n", "a     b"),
        ("-\t`a\n\t    b`\n", "a     b"),
        ("1. `a\n     b`\n", "a   b"),
    ] {
        let doc = load_markdown(source, editor_options());
        let leaf = doc.text_leaves()[0];
        let first = doc.text_of(leaf).unwrap().to_string();
        assert_eq!(first, display, "src={source:?}");
        let saved = doc.to_markdown();
        let reloaded = load_markdown(&saved, editor_options());
        let rleaf = reloaded.text_leaves()[0];
        let second = reloaded.text_of(rleaf).unwrap().to_string();
        assert_eq!(first, second, "src={source:?}; saved={saved:?}");
    }
}

#[test]
fn escaped_leading_backtick_on_a_continuation_line_survives_reload() {
    let source = "\\`\\`\\`rust\nfn q() {}\n\\`\\`\\`pha **\n";
    let doc = load_markdown(source, editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(
        doc.leaf_source(id),
        "\\`\\`\\`rust\nfn q() {}\n\\`\\`\\`pha **"
    );
    assert_eq!(doc.display(id), "```rust\nfn q() {}\n```pha **");
    let saved = doc.to_markdown();
    let reloaded = load_markdown(&saved, editor_options());
    let rid = reloaded
        .live_id(reloaded.text_leaves()[0])
        .expect("reloaded");
    assert_eq!(
        reloaded.leaf_source(rid),
        "\\`\\`\\`rust\nfn q() {}\n\\`\\`\\`pha **",
        "saved={saved:?}"
    );
}

#[test]
fn nested_list_inline_code_preserves_content_spaces_on_save() {
    let doc = load_markdown("- outer\n  - `a\n      b`\n", editor_options());
    let leaves = doc.text_leaves();
    let before = doc.text_of(leaves[1]).unwrap().to_string();
    assert_eq!(before, "a   b");
    let markdown = doc.to_markdown();
    let reloaded = load_markdown(&markdown, editor_options());
    let after = reloaded
        .text_of(reloaded.text_leaves()[1])
        .unwrap()
        .to_string();
    assert_eq!(after, before, "saved={markdown:?}");
}
