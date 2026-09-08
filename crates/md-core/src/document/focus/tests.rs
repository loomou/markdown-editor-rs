use super::{
    FocusBias, FocusProjection, FocusQuery, RawConstruct, finish_constructs, focus_s2d_many,
    inline_constructs, project_focus, raw_constructs,
};
use crate::block::BlockId;
use crate::block::BlockKind;
use crate::block::TextEditStrategy;
use crate::document::NodeId;
use crate::document::bind::identity_map;
use crate::document::bind::source_to_display_map;
use crate::document::edit::{Caret, Sel};
use crate::document::{Document, editor_options, load_markdown};
use crate::inline::InlineMarks;
use crate::inline::InlineRun;

fn map(source: &str) -> (String, Vec<InlineRun>, Vec<usize>) {
    let doc = load_markdown(source, editor_options());
    let id = doc.live_id(doc.text_leaves()[0]).expect("leaf");
    (
        doc.display(id).to_string(),
        doc.runs(id).to_vec(),
        source_to_display_map(source, &[]),
    )
}

fn focus_at(
    source: &str,
    d: &str,
    runs: &[InlineRun],
    s2d: &[usize],
    caret: usize,
) -> Option<FocusProjection> {
    project_focus(
        source,
        d,
        runs,
        s2d,
        &raw_constructs(source, &[]),
        FocusQuery {
            caret,
            src_hint: None,
            bias: FocusBias::Neutral,
        },
    )
}

#[test]
fn short_collapsed_mapping_falls_back_to_identity() {
    assert_eq!(focus_s2d_many(&[0], 3, &[]), identity_map(3));
}

#[test]
fn constructs_strong_in_plain() {
    let source = "a**b**c";
    let (_, _, s2d) = map(source);
    let cs = inline_constructs(source, &s2d, &[]);
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].source, 1..6);
    assert_eq!(cs[0].inner, 3..4);
    assert_eq!(cs[0].display, 1..2);
}

#[test]
fn nested_runs_follow_source_offsets_when_the_outer_construct_is_revealed() {
    let source = "**a *b* c**";
    let (display, runs, s2d) = map(source);
    let focused = focus_at(source, &display, &runs, &s2d, "a ".len()).expect("focus");
    assert_eq!(focused.display, source);

    let marks_at = |offset: usize| {
        focused
            .runs
            .iter()
            .find(|run| {
                run.display_range.start as usize <= offset
                    && offset < run.display_range.end as usize
            })
            .map(|run| run.marks)
            .unwrap_or(InlineMarks::NONE)
    };
    let b = source.find('b').expect("b");
    let nested_closer = source[b..].find('*').map(|i| b + i).expect("closer");
    let c = source.find('c').expect("c");
    assert!(marks_at(b).contains(InlineMarks::STRONG));
    assert!(marks_at(b).contains(InlineMarks::EM));
    assert!(!marks_at(nested_closer).contains(InlineMarks::EM));
    assert!(marks_at(c).contains(InlineMarks::STRONG));

    let utf8 = "**a *€* c**";
    let (display, runs, s2d) = map(utf8);
    let focused = focus_at(utf8, &display, &runs, &s2d, "a ".len()).expect("utf8 focus");
    for run in &focused.runs {
        assert!(utf8.is_char_boundary(run.display_range.start as usize));
        assert!(utf8.is_char_boundary(run.display_range.end as usize));
    }
}

#[test]
fn empty_alt_image_map_reaches_display_end() {
    let source = "x ![](u) y";
    let (d, _, s2d) = map(source);
    assert_eq!(d, "x \u{FFFC} y");
    assert_eq!(s2d.last().copied(), Some(d.len()));
}

#[test]
fn revealed_empty_alt_image_drops_placeholder_and_maps_identically() {
    let source = "x ![](u) y";
    let (d, runs, s2d) = map(source);
    let f = focus_at(source, &d, &runs, &s2d, 2).expect("focus");
    assert_eq!(f.display, source);
    assert_eq!(f.s2d, identity_map(source.len()));
    assert!(
        f.runs
            .iter()
            .any(|r| r.marks.is_syntax() && r.display_range == (2u32..4))
    );
    assert!(
        f.runs
            .iter()
            .any(|r| r.marks.is_syntax() && r.display_range == (4u32..8))
    );
    assert!(!f.runs.iter().any(|r| r.marks.is_image()));
}

