use super::support::{flow_metrics, heading_spacing_theme, kind_count, layout, spacing_theme};
use crate::compose::{compose, sync_layout};
use md_core::document::{PasteIntent, editor_options, load_markdown};

#[test]
fn split_sync_matches_full_compose() {
    let mut doc = load_markdown("hello world\n\nnext\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let leaf = doc.text_leaves()[0];
    let changes = doc.split_leaf(leaf, 5).0;
    let replaced = sync_layout(&mut tree, &doc, &changes, &layout);
    assert!(!replaced);
    let cold = compose(&doc, &layout);
    assert_eq!(tree.nodes.len(), cold.nodes.len());
    for (id, node) in &cold.nodes {
        let hot = tree.nodes.get(id).expect("hot node");
        assert_eq!(hot.kind, node.kind);
        assert_eq!(tree.style_of(hot), cold.style_of(node));
        assert_eq!(
            tree.intern.text(hot.text_id),
            cold.intern.text(node.text_id)
        );
        assert_eq!(hot.content_revision, node.content_revision);
    }
}

#[test]
fn text_patch_keeps_published_tree_snapshot() {
    use md_core::document::{Caret, Command, Sel, apply};
    use std::rc::Rc;

    let mut doc = load_markdown("before\n", editor_options());
    let layout = layout();
    let tree = Rc::new(compose(&doc, &layout));
    let _ = doc.take_changes();
    let old_tree = Rc::clone(&tree);
    let leaf = doc.text_leaves()[0];

    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert {
            text: "after ".into(),
        },
    );
    let changes = doc.take_changes();
    let mut current = tree;
    assert!(!sync_layout(
        Rc::make_mut(&mut current),
        &doc,
        &changes,
        &layout
    ));

    let id = crate::box_tree::LayoutBoxId::frame(leaf);
    assert_eq!(old_tree.text(id), "before");
    assert_eq!(current.text(id), "after before");
}

#[test]
fn removed_subtree_releases_only_its_text_slot() {
    use md_core::document::{Caret, Command, Sel, apply};

    let mut doc = load_markdown("first\n\nsecond\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let _ = doc.take_changes();
    let leaves = doc.text_leaves();
    let survivor = crate::box_tree::LayoutBoxId::frame(leaves[1]);
    let before = tree.get(survivor).text_id().expect("second has text");

    let _ = apply(
        &mut doc,
        Sel {
            anchor: Caret {
                block: leaves[0],
                offset: 0,
            },
            head: Caret {
                block: leaves[0],
                offset: 5,
            },
        },
        Command::DeleteBackward,
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    assert_eq!(tree.get(survivor).text_id(), Some(before));
    assert_eq!(tree.text(survivor), "second");
    assert_eq!(tree.intern().len(), 1);
}

#[test]
fn quote_split_retargets_lead_top_margin() {
    let theme = spacing_theme(|kind| match kind {
        md_core::block::BlockKind::Paragraph => 20.0,
        md_core::block::BlockKind::BlockQuote => 32.0,
        _ => 0.0,
    });
    let mut doc = load_markdown("> hello world\n", editor_options());
    let mut tree = compose(&doc, &theme);
    let leaf = doc.text_leaves()[0];
    let changes = doc.split_leaf(leaf, 5).0;
    let replaced = sync_layout(&mut tree, &doc, &changes, &theme);
    assert!(!replaced);
    let cold = compose(&doc, &theme);
    let tops = |tree: &crate::box_tree::BoxTree| -> Vec<md_core::Px> {
        let quote = tree
            .nodes
            .values()
            .find(|n| {
                n.kind == md_core::block::BlockKind::BlockQuote
                    && n.id.role == crate::box_tree::BoxRole::Frame
            })
            .expect("quote");
        let crate::box_tree::BoxChildren::Vertical(kids) = &quote.children else {
            panic!("quote should stack paragraphs");
        };
        kids.iter()
            .filter_map(|id| tree.nodes.get(id))
            .filter(|n| n.kind == md_core::block::BlockKind::Paragraph)
            .map(|n| tree.style_of(n).margin.top)
            .collect()
    };
    assert_eq!(tops(&tree), vec![0.0, 0.0]);
    assert_eq!(tops(&tree), tops(&cold));
}

#[test]
fn attrs_changed_patches_task_extra() {
    use crate::box_tree::{BoxRole, LayoutBoxId};
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, apply};
    let mut doc = load_markdown("- a\n", editor_options());
    let item = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::ListItem))
        .expect("item");
    let leaf = doc.text_leaves()[0];
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let _ = doc.take_changes();
    let pad = |tree: &crate::box_tree::BoxTree| {
        tree.nodes
            .values()
            .find(|n| n.kind == BlockKind::List && n.id.role == BoxRole::Frame)
            .map(|n| tree.style_of(n).padding.left)
            .expect("list pad")
    };
    let before_pad = pad(&tree);
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::ToggleTask,
    );
    let changes = doc.take_changes();
    assert!(!changes.is_structural());
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);
    assert_eq!(
        tree.nodes
            .get(&LayoutBoxId::frame(item.index))
            .and_then(|n| n.extra.task_checked()),
        Some(false)
    );
    assert!(pad(&tree) > before_pad);
    let cold = compose(&doc, &layout);
    assert_eq!(
        tree.nodes
            .get(&LayoutBoxId::frame(item.index))
            .map(|n| n.extra),
        cold.nodes
            .get(&LayoutBoxId::frame(item.index))
            .map(|n| n.extra)
    );
    assert_eq!(pad(&tree), pad(&cold));
}

