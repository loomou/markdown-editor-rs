use super::support::{caret, has_mark, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn typing_closed_code_span_projects_marks() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let mid = type_chars(&mut doc, caret(leaf, 0), "`a");
    assert_eq!(doc.text_of(leaf).unwrap(), "`a");
    assert!(!has_mark(&doc, leaf, InlineMarks::CODE));
    let out = type_chars(&mut doc, mid, "`");
    assert_eq!(doc.text_of(leaf).unwrap(), "`a`");
    assert!(has_mark(&doc, leaf, InlineMarks::CODE));
    assert_eq!(out.offset, 3);
    assert_eq!(doc.leaf_source(id), "`a`");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    let back = apply(&mut doc, Sel::collapsed(out), Command::DeleteBackward);
    assert!(doc.text_of(leaf).unwrap().contains('`'));
    assert!(!has_mark(&doc, leaf, InlineMarks::CODE));
    assert_eq!(back.block, leaf);
}

#[test]
fn typing_closed_emphasis_strong_and_strike() {
    use crate::inline::InlineMarks;
    for (typed, mark) in [
        ("*b*", InlineMarks::EM),
        ("**c**", InlineMarks::STRONG),
        ("~~d~~", InlineMarks::STRIKE),
    ] {
        let mut doc = load_markdown("", editor_options());
        let leaf = doc.text_leaves()[0];
        let out = type_chars(&mut doc, caret(leaf, 0), typed);
        assert_eq!(doc.text_of(leaf).unwrap(), typed, "{typed}");
        assert!(has_mark(&doc, leaf, mark), "{typed}");
        assert_eq!(out.offset, typed.len(), "{typed}");
    }
}

#[test]
fn cross_block_soft_break_emits_the_revealed_leaf_collapse() {
    use crate::document::DocChange;

    let mut doc = load_markdown("alpha **bold** beta\n\nnext\n", editor_options());
    let leaves = doc.text_leaves();
    let first = leaves[0];
    let second = leaves[1];
    let anchor = doc.retarget_inline_focus(caret(first, 8));
    let _ = doc.take_changes();

    let _ = apply(
        &mut doc,
        Sel {
            anchor,
            head: caret(second, 2),
        },
        Command::SoftBreak,
    );
    let first_id = doc.live_id(first).expect("surviving first leaf");
    let changes = doc.take_changes();
    assert!(changes.changes.iter().any(|change| {
        matches!(
            change,
            DocChange::TextChanged {
                node,
                deleted,
                inserted,
                ..
            } if *node == first_id && deleted.is_empty() && inserted.is_empty()
        )
    }));
}

#[test]
fn typing_code_span_inside_list_item_keeps_item() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::WrapList {
            ordered: false,
            task: None,
        },
    );
    let out = type_chars(&mut doc, at, "`a`");
    assert_eq!(doc.text_of(leaf).unwrap(), "`a`");
    assert!(has_mark(&doc, leaf, InlineMarks::CODE));
    assert_eq!(out.offset, 3);
    let id = doc.live_id(leaf).expect("live");
    let parent = doc.arena.get(id).and_then(|n| n.parent).expect("parent");
    assert_eq!(
        doc.arena.get(parent).map(|n| n.kind),
        Some(BlockKind::ListItem)
    );
}

#[test]
fn cold_code_span_insert_at_end_lands_outside_the_span() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("`d`\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.text_of(leaf).unwrap(), "d");
    assert!(has_mark(&doc, leaf, InlineMarks::CODE));
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 1)),
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "dx");
    assert_eq!(code_bytes(&doc, leaf), 1);
    assert_eq!(out.offset, 2);
    assert_eq!(doc.leaf_source(id), "`d`x");
}

#[test]
fn typing_a_then_one_space_keeps_inline_display_and_caret_in_sync() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];

    let after_a = type_chars(&mut doc, caret(leaf, 0), "a");
    assert_eq!(doc.text_of(leaf), Some("a"));
    assert_eq!(doc.leaf_source(doc.live_id(leaf).expect("live")), "a");
    assert_eq!(after_a, caret(leaf, 1));

    let after_space = type_chars(&mut doc, after_a, " ");
    assert_eq!(doc.text_of(leaf), Some("a "));
    assert_eq!(doc.leaf_source(doc.live_id(leaf).expect("live")), "a ");
    assert_eq!(after_space, caret(leaf, 2));
}