#[test]
fn revealed_trailing_image_maps_to_display_end() {
    let source = "y ![](u)";
    let (d, runs, s2d) = map(source);
    let f = focus_at(source, &d, &runs, &s2d, 2).expect("focus");
    assert_eq!(f.display, source);
    assert_eq!(f.s2d.last().copied(), Some(f.display.len()));
    assert_eq!(f.s2d, identity_map(source.len()));
}

#[test]
fn revealed_alt_image_expands_whole_construct() {
    let source = "x ![c](u) y";
    let (d, runs, s2d) = map(source);
    assert_eq!(d, "x c y");
    let f = focus_at(source, &d, &runs, &s2d, 2).expect("focus");
    assert_eq!(f.display, "x ![c](u) y");
    assert_eq!(f.s2d.last().copied(), Some(f.display.len()));
}

#[test]
fn caret_past_span_stays_collapsed() {
    let source = "a**b**c";
    let (d, runs, s2d) = map(source);
    assert_eq!(d, "abc");
    assert!(focus_at(source, &d, &runs, &s2d, 3).is_none());
}

#[test]
fn caret_left_edge_reveals_markers() {
    let source = "a**b**c";
    let (d, runs, s2d) = map(source);
    let f = focus_at(source, &d, &runs, &s2d, 1).expect("focus");
    assert_eq!(f.display, "a**b**c");
    assert_eq!(f.caret, 1);
    assert!(f.runs.iter().any(|r| r.marks.contains(InlineMarks::STRONG)
        && r.display_range.start <= 3
        && r.display_range.end >= 4));
    assert!(
        f.runs
            .iter()
            .any(|r| r.marks.is_syntax() && r.display_range == (1u32..3))
    );
}

#[test]
fn caret_right_edge_reveals_markers() {
    let source = "a**b**c";
    let (d, runs, s2d) = map(source);
    let f = focus_at(source, &d, &runs, &s2d, 2).expect("focus");
    assert_eq!(f.display, "a**b**c");
    assert_eq!(f.caret, 6);
}

#[test]
fn source_hint_keeps_caret_inside_markers() {
    let source = "a**b**c";
    let (d, runs, s2d) = map(source);
    let f = project_focus(
        source,
        &d,
        &runs,
        &s2d,
        &raw_constructs(source, &[]),
        FocusQuery {
            caret: 1,
            src_hint: Some(3),
            bias: FocusBias::Neutral,
        },
    )
    .expect("focus");
    assert_eq!(f.display, "a**b**c");
    assert_eq!(f.caret, 3);
}

#[test]
fn source_hint_after_inner_stays_before_closer() {
    let source = "a**bd**c";
    let (d, runs, s2d) = map(source);
    let f = project_focus(
        source,
        &d,
        &runs,
        &s2d,
        &raw_constructs(source, &[]),
        FocusQuery {
            caret: 3,
            src_hint: Some(5),
            bias: FocusBias::Neutral,
        },
    )
    .expect("focus");
    assert_eq!(f.display, "a**bd**c");
    assert_eq!(f.caret, 5);
}

#[test]
fn code_and_emphasis_constructs() {
    for (source, start, end) in [("`a`", 0, 3), ("*b*", 0, 3), ("~~d~~", 0, 5)] {
        let (_, _, s2d) = map(source);
        let cs = inline_constructs(source, &s2d, &[]);
        assert_eq!(cs.len(), 1, "{source}");
        assert_eq!(cs[0].source, start..end, "{source}");
    }
}

#[test]
fn link_construct_reveals_destination() {
    let source = "a[b](u)c";
    let (d, runs, s2d) = map(source);
    assert_eq!(d, "abc");
    let cs = inline_constructs(source, &s2d, &[]);
    assert_eq!(cs.len(), 1);
    let f = focus_at(source, &d, &runs, &s2d, 1).expect("focus");
    assert_eq!(f.display, "a[b](u)c");
    assert!(f.runs.iter().any(|r| r.marks.is_syntax()));
}

#[test]
fn math_construct_reveals_dollars() {
    let source = "a$x$c";
    let (_, _, s2d) = map(source);
    let cs = inline_constructs(source, &s2d, &[]);
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].source, 1..4);
}