#[test]
fn replayed_attrs_sync_content_identity_for_frame_and_chrome() {
    use crate::box_tree::LayoutBoxId;
    use md_core::block::BlockKind;
    use md_core::doc::Doc;
    use md_core::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown("- a\n", editor_options()));
    let item = doc
        .document
        .preorder()
        .into_iter()
        .find(|&id| doc.document.arena.get(id).map(|node| node.kind) == Some(BlockKind::ListItem))
        .expect("list item");
    let leaf = doc.text_leaves()[0];
    let layout = layout();
    let mut tree = compose(&doc.document, &layout);

    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::ToggleTask,
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc.document, &changes, &layout);

    let _ = doc.undo().expect("undo task conversion");
    let replayed = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc.document, &replayed, &layout);
    let cold = compose(&doc.document, &layout);

    let frame = LayoutBoxId::frame(item.index);
    let chrome = LayoutBoxId::chrome(BlockKind::ListItem, item.index).expect("list item slot");
    for id in [frame, chrome] {
        assert_eq!(
            tree.nodes.get(&id).map(|node| node.content_revision),
            cold.nodes.get(&id).map(|node| node.content_revision),
            "revision for {id:?}"
        );
        assert_eq!(
            tree.nodes.get(&id).map(|node| node.content_generation),
            cold.nodes.get(&id).map(|node| node.content_generation),
            "generation for {id:?}"
        );
    }
}

#[test]
fn toggling_one_list_does_not_restyle_its_twin() {
    use crate::box_tree::LayoutBoxId;
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, apply};

    let mut doc = load_markdown("- a\n\nbetween\n\n- b\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let lists: Vec<LayoutBoxId> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .map(|id| LayoutBoxId::frame(id.index))
        .collect();
    assert_eq!(lists.len(), 2, "the fixture needs two parallel lists");
    assert_eq!(
        tree.get(lists[0]).style_id,
        tree.get(lists[1]).style_id,
        "the two lists start out sharing one resident style, or this case proves nothing about aliasing"
    );
    let untouched_before = *tree.style(lists[1]);

    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::ToggleTask,
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    assert!(
        tree.style(lists[0]).padding.left > untouched_before.padding.left,
        "checking the task must widen the first list's gutter"
    );
    assert_eq!(
        *tree.style(lists[1]),
        untouched_before,
        "the other list was not edited, so its style must not change"
    );
}

#[test]
fn attrs_changed_patches_task_done_type_slot() {
    use crate::box_tree::{BoxRole, TypeSlot};
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, apply};
    let mut doc = load_markdown("- [ ] a\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let para_slot = |tree: &crate::box_tree::BoxTree| {
        tree.nodes
            .values()
            .find(|n| n.kind == BlockKind::Paragraph && n.id.role == BoxRole::Frame)
            .map(|n| n.type_slot)
            .expect("para")
    };
    assert_eq!(para_slot(&tree), TypeSlot::FromKind);
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::ToggleTask,
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);
    assert_eq!(para_slot(&tree), TypeSlot::TaskDone);
    let cold = compose(&doc, &layout);
    assert_eq!(para_slot(&tree), para_slot(&cold));
}