#[test]
fn typing_a_then_space_extends_the_inline_run_over_visible_whitespace() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "a");
    let _ = type_chars(&mut doc, at, " ");
    let id = doc.live_id(leaf).expect("live");

    assert_eq!(doc.text_of(leaf), Some("a "));
    assert_eq!(
        doc.runs(id)
            .iter()
            .map(|run| run.display_range.clone())
            .collect::<Vec<_>>(),
        vec![0..2]
    );
}

#[test]
fn typing_space_after_closed_code_span_lands_outside_and_stays_visible() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let closed = type_chars(&mut doc, caret(leaf, 0), "`a`");
    assert_eq!(doc.text_of(leaf).unwrap(), "`a`");
    assert_eq!(closed.offset, 3);

    let after_space = type_chars(&mut doc, closed, " ");
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.text_of(leaf).unwrap(), "a ");
    assert_eq!(doc.leaf_source(id), "`a` ");
    assert_eq!(after_space.offset, 2);
    assert_eq!(code_bytes(&doc, leaf), 1);

    let after_b = type_chars(&mut doc, after_space, "b");
    assert_eq!(doc.leaf_source(id), "`a` b");
    assert_eq!(doc.text_of(leaf).unwrap(), "a b");
    assert_eq!(after_b.offset, 3);
    assert_eq!(code_bytes(&doc, leaf), 1);
}

fn code_bytes(doc: &crate::document::Document, leaf: u32) -> u32 {
    use crate::inline::InlineMarks;
    let id = doc.live_id(leaf).expect("live");
    doc.runs(id)
        .iter()
        .filter(|r| r.marks.contains(InlineMarks::CODE))
        .map(|r| r.display_range.end.saturating_sub(r.display_range.start))
        .sum()
}

#[test]
fn typing_two_spaces_preserves_both_visible_spaces_and_byte_offsets() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let after_a = type_chars(&mut doc, caret(leaf, 0), "a");
    let after_one_space = type_chars(&mut doc, after_a, " ");
    let after_two_spaces = type_chars(&mut doc, after_one_space, " ");

    assert_eq!(doc.text_of(leaf), Some("a  "));
    assert_eq!(doc.leaf_source(doc.live_id(leaf).expect("live")), "a  ");
    assert_eq!(after_two_spaces, caret(leaf, 3));
}

#[test]
fn typing_after_spaces_starts_after_the_visible_whitespace() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let after_a = type_chars(&mut doc, caret(leaf, 0), "a");
    let after_spaces = type_chars(&mut doc, after_a, "  ");
    let after_b = type_chars(&mut doc, after_spaces, "b");

    assert_eq!(doc.text_of(leaf), Some("a  b"));
    assert_eq!(after_b, caret(leaf, 4));
}