#[test]
fn image_construct_reveals_markdown() {
    let source = "a![x](u)c";
    let (d, runs, s2d) = map(source);
    let cs = inline_constructs(source, &s2d, &[]);
    assert_eq!(cs.len(), 1);
    let f = focus_at(source, &d, &runs, &s2d, 1).expect("focus");
    assert!(f.display.contains("![x](u)"));
}

#[test]
fn nested_expands_ancestor_markers() {
    let source = "**a *b* c**";
    let (d, runs, s2d) = map(source);
    let inner = d.find('b').expect("b");
    let f = focus_at(source, &d, &runs, &s2d, inner).expect("focus");
    assert_eq!(f.display, "**a *b* c**");
}

#[test]
fn seam_bias_picks_direction() {
    let source = "**a** *b*";
    let (d, runs, s2d) = map(source);
    let seam = d.find(' ').map(|i| i + 1).unwrap_or(1);
    let right = project_focus(
        source,
        &d,
        &runs,
        &s2d,
        &raw_constructs(source, &[]),
        FocusQuery {
            caret: seam,
            src_hint: None,
            bias: FocusBias::Right,
        },
    )
    .expect("right");
    assert!(right.display.contains("*b*"));
    assert!(!right.display.contains("**a**") || right.span.start > 0);
    let left = project_focus(
        source,
        &d,
        &runs,
        &s2d,
        &raw_constructs(source, &[]),
        FocusQuery {
            caret: seam.saturating_sub(1),
            src_hint: None,
            bias: FocusBias::Left,
        },
    )
    .expect("left");
    assert!(left.display.contains("**a**"));
}

#[test]
fn retarget_range_keeps_constructs_collapsed() {
    let mut doc = load_markdown("**a** x *b*", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let collapsed = doc.display(id).to_string();
    assert_eq!(collapsed, "a x b");
    let (anchor, head) = doc.retarget_inline_focus_range(
        Caret {
            block: leaf,
            offset: 0,
        },
        Caret {
            block: leaf,
            offset: collapsed.len(),
        },
        FocusBias::Neutral,
    );
    assert_eq!(doc.display(id), collapsed);
    assert!(
        doc.runs(id)
            .iter()
            .any(|r| r.marks.contains(InlineMarks::STRONG))
    );
    assert!(
        doc.runs(id)
            .iter()
            .any(|r| r.marks.contains(InlineMarks::EM))
    );
    assert_eq!(anchor.offset, 0);
    assert_eq!(head.offset, collapsed.len());
}

#[test]
fn retarget_range_keeps_reveal_after_caret() {
    let mut doc = load_markdown("a**b**c", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.retarget_inline_focus(Caret {
        block: leaf,
        offset: 1,
    });
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.display(id), "a**b**c");
    let (anchor, head) = doc.retarget_inline_focus_range(
        Caret {
            block: leaf,
            offset: 0,
        },
        Caret {
            block: leaf,
            offset: 6,
        },
        FocusBias::Neutral,
    );
    assert_eq!(doc.display(id), "a**b**c");
    assert_eq!(anchor.offset, 0);
    assert_eq!(head.offset, 6);
}

#[test]
fn retarget_range_keeps_reveal_past_the_construct_and_onto_the_next_leaf() {
    let mut doc = load_markdown("a**b**c\n\nx\n", editor_options());
    let leaves = doc.text_leaves();
    let leaf = leaves[0];
    let next = leaves[1];
    let _ = doc.retarget_inline_focus(Caret {
        block: leaf,
        offset: 1,
    });
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.display(id), "a**b**c");
    let (anchor, head) = doc.retarget_inline_focus_range(
        Caret {
            block: leaf,
            offset: 0,
        },
        Caret {
            block: leaf,
            offset: 7,
        },
        FocusBias::Neutral,
    );
    assert_eq!(doc.display(id), "a**b**c");
    assert_eq!(doc.copy_markdown(Sel { anchor, head }), "a**b**c");
    let (anchor, head) = doc.retarget_inline_focus_range(
        Caret {
            block: leaf,
            offset: 0,
        },
        Caret {
            block: next,
            offset: 1,
        },
        FocusBias::Neutral,
    );
    assert_eq!(doc.display(id), "a**b**c");
    assert_eq!(doc.copy_markdown(Sel { anchor, head }), "a**b**c\n\nx");
}