#[test]
fn independent_fragment_paste_matches_cold_compose() {
    let mut doc = load_markdown("hello\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let leaf = doc.text_leaves()[0];
    let (changes, _, _) = doc.paste(
        leaf,
        5..5,
        "# title\n\npara\n",
        PasteIntent::IndependentFragment,
    );
    assert!(changes.is_structural());
    let replaced = sync_layout(&mut tree, &doc, &changes, &layout);
    assert!(!replaced);
    let cold = compose(&doc, &layout);
    assert_eq!(tree.nodes.len(), cold.nodes.len());
    for (id, node) in &cold.nodes {
        let hot = tree.nodes.get(id).expect("hot node");
        assert_eq!(hot.kind, node.kind);
        assert_eq!(tree.style_of(hot), cold.style_of(node));
        assert_eq!(
            tree.intern.text(hot.text_id),
            cold.intern.text(node.text_id)
        );
        assert_eq!(hot.id.role, node.id.role);
    }
    assert!(kind_count(&doc, md_core::block::BlockKind::Heading(1)) >= 1);
}

#[test]
fn atx_heading_keeps_doc_lead_zero() {
    use crate::box_tree::BoxRole;
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, apply};
    let layout = spacing_theme(|kind| match kind {
        BlockKind::Paragraph => 20.0,
        BlockKind::Heading(2) => 48.0,
        _ => 0.0,
    })
    .with_flow_metrics(flow_metrics(24.0));
    let mut doc = load_markdown("", editor_options());
    let mut tree = compose(&doc, &layout);
    let leaf = doc.text_leaves()[0];
    let top = |tree: &crate::box_tree::BoxTree, kind: BlockKind| {
        tree.nodes
            .values()
            .find(|n| n.kind == kind && n.id.role == BoxRole::Frame)
            .map(|n| tree.style_of(n).margin.top)
            .expect("frame")
    };
    assert_eq!(top(&tree, BlockKind::Paragraph), 0.0);
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert { text: "## ".into() },
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);
    assert_eq!(top(&tree, BlockKind::Heading(2)), 0.0);
    let cold = compose(&doc, &layout);
    assert_eq!(
        top(&tree, BlockKind::Heading(2)),
        top(&cold, BlockKind::Heading(2))
    );
}

#[test]
fn atx_heading_keeps_mid_doc_paragraph_top() {
    use crate::box_tree::BoxRole;
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, apply};
    let layout = heading_spacing_theme();
    let frame_top = |tree: &crate::box_tree::BoxTree, kind: BlockKind| {
        tree.nodes
            .values()
            .find(|n| n.kind == kind && n.id.role == BoxRole::Frame)
            .map(|n| tree.style_of(n).margin.top)
            .expect("frame")
    };
    for (typed, kind) in [
        ("# ", BlockKind::Heading(1)),
        ("## ", BlockKind::Heading(2)),
        ("### ", BlockKind::Heading(3)),
        ("#### ", BlockKind::Heading(4)),
        ("##### ", BlockKind::Heading(5)),
        ("###### ", BlockKind::Heading(6)),
    ] {
        let mut doc = load_markdown("first\n\nsecond\n", editor_options());
        let mut tree = compose(&doc, &layout);
        let leaf = doc.text_leaves()[1];
        let mut para_tops: Vec<_> = tree
            .nodes
            .values()
            .filter(|n| n.kind == BlockKind::Paragraph && n.id.role == BoxRole::Frame)
            .map(|n| tree.style_of(n).margin.top)
            .collect();
        para_tops.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(para_tops, vec![0.0, 20.0], "{typed}");
        let _ = apply(
            &mut doc,
            Sel::collapsed(Caret {
                block: leaf,
                offset: 0,
            }),
            Command::Insert { text: typed.into() },
        );
        let changes = doc.take_changes();
        let _ = sync_layout(&mut tree, &doc, &changes, &layout);
        assert_eq!(frame_top(&tree, kind), 20.0, "{typed}");
        let cold = compose(&doc, &layout);
        assert_eq!(frame_top(&tree, kind), frame_top(&cold, kind), "{typed}");
    }
}