#[test]
fn caret_away_from_strong_stays_rich() {
    let mut doc = load_markdown("a**b**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.text_of(leaf).unwrap(), "abc");
    assert!(has_mark(&doc, leaf, crate::inline::InlineMarks::STRONG));
    let out = doc.retarget_inline_focus(caret(leaf, 3));
    assert_eq!(out.offset, 3);
    assert_eq!(doc.text_of(leaf).unwrap(), "abc");
    assert_eq!(doc.collapsed_display(id), "abc");
}

#[test]
fn caret_left_of_strong_reveals_markers() {
    let mut doc = load_markdown("a**b**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = doc.retarget_inline_focus(caret(leaf, 1));
    assert_eq!(doc.text_of(leaf).unwrap(), "a**b**c");
    assert_eq!(out.offset, 1);
    assert!(has_mark(&doc, leaf, crate::inline::InlineMarks::STRONG));
}

#[test]
fn caret_right_of_strong_reveals_markers() {
    let mut doc = load_markdown("a**b**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = doc.retarget_inline_focus(caret(leaf, 2));
    assert_eq!(doc.text_of(leaf).unwrap(), "a**b**c");
    assert_eq!(out.offset, 6);
}

#[test]
fn leaving_strong_collapses_markers() {
    let mut doc = load_markdown("a**b**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.retarget_inline_focus(caret(leaf, 1));
    assert_eq!(doc.text_of(leaf).unwrap(), "a**b**c");
    let out = doc.retarget_inline_focus(caret(leaf, 7));
    assert_eq!(doc.text_of(leaf).unwrap(), "abc");
    assert_eq!(out.offset, 3);
}

#[test]
fn insert_inside_strong_keeps_caret_before_closer() {
    let mut doc = load_markdown("a**b**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.retarget_inline_focus(caret(leaf, 2));
    let at = doc.retarget_inline_focus(caret(leaf, 4));
    assert_eq!(doc.text_of(leaf).unwrap(), "a**b**c");
    assert_eq!(at.offset, 4);
    let out = apply(
        &mut doc,
        Sel::collapsed(at),
        Command::Insert { text: "d".into() },
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "a**bd**c");
    assert_eq!(out.offset, 5);
}

#[test]
fn delete_inside_strong_keeps_caret_before_closer() {
    let mut doc = load_markdown("a**bd**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.retarget_inline_focus(caret(leaf, 3));
    let at = doc.retarget_inline_focus(caret(leaf, 5));
    assert_eq!(doc.text_of(leaf).unwrap(), "a**bd**c");
    assert_eq!(at.offset, 5);
    let out = apply(&mut doc, Sel::collapsed(at), Command::DeleteBackward);
    assert_eq!(doc.text_of(leaf).unwrap(), "a**b**c");
    assert_eq!(out.offset, 4);
}

#[test]
fn typing_past_closed_strong_collapses() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "a**b**");
    assert_eq!(doc.text_of(leaf).unwrap(), "a**b**");
    let out = type_chars(&mut doc, at, "c");
    assert_eq!(doc.text_of(leaf).unwrap(), "abc");
    assert_eq!(out.offset, 3);
    assert!(has_mark(&doc, leaf, crate::inline::InlineMarks::STRONG));
}

#[test]
fn source_range_survives_binding_and_cold_load() {
    use crate::inline::InlineMarks;
    let doc = load_markdown("a**b**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let run = doc
        .runs(id)
        .iter()
        .find(|r| r.marks.contains(InlineMarks::STRONG))
        .expect("strong");
    assert_eq!(run.source_range, Some(3..4));
}

#[test]
fn retarget_range_keeps_touched_strong_collapsed() {
    let mut doc = load_markdown("a**b**c\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let (anchor, head) = doc.retarget_inline_focus_range(
        caret(leaf, 0),
        caret(leaf, 2),
        crate::document::FocusBias::Neutral,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "abc");
    assert_eq!(anchor.offset, 0);
    assert_eq!(head.offset, 2);
    assert!(has_mark(&doc, leaf, crate::inline::InlineMarks::STRONG));
}

#[test]
fn retarget_link_reveals_markdown() {
    let mut doc = load_markdown("see [b](u) now\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = doc.retarget_inline_focus(caret(leaf, 4));
    assert!(doc.text_of(leaf).unwrap().contains("[b](u)"));
    assert!(out.offset >= 4);
}

#[test]
fn retarget_inline_math_falls_back_to_source_text() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("$a^2$\n", editor_options());
    let leaf = doc.text_leaves()[0];
    assert_eq!(doc.text_of(leaf).unwrap(), "a^2");
    assert!(has_mark(&doc, leaf, InlineMarks::MATH_INLINE));

    let _ = doc.retarget_inline_focus(caret(leaf, 3));
    assert_eq!(doc.text_of(leaf).unwrap(), "$a^2$");

    assert!(!has_mark(&doc, leaf, InlineMarks::MATH_INLINE));
    assert!(has_mark(&doc, leaf, InlineMarks::SYNTAX));
}

#[test]
fn leaving_inline_math_restores_the_formula() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("$a^2$ x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.retarget_inline_focus(caret(leaf, 3));
    assert_eq!(doc.text_of(leaf).unwrap(), "$a^2$ x");
    let out = doc.retarget_inline_focus(caret(leaf, 7));
    assert_eq!(doc.text_of(leaf).unwrap(), "a^2 x");
    assert_eq!(out.offset, 5);
    assert!(has_mark(&doc, leaf, InlineMarks::MATH_INLINE));
}

#[test]
fn backspace_in_revealed_empty_alt_image_deletes_at_caret() {
    for (at, want) in [
        (3usize, "x [](u) y"),
        (4, "x !](u) y"),
        (5, "x ![(u) y"),
        (8, "x ![](u y"),
    ] {
        let mut doc = load_markdown("x ![](u) y\n", editor_options());
        let leaf = doc.text_leaves()[0];
        let _ = doc.retarget_inline_focus(caret(leaf, 2));
        assert_eq!(doc.text_of(leaf).unwrap(), "x ![](u) y", "at={at}");
        let _ = apply(
            &mut doc,
            Sel::collapsed(caret(leaf, at)),
            Command::DeleteBackward,
        );
        let id = doc.live_id(leaf).expect("live");
        assert_eq!(doc.leaf_source(id), want, "at={at}");
    }
}

#[test]
fn backspace_in_revealed_trailing_image_deletes_at_caret() {
    for (at, want) in [
        (15usize, "y ![](L:/a/b/1.png)"),
        (20, "y ![](L:/a/b/14.png"),
        (5, "y ![(L:/a/b/14.png)"),
    ] {
        let mut doc = load_markdown("y ![](L:/a/b/14.png)\n", editor_options());
        let leaf = doc.text_leaves()[0];
        let _ = doc.retarget_inline_focus(caret(leaf, 2));
        assert_eq!(
            doc.text_of(leaf).unwrap(),
            "y ![](L:/a/b/14.png)",
            "at={at}"
        );
        let _ = apply(
            &mut doc,
            Sel::collapsed(caret(leaf, at)),
            Command::DeleteBackward,
        );
        let id = doc.live_id(leaf).expect("live");
        assert_eq!(doc.leaf_source(id), want, "at={at}");
    }
}

#[test]
fn retarget_image_falls_back_to_source_text() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("a![x](u)b\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    assert!(has_mark(&doc, leaf, InlineMarks::IMAGE));

    let _ = doc.retarget_inline_focus(caret(leaf, 1));
    assert!(doc.text_of(leaf).unwrap().contains("![x](u)"));
    assert!(!has_mark(&doc, leaf, InlineMarks::IMAGE));

    assert!(doc.runs(id).iter().all(|r| r.link.is_none()));
}

#[test]
fn arrow_walks_out_of_revealed_empty_alt_image() {
    use crate::document::chars::next_char_boundary;
    use crate::document::focus::FocusBias;

    for source in ["![](u)s\n", "![](u) s\n", "a![](u)s\n"] {
        let mut doc = load_markdown(source, editor_options());
        let leaf = doc.text_leaves()[0];

        let enter = doc
            .text_of(leaf)
            .expect("text")
            .find('\u{FFFC}')
            .expect("placeholder");
        let mut at = doc.retarget_inline_focus(caret(leaf, enter));

        let want = source.trim_end_matches('\n');
        assert_eq!(doc.text_of(leaf).unwrap(), want, "{source:?}");

        let mut steps = 0;
        loop {
            let text = doc.text_of(at.block).expect("text").to_string();
            if at.offset >= text.len() {
                break;
            }
            let next = next_char_boundary(&text, at.offset);
            let out = doc.retarget_inline_focus_biased(caret(at.block, next), FocusBias::Right);
            assert_ne!(
                out, at,
                "{source:?} got stuck at offset={} text={text:?}",
                at.offset
            );
            at = out;
            steps += 1;
            assert!(steps < 32, "{source:?} steps ran away");
        }

        assert_eq!(at.offset, doc.text_of(at.block).expect("text").len());
    }
}

#[test]
fn editing_a_second_block_bumps_the_revealed_one() {
    let mut doc = load_markdown("a **b** c\n\ntail\n", editor_options());
    let a = doc.text_leaves()[0];
    let tail = doc.text_leaves()[1];

    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(a, 2)),
        Command::Insert { text: "x".into() },
    );
    assert_eq!(doc.text_of(a).unwrap(), "a **xb** c");
    let revealed_rev = doc
        .live_id(a)
        .and_then(|id| doc.arena.get(id))
        .expect("live")
        .content_revision;

    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(tail, 4)),
        Command::Insert { text: "x".into() },
    );
    assert_eq!(
        doc.text_of(a).unwrap(),
        "a xb c",
        "the projection is restored after the focus is lost"
    );
    let unfocused_rev = doc
        .live_id(a)
        .and_then(|id| doc.arena.get(id))
        .expect("live")
        .content_revision;
    assert!(
        unfocused_rev > revealed_rev,
        "blur must be booked: revealed and unfocused text generations share one \
         shaping-cache key, and this edit is what keeps them apart \
         (revealed {revealed_rev} -> unfocused {unfocused_rev})"
    );
}