#[test]
fn retarget_range_collapses_a_reveal_outside_both_endpoints() {
    let mut doc = load_markdown("**a**\n\nb\n\nc\n", editor_options());
    let leaves = doc.text_leaves();
    let revealed = leaves[0];
    let revealed_id = doc.live_id(revealed).expect("live");
    let _ = doc.retarget_inline_focus(Caret {
        block: revealed,
        offset: 0,
    });
    assert_eq!(doc.display(revealed_id), "**a**");

    let (anchor, head) = doc.retarget_inline_focus_range(
        Caret {
            block: leaves[1],
            offset: 0,
        },
        Caret {
            block: leaves[2],
            offset: 1,
        },
        FocusBias::Neutral,
    );

    assert_eq!(doc.display(revealed_id), "a");
    assert!(doc.focus.is_none());
    assert_eq!(anchor.block, leaves[1]);
    assert_eq!(head.block, leaves[2]);
}

fn math_run_start(doc: &Document, id: NodeId, display: bool) -> usize {
    doc.runs(id)
        .iter()
        .find(|r| r.marks.is_math() && r.marks.contains(InlineMarks::MATH_DISPLAY) == display)
        .map(|r| r.display_range.start as usize)
        .expect("math run")
}

#[test]
fn revealed_display_math_enters_block_edit() {
    let mut doc = load_markdown("text\n$$\n\\frac{a}{b}\n$$\n", editor_options());
    let math = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::Math))
        .expect("glued $$ is a math block");
    let _ = doc.retarget_inline_focus(Caret {
        block: math,
        offset: 0,
    });
    assert_eq!(doc.block_edit(), Some(math));
    assert!(
        doc.revealed_math().is_none(),
        "a math block does not go through inline reveal"
    );
}

#[test]
fn revealed_inline_math_does_not_enter_block_edit() {
    let mut doc = load_markdown("x $a$ y", editor_options());
    let block = doc.text_leaves()[0];
    let id = doc.live_id(block).expect("live");
    let off = math_run_start(&doc, id, false);
    let _ = doc.retarget_inline_focus(Caret { block, offset: off });
    let m = doc.revealed_math().expect("revealed");
    assert!(!m.display_math);
    assert_eq!(doc.block_edit(), None);
}

#[test]
fn leaving_display_math_clears_block_edit() {
    let mut doc = load_markdown("text\n$$\n\\frac{a}{b}\n$$\n", editor_options());
    let math = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::Math))
        .expect("math");
    let para = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::Paragraph))
        .expect("para");
    let _ = doc.retarget_inline_focus(Caret {
        block: math,
        offset: 0,
    });
    assert_eq!(doc.block_edit(), Some(math));
    let _ = doc.retarget_inline_focus(Caret {
        block: para,
        offset: 0,
    });
    assert_eq!(doc.block_edit(), None);
    assert!(doc.revealed_math().is_none());
}

fn leaf_of(doc: &Document, kind: BlockKind) -> BlockId {
    doc.text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(kind))
        .unwrap_or_else(|| panic!("no {kind:?}"))
}