#[test]
fn insert_row_above_header_retargets_type_slot() {
    use crate::box_tree::{LayoutBoxId, TypeSlot};
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, TableOp, apply};
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let header = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.text_of(id) == Some("a"))
        .expect("header");
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let cell_box = LayoutBoxId::for_kind(BlockKind::TableCell, header);
    assert_eq!(
        tree.nodes.get(&cell_box).map(|n| n.type_slot),
        Some(TypeSlot::TableHeader)
    );
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: header,
            offset: 0,
        }),
        Command::Table(TableOp::InsertRowAbove),
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);
    assert_eq!(
        tree.nodes.get(&cell_box).map(|n| n.type_slot),
        Some(TypeSlot::FromKind)
    );
    let cold = compose(&doc, &layout);
    assert_eq!(
        tree.nodes.get(&cell_box).map(|n| n.type_slot),
        cold.nodes.get(&cell_box).map(|n| n.type_slot)
    );
}

#[test]
fn deleted_row_leaves_no_orphan_boxes_behind() {
    use md_core::document::{Caret, Command, Sel, TableOp, apply};

    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let header = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.text_of(id) == Some("a"))
        .expect("header cell");
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let _ = doc.take_changes();

    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: header,
            offset: 0,
        }),
        Command::Table(TableOp::DeleteRow),
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    let cold = compose(&doc, &layout);
    assert_box_sets_match(&tree, &cold);
}

#[test]
fn undone_lift_keeps_migrated_boxes_under_their_new_host() {
    use md_core::doc::Doc;
    use md_core::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown(
        "- a\n  - b\n    - c\n  - d\n",
        editor_options(),
    ));
    let caret = doc
        .document
        .text_leaves()
        .into_iter()
        .find(|&leaf| doc.text(leaf) == Some("b"))
        .expect("b paragraph");
    let layout = layout();
    let mut tree = compose(&doc.document, &layout);
    let _ = doc.take_changes();

    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: caret,
            offset: 0,
        }),
        Command::Outdent,
    );
    let lifted = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc.document, &lifted, &layout);
    assert_box_sets_match(&tree, &compose(&doc.document, &layout));

    let _ = doc.undo().expect("undo outdent");
    let replayed = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc.document, &replayed, &layout);
    assert_box_sets_match(&tree, &compose(&doc.document, &layout));
}

fn assert_box_sets_match(hot: &crate::box_tree::BoxTree, cold: &crate::box_tree::BoxTree) {
    assert_eq!(
        hot.nodes.len(),
        cold.nodes.len(),
        "hot has {} boxes, cold has {} — residue or over-drop",
        hot.nodes.len(),
        cold.nodes.len()
    );
    for (id, node) in &cold.nodes {
        let Some(hot_node) = hot.nodes.get(id) else {
            panic!("hot tree is missing {id:?} ({:?})", node.kind);
        };
        assert_eq!(hot_node.kind, node.kind, "kind drift at {id:?}");
    }
}

#[test]
fn repeated_list_moves_do_not_retain_unowned_text_snapshots() {
    use md_core::document::{Caret, Command, Sel, apply};

    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let block = doc.text_leaves()[1];
    let _ = doc.take_changes();
    for _ in 0..10 {
        for command in [Command::Indent, Command::Outdent] {
            apply(
                &mut doc,
                Sel::collapsed(Caret { block, offset: 0 }),
                command,
            );
            let changes = doc.take_changes();
            assert!(!changes.is_empty());
            sync_layout(&mut tree, &doc, &changes, &layout);
        }
    }
    let live_texts = tree
        .nodes()
        .values()
        .filter(|node| node.text_id().is_some())
        .count();
    let cold = compose(&doc, &layout);
    assert_eq!(cold.intern().len(), live_texts, "fixture sanity: cold tree");
    assert_eq!(
        tree.intern().len(),
        live_texts,
        "moved boxes must release the snapshots they replace"
    );
}

