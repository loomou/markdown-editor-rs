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