fn block_edit_fixtures() -> [(&'static str, BlockKind); 3] {
    [
        (
            "before\n\n```mermaid\nflowchart TD\nA-->B\n```\n\nafter\n",
            BlockKind::Mermaid,
        ),
        ("before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n", BlockKind::Math),
        ("before\n\n![a](u)\n\nafter\n", BlockKind::Image),
    ]
}

#[test]
fn retarget_range_does_not_enter_block_edit() {
    for (md, kind) in block_edit_fixtures() {
        let mut doc = load_markdown(md, editor_options());
        let block = leaf_of(&doc, kind);
        let para = leaf_of(&doc, BlockKind::Paragraph);
        let _ = doc.retarget_inline_focus_range(
            Caret {
                block: para,
                offset: 0,
            },
            Caret { block, offset: 0 },
            FocusBias::Neutral,
        );
        assert_eq!(doc.block_edit(), None, "{kind:?}");
    }
}

#[test]
fn retarget_range_inside_idle_block_does_not_enter() {
    for (md, kind) in block_edit_fixtures() {
        let mut doc = load_markdown(md, editor_options());
        let block = leaf_of(&doc, kind);
        let id = doc.live_id(block).expect("live");
        let end = doc.caret_text(id).len();
        let _ = doc.retarget_inline_focus_range(
            Caret { block, offset: 0 },
            Caret { block, offset: end },
            FocusBias::Neutral,
        );
        assert_eq!(doc.block_edit(), None, "{kind:?}");
    }
}

#[test]
fn retarget_range_inside_active_block_edit_keeps_it() {
    for (md, kind) in block_edit_fixtures() {
        let mut doc = load_markdown(md, editor_options());
        let block = leaf_of(&doc, kind);
        let _ = doc.retarget_inline_focus(Caret { block, offset: 0 });
        assert_eq!(doc.block_edit(), Some(block), "{kind:?}");
        let id = doc.live_id(block).expect("live");
        let end = doc.caret_text(id).len();
        let _ = doc.retarget_inline_focus_range(
            Caret { block, offset: 0 },
            Caret { block, offset: end },
            FocusBias::Neutral,
        );
        assert_eq!(doc.block_edit(), Some(block), "{kind:?}");
    }
}

#[test]
fn retarget_range_leaving_active_block_edit_clears_it() {
    for (md, kind) in block_edit_fixtures() {
        let mut doc = load_markdown(md, editor_options());
        let block = leaf_of(&doc, kind);
        let para = leaf_of(&doc, BlockKind::Paragraph);
        let _ = doc.retarget_inline_focus(Caret { block, offset: 0 });
        let _ = doc.retarget_inline_focus_range(
            Caret { block, offset: 0 },
            Caret {
                block: para,
                offset: 0,
            },
            FocusBias::Neutral,
        );
        assert_eq!(doc.block_edit(), None, "{kind:?}");
    }
}

#[test]
fn park_caret_does_not_enter_block_edit() {
    for (md, kind) in block_edit_fixtures() {
        let mut doc = load_markdown(md, editor_options());
        let block = leaf_of(&doc, kind);
        let _ = doc.retarget_inline_focus_without_block_edit(
            Caret { block, offset: 0 },
            FocusBias::Neutral,
        );
        assert_eq!(doc.block_edit(), None, "{kind:?}");
    }
}

#[test]
fn construct_cache_follows_load_edit_and_merge() {
    let mut doc = load_markdown("*a* b\n\nz\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(
        doc.recorded_constructs(id),
        Some(
            &[RawConstruct {
                source: 0..3,
                inner: 1..2
            }][..]
        )
    );

    let _ = doc.replace_text(leaf, 3..3, "c");
    let id = doc.live_id(leaf).expect("live after edit");
    let expected = raw_constructs(doc.leaf_source(id), &[]);
    assert_eq!(doc.recorded_constructs(id), Some(expected.as_slice()));

    let tail = doc.text_leaves()[1];
    let _ = doc.merge_into_prev(tail);
    let id = doc.live_id(leaf).expect("live after merge");
    let expected = raw_constructs(doc.leaf_source(id), &[]);
    assert_eq!(doc.recorded_constructs(id), Some(expected.as_slice()));
    assert_eq!(doc.leaf_source(id), "*a* bcz");

    let _ = doc.retarget_inline_focus(Caret {
        block: leaf,
        offset: 0,
    });
    let id = doc.live_id(leaf).expect("live after refocus");
    let expected = raw_constructs(doc.leaf_source(id), &[]);
    assert_eq!(doc.recorded_constructs(id), Some(expected.as_slice()));
    assert_eq!(doc.display(id), "*a* bcz");
}

#[test]
fn recorded_constructs_match_standalone_parsing() {
    for source in [
        "a**b**c",
        "*a* b",
        "**a *b* c**",
        "~~s~~ x",
        "a[b](u)c",
        "a![x](u)c",
        "x ![](u) y",
        "a `c` d",
        "a $m$ d",
        "soft *a\nb* break",
        "plain text only",
        "# head *x*",
        "- list *item*",
        "> quote *x*",
        "| *a* | b |\n| --- | --- |\n| c | *d* |\n",
        "text [ref][d]\n\n[d]: /u\n",
    ] {
        let doc = load_markdown(source, editor_options());
        for block in doc.text_leaves() {
            let Some(id) = doc.live_id(block) else {
                continue;
            };
            if !doc
                .kind(block)
                .is_some_and(|k| k.text_edit_strategy() == TextEditStrategy::Phrasing)
            {
                continue;
            }
            let src = doc.leaf_source(id).to_string();
            let s2d = doc.collapsed_s2d(id);
            let recorded = doc.recorded_constructs(id).unwrap_or_else(|| {
                panic!("cache must be fresh right after load: source={source:?}")
            });
            assert_eq!(
                finish_constructs(recorded, &s2d),
                inline_constructs(&src, &s2d, &doc.reference_definitions),
                "source={source:?} leaf={src:?}"
            );
        }
    }
}

#[test]
fn reference_link_construct_survives_on_both_paths() {
    let doc = load_markdown("text [ref][d]\n\n[d]: /u\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.leaf_source(id), "text [ref][d]");
    let recorded = doc.recorded_constructs(id).expect("recorded");
    assert!(
        !recorded.is_empty(),
        "a whole-document parse sees the definition, so Link records it"
    );
    let s2d = doc.collapsed_s2d(id);
    assert_eq!(
        finish_constructs(recorded, &s2d),
        inline_constructs(doc.leaf_source(id), &s2d, &doc.reference_definitions)
    );
    assert_eq!(doc.display(id), "text ref");
    assert_eq!(s2d.last().copied(), Some(doc.display(id).len()));
}

#[test]
fn emphasis_straddling_display_math_stays_in_the_paragraph() {
    let mut doc = load_markdown("*a\n$$x$$\nb*\n", editor_options());
    let para = doc.text_leaves()[0];
    let id = doc.live_id(para).expect("live");
    let _ = doc.retarget_inline_focus(Caret {
        block: para,
        offset: 0,
    });
    assert!(
        doc.focus.is_some(),
        "the construct is intact, so the paragraph start reveals as usual"
    );
    assert_eq!(doc.display(id), "*a\n$$x$$\nb*");
}

#[test]
fn entity_boundaries_stay_atomic_when_typing() {
    use crate::doc::Doc;
    use crate::document::edit::Command;

    let mut doc = Doc::new(load_markdown("&#x4E2D;tail\n", editor_options()));
    let block = doc.text_leaves()[0];
    assert_eq!(doc.collapsed_text(block), Some("中tail"));
    let caret = doc
        .document
        .retarget_inline_focus_biased(Caret { block, offset: 0 }, FocusBias::Neutral);
    let _ = doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(
        doc.document.to_markdown(),
        "X&#x4E2D;tail\n",
        "the insert must land before the entity, never splitting the encoding"
    );

    let mut doc = Doc::new(load_markdown("&#x4E2D;tail\n", editor_options()));
    let block = doc.text_leaves()[0];
    let caret = doc
        .document
        .retarget_inline_focus_biased(Caret { block, offset: 3 }, FocusBias::Neutral);
    let _ = doc.apply(Sel::collapsed(caret), Command::Insert { text: "Y".into() });
    assert_eq!(
        doc.document.to_markdown(),
        "&#x4E2D;Ytail\n",
        "the insert must land after the entity, never splitting the encoding"
    );
}

#[test]
fn container_prefixes_do_not_blank_cross_line_constructs() {
    use crate::doc::Doc;

    for (source, revealed) in [
        ("> **alpha\n> beta**\n", "**alpha\nbeta**"),
        ("- **alpha\n  beta**\n", "**alpha\nbeta**"),
    ] {
        let mut doc = Doc::new(load_markdown(source, editor_options()));
        let block = doc.text_leaves()[0];
        assert_eq!(doc.collapsed_text(block), Some("alpha\nbeta"));
        doc.document
            .retarget_inline_focus_biased(Caret { block, offset: 1 }, FocusBias::Neutral);
        let id = doc.document.live_id(block).expect("live");
        assert_eq!(
            doc.document.text_of(id.index),
            Some(revealed),
            "source={source:?}: a construct spanning lines must still reveal"
        );
    }
}

#[test]
fn nested_math_preview_targets_the_inner_construct() {
    for (source, caret_off) in [("**$x$**", 1usize), ("**before $x$**", 8usize)] {
        let mut doc = load_markdown(source, editor_options());
        let block = doc.text_leaves()[0];
        let _ = doc.retarget_inline_focus(Caret {
            block,
            offset: caret_off,
        });
        let m = doc.revealed_math().expect("revealed");
        assert_eq!(
            m.latex, "x",
            "source={source:?}: LaTeX must not carry the `$` delimiters"
        );
        assert!(!m.display_math);
        let id = doc.live_id(block).expect("live");
        assert_eq!(
            &doc.display(id)[m.display.clone()],
            "$x$",
            "source={source:?}: the reveal range must cover the formula inside the revealed display"
        );
    }
}