#[test]
fn quote_list_and_fence_commit_keep_doc_lead_zero() {
    use crate::box_tree::BoxRole;
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Document, Sel, apply};

    let layout = spacing_theme(|kind| match kind {
        BlockKind::Paragraph => 20.0,
        BlockKind::BlockQuote | BlockKind::CodeBlock | BlockKind::List => 30.0,
        _ => 0.0,
    })
    .with_flow_metrics(flow_metrics(24.0));

    fn top(tree: &crate::box_tree::BoxTree, kind: BlockKind) -> md_core::Px {
        tree.nodes
            .values()
            .find(|n| n.kind == kind && n.id.role == BoxRole::Frame)
            .map(|n| tree.style_of(n).margin.top)
            .unwrap_or_else(|| panic!("{kind:?} frame"))
    }
    fn commit_keep_zero(
        doc: &mut Document,
        tree: &mut crate::box_tree::BoxTree,
        layout: &crate::compose::LayoutTheme,
        kind: BlockKind,
    ) {
        let changes = doc.take_changes();
        let _ = sync_layout(tree, doc, &changes, layout);
        let cold = compose(doc, layout);
        assert_eq!(top(tree, kind), 0.0, "{kind:?}: hot doc-lead margin.top");
        assert_eq!(
            top(tree, kind),
            top(&cold, kind),
            "{kind:?}: hot/cold doc-lead margin.top diverged"
        );
    }

    let mut doc = load_markdown("", editor_options());
    let mut tree = compose(&doc, &layout);
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert { text: "> ".into() },
    );
    commit_keep_zero(&mut doc, &mut tree, &layout, BlockKind::BlockQuote);

    let mut doc = load_markdown("", editor_options());
    let mut tree = compose(&doc, &layout);
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert { text: "- ".into() },
    );
    commit_keep_zero(&mut doc, &mut tree, &layout, BlockKind::List);

    let mut doc = load_markdown("", editor_options());
    let mut tree = compose(&doc, &layout);
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert {
            text: "```rust".into(),
        },
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 6,
        }),
        Command::Break,
    );
    commit_keep_zero(&mut doc, &mut tree, &layout, BlockKind::CodeBlock);
}

#[test]
fn wrapper_lead_stays_flushed_through_a_sync() {
    use crate::box_tree::LayoutBoxId;
    use md_core::block::BlockKind;

    let layout = spacing_theme(|kind| match kind {
        BlockKind::Paragraph => 20.0,
        BlockKind::BlockQuote => 30.0,
        BlockKind::ThematicBreak => 24.0,
        _ => 0.0,
    })
    .with_flow_metrics(flow_metrics(24.0));

    let mut doc = load_markdown("> ***\n> p\n", editor_options());
    let mut tree = compose(&doc, &layout);
    let rule = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::ThematicBreak))
        .expect("rule");
    let bid = LayoutBoxId::for_kind(BlockKind::ThematicBreak, rule.index);
    assert_eq!(tree.style(bid).margin.top, 0.0, "quote-lead rule");

    let leaf = doc.text_leaves()[0];
    let changes = doc.split_leaf(leaf, 0).0;
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    let cold = compose(&doc, &layout);
    assert_eq!(
        tree.style(bid).margin.top,
        0.0,
        "the rule is still the quote's lead after the split"
    );
    assert_eq!(
        tree.style(bid).margin.top,
        cold.style(bid).margin.top,
        "hot and cold must agree on the quote lead"
    );
}

#[test]
fn absorbing_a_soft_break_gives_the_new_paragraph_a_box() {
    use crate::box_tree::LayoutBoxId;
    use md_core::block::BlockKind;
    use md_core::doc::Doc;
    use md_core::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown("", editor_options()));
    doc.enable_trailing_blank();
    let layout = layout();
    let mut tree = compose(&doc.document, &layout);
    let _ = doc.take_changes();

    let leaf = doc.text_leaves()[0];
    let mut at = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert { text: "123".into() },
    );
    let _ = doc.take_changes();
    at = doc.apply(Sel::collapsed(at), Command::SoftBreak);
    let _ = doc.take_changes();
    at = doc.apply(Sel::collapsed(at), Command::SoftBreak);
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc.document, &changes, &layout);

    let landed = LayoutBoxId::for_kind(BlockKind::Paragraph, at.block);
    assert!(
        tree.nodes.contains_key(&landed),
        "the caret moved into {landed:?} but the tree holds no such box"
    );
    let cold = compose(&doc.document, &layout);
    assert_eq!(tree.nodes.len(), cold.nodes.len());
    for (id, node) in &cold.nodes {
        let hot = tree.nodes.get(id).expect("every cold box has a hot twin");
        assert_eq!(hot.kind, node.kind);
        assert_eq!(
            tree.intern.text(hot.text_id),
            cold.intern.text(node.text_id)
        );
    }
}

#[test]
fn binding_an_alert_restyles_the_quote_lead() {
    use crate::box_tree::LayoutBoxId;
    use md_core::block::BlockKind;
    use md_core::doc::Doc;
    use md_core::document::{Caret, Command, Sel};

    let mut doc = Doc::new(load_markdown("> [!NOTE\n", editor_options()));
    doc.enable_trailing_blank();
    let layout = layout();
    let mut tree = compose(&doc.document, &layout);
    let _ = doc.take_changes();

    let leaf = doc.text_leaves()[0];
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 6,
        }),
        Command::Insert { text: "]".into() },
    );
    let changes = doc.take_changes();
    let _ = sync_layout(&mut tree, &doc.document, &changes, &layout);

    let quote = doc
        .document
        .preorder()
        .into_iter()
        .find(|&id| {
            doc.document
                .arena
                .get(id)
                .is_some_and(|n| n.kind == BlockKind::BlockQuote)
        })
        .expect("the quote survived the bind");
    let box_id = LayoutBoxId::for_kind(BlockKind::BlockQuote, quote.index);
    let cold = compose(&doc.document, &layout);
    assert_eq!(
        tree.style_of(tree.get(box_id)).padding.top,
        cold.style_of(cold.get(box_id)).padding.top,
        "the hot quote kept a stale lead padding"
    );
    assert_eq!(
        cold.style_of(cold.get(box_id)).padding.top,
        layout.quote_alert_lead
    );
}

fn child_list(
    tree: &crate::box_tree::BoxTree,
    id: crate::box_tree::LayoutBoxId,
) -> (u8, Vec<crate::box_tree::LayoutBoxId>) {
    use crate::box_tree::BoxChildren;
    match tree.nodes.get(&id).map(|n| &n.children) {
        Some(BoxChildren::Vertical(c)) => (0, c.clone()),
        Some(BoxChildren::Island(c)) => (1, c.clone()),
        Some(BoxChildren::None) | None => (2, Vec::new()),
    }
}

fn assert_sync_matches_cold(
    hot: &crate::box_tree::BoxTree,
    cold: &crate::box_tree::BoxTree,
    what: &str,
) {
    assert_eq!(
        hot.nodes.len(),
        cold.nodes.len(),
        "{what}: hot has {} boxes, cold has {}",
        hot.nodes.len(),
        cold.nodes.len()
    );
    for (id, node) in &cold.nodes {
        let Some(hot_node) = hot.nodes.get(id) else {
            panic!("{what}: the hot tree is missing {id:?} ({:?})", node.kind);
        };
        assert_eq!(hot_node.kind, node.kind, "{what}: kind drift at {id:?}");
        assert_eq!(
            hot_node.parent, node.parent,
            "{what}: parent drift at {id:?}"
        );
        assert_eq!(
            child_list(hot, *id),
            child_list(cold, *id),
            "{what}: child list drift at {id:?}"
        );
        assert_eq!(
            hot_node.type_slot, node.type_slot,
            "{what}: type slot drift at {id:?}"
        );
        assert_eq!(
            hot.style_of(hot_node),
            cold.style_of(node),
            "{what}: style drift at {id:?}"
        );
        assert_eq!(
            hot.intern.text(hot_node.text_id),
            cold.intern.text(node.text_id),
            "{what}: text drift at {id:?}"
        );
    }
}

#[test]
fn splitting_a_root_paragraph_leaves_the_other_root_children_in_place() {
    let mut doc = load_markdown("alpha\n\nbeta\n\ngamma\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let root = tree.root();
    let before = child_list(&tree, root).1;
    assert_eq!(before.len(), 3, "the fixture needs three root blocks");

    let leaf = doc.text_leaves()[1];
    let changes = doc.split_leaf(leaf, 2).0;
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    let after = child_list(&tree, root).1;
    assert_eq!(after.len(), 4, "a split adds exactly one root child");
    assert_eq!(
        after[0], before[0],
        "the block before the split must not move"
    );
    assert_eq!(
        after[1], before[1],
        "the split block keeps its own slot in the parent"
    );
    assert_eq!(
        after[3], before[2],
        "the block after the split must not move"
    );
    assert_ne!(
        after[2], before[1],
        "the new block takes the slot after the split"
    );
    assert_sync_matches_cold(&tree, &compose(&doc, &layout), "root split");
}

#[test]
fn deleting_a_root_paragraph_keeps_the_survivors_in_their_slots() {
    use md_core::document::{Caret, Command, Sel, apply};

    let mut doc = load_markdown("alpha\n\nbeta\n\ngamma\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let root = tree.root();
    let before = child_list(&tree, root).1;
    let _ = doc.take_changes();

    let leaf = doc.text_leaves()[1];
    let next = doc.text_leaves()[2];
    let _ = apply(
        &mut doc,
        Sel {
            anchor: Caret {
                block: leaf,
                offset: 5,
            },
            head: Caret {
                block: next,
                offset: 2,
            },
        },
        Command::DeleteBackward,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural(), "a merge must restructure the root");
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    assert_eq!(
        child_list(&tree, root).1,
        vec![before[0], before[1]],
        "only the merged-away block's slots may leave the parent"
    );
    assert_sync_matches_cold(&tree, &compose(&doc, &layout), "root merge");
}

#[test]
fn wrapping_a_root_paragraph_replaces_its_slot_without_duplicating_it() {
    use md_core::document::{Caret, Command, Sel, apply};

    let mut doc = load_markdown("alpha\n\nbeta\n\ngamma\n", editor_options());
    let layout = layout();
    let mut tree = compose(&doc, &layout);
    let root = tree.root();
    let before = child_list(&tree, root).1;
    let _ = doc.take_changes();

    let leaf = doc.text_leaves()[1];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::WrapList {
            ordered: false,
            task: None,
        },
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural(), "wrapping a paragraph restructures");
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    let after = child_list(&tree, root).1;
    assert_eq!(
        after.len(),
        before.len(),
        "the wrapper replaces the paragraph instead of adding a sibling"
    );
    assert_eq!(
        after[0], before[0],
        "the block before the wrap must not move"
    );
    assert_eq!(
        after[2], before[2],
        "the block after the wrap must not move"
    );
    assert_sync_matches_cold(&tree, &compose(&doc, &layout), "wrap list");
}

#[test]
fn splitting_a_heading_moves_the_following_paragraph_off_the_lead_spacing() {
    use crate::box_tree::LayoutBoxId;
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, apply};

    let layout = heading_spacing_theme();
    let mut doc = load_markdown("# h\n\npara\n", editor_options());
    let mut tree = compose(&doc, &layout);
    let _ = doc.take_changes();

    let head = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::Heading(1)))
        .expect("a heading");
    let head_box = LayoutBoxId::for_kind(BlockKind::Heading(1), head.index);
    let para = doc.text_leaves()[1];
    let para_box = LayoutBoxId::frame(para);
    assert_eq!(
        tree.style_of(tree.get(head_box)).margin.top,
        0.0,
        "the doc lead is flushed to the top"
    );
    assert_eq!(
        tree.style_of(tree.get(para_box)).margin.top,
        24.0,
        "a paragraph behind an H1 takes the lead spacing"
    );

    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: head.index,
            offset: 1,
        }),
        Command::Break,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural(), "splitting restructures the tree");
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    let cold = compose(&doc, &layout);
    assert_eq!(
        tree.style_of(tree.get(head_box)).margin.top,
        cold.style_of(cold.get(head_box)).margin.top,
        "the heading in front of the splice must not move"
    );
    assert_eq!(
        tree.style_of(tree.get(para_box)).margin.top,
        20.0,
        "a paragraph behind the fresh split must take its own top margin"
    );
    assert_eq!(
        tree.style_of(tree.get(para_box)).margin.top,
        cold.style_of(cold.get(para_box)).margin.top,
        "hot and cold must agree on the paragraph behind the split"
    );
    assert_sync_matches_cold(&tree, &cold, "heading split");
}

#[test]
fn deleting_the_doc_lead_flushes_the_new_lead_to_the_top() {
    use crate::box_tree::LayoutBoxId;
    use md_core::block::BlockKind;
    use md_core::document::{Caret, Command, Sel, apply};

    let layout = heading_spacing_theme();
    let mut doc = load_markdown("***\n\npara\n", editor_options());
    let mut tree = compose(&doc, &layout);
    let _ = doc.take_changes();

    let rule = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::ThematicBreak))
        .expect("a leading rule");
    let rule_box = LayoutBoxId::for_kind(BlockKind::ThematicBreak, rule.index);
    assert_eq!(
        tree.style_of(tree.get(rule_box)).margin.top,
        0.0,
        "the doc lead is flushed to the top"
    );

    let para = doc.text_leaves()[0];
    let para_box = LayoutBoxId::frame(para);
    assert_eq!(
        tree.style_of(tree.get(para_box)).margin.top,
        20.0,
        "a paragraph behind the doc lead keeps its own top margin"
    );

    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: para,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    let changes = doc.take_changes();
    assert!(
        changes.is_structural(),
        "removing the leading rule restructures the tree"
    );
    let _ = sync_layout(&mut tree, &doc, &changes, &layout);

    let cold = compose(&doc, &layout);
    assert!(
        !tree.nodes.contains_key(&rule_box),
        "the removed rule must leave the tree"
    );
    assert_eq!(
        tree.style_of(tree.get(para_box)).margin.top,
        0.0,
        "the paragraph that becomes the doc lead must be flushed to the top"
    );
    assert_eq!(
        tree.style_of(tree.get(para_box)).margin.top,
        cold.style_of(cold.get(para_box)).margin.top,
        "hot and cold must agree on the new lead"
    );
    assert_sync_matches_cold(&tree, &cold, "doc lead delete");
}

#[test]
fn a_scripted_edit_sequence_keeps_every_child_list_in_document_order() {
    use md_core::doc::Doc;
    use md_core::document::{Caret, Command, PasteIntent, Sel, TableOp};

    let source = "# head\n\nintro\n\n- a\n- b\n  - c\n\n> quoted\n>\n> more\n\n```rust\nlet a = 1;\n```\n\n| h | i |\n| --- | --- |\n| x | y |\n\ntail\n";
    let layout = layout();
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let mut tree = compose(&doc.document, &layout);
    let _ = doc.take_changes();

    let leaf = |doc: &Doc, text: &str| {
        doc.document
            .text_leaves()
            .into_iter()
            .find(|&id| doc.document.text_of(id) == Some(text))
            .unwrap_or_else(|| panic!("no text leaf reading {text:?}"))
    };

    let steps: Vec<(&str, &str, (usize, usize), Command)> = vec![
        ("split the intro paragraph", "intro", (5, 5), Command::Break),
        ("indent a list item", "b", (0, 0), Command::Indent),
        ("outdent it again", "b", (0, 0), Command::Outdent),
        ("split inside a quote", "quoted", (3, 3), Command::Break),
        ("check a task box", "a", (0, 0), Command::ToggleTask),
        ("delete a cell's text", "y", (0, 1), Command::DeleteBackward),
        (
            "delete a table row",
            "x",
            (0, 0),
            Command::Table(TableOp::DeleteRow),
        ),
        (
            "insert a table row above",
            "h",
            (0, 0),
            Command::Table(TableOp::InsertRowAbove),
        ),
        (
            "wrap the tail in a list",
            "tail",
            (0, 0),
            Command::WrapList {
                ordered: true,
                task: None,
            },
        ),
        ("split the tail paragraph", "tail", (2, 2), Command::Break),
        ("outdent a nested item", "c", (0, 0), Command::Outdent),
        (
            "quote the heading",
            "head",
            (0, 0),
            Command::Insert { text: "> ".into() },
        ),
        (
            "paste an independent fragment",
            "more",
            (0, 0),
            Command::Paste {
                text: "p1\n\np2\n".into(),
                intent: PasteIntent::IndependentFragment,
            },
        ),
    ];

    for (what, needle, (from, to), command) in steps {
        let block = leaf(&doc, needle);
        let sel = Sel {
            anchor: Caret {
                block,
                offset: from,
            },
            head: Caret { block, offset: to },
        };
        let _ = doc.apply(sel, command);
        let changes = doc.take_changes();
        let _ = sync_layout(&mut tree, &doc.document, &changes, &layout);
        assert_sync_matches_cold(&tree, &compose(&doc.document, &layout), what);
    }

    for _ in 0..6 {
        let _ = doc.undo().expect("the sequence is undoable");
        let changes = doc.take_changes();
        let _ = sync_layout(&mut tree, &doc.document, &changes, &layout);
        assert_sync_matches_cold(&tree, &compose(&doc.document, &layout), "undo");
    }
    for _ in 0..6 {
        let _ = doc.redo().expect("the sequence is redoable");
        let changes = doc.take_changes();
        let _ = sync_layout(&mut tree, &doc.document, &changes, &layout);
        assert_sync_matches_cold(&tree, &compose(&doc.document, &layout), "redo");
    }
}
