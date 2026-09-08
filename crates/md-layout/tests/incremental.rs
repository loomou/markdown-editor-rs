use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{ChangeSet, DocChange, editor_options, load_markdown};
use md_layout::box_tree::{BoxChildren, BoxRole, LayoutBoxId};
use md_layout::compose::{
    ComposeWindow, LayoutTheme, LeafMetrics, compose, compose_window, defer_composed, sync_layout,
};
use md_layout::flow::HeightState;
use md_layout::spine::FlowSpine;
use md_layout::style::{BoxDisplay, BoxLayoutStyle, Edges};

fn theme() -> LayoutTheme {
    LayoutTheme::from_resolver(|kind| BoxLayoutStyle {
        display: if kind.is_vertical_container() {
            BoxDisplay::FlowStack
        } else if kind == BlockKind::TableRow {
            BoxDisplay::IslandRow
        } else {
            BoxDisplay::MeasuredLeaf
        },
        margin: if matches!(kind, BlockKind::DocRoot | BlockKind::DocStart) {
            Edges::ZERO
        } else {
            Edges {
                top: 10.0,
                bottom: 3.0,
                ..Edges::ZERO
            }
        },
        padding: Edges::ZERO,
        border: Edges::ZERO,
        gap: 0.0,
    })
}

fn metrics() -> LeafMetrics {
    LeafMetrics {
        line_height: 20.0,
        em_width: 10.0,
        heading1_mult: 2.0,
        heading_mult: 1.5,
        table_row_mult: 1.2,
        mermaid_max_height: 400.0,
        image_placeholder_height: 100.0,
        code_max_height: 200.0,
        math_max_height: 120.0,
        image_max_height: 400.0,
    }
}

#[test]
fn release_frame_respects_visible_and_pinned_preview() {
    let mut doc = Doc::new(load_markdown(
        "```mermaid\ngraph TD; A-->B\n```\n\ntail\n",
        editor_options(),
    ));
    let id = doc
        .document
        .arena
        .children(doc.document.root)
        .next()
        .unwrap();
    doc.retarget_focus(Cursor {
        block: id.index,
        offset: 0,
    });
    assert_eq!(doc.block_edit(), Some(id.index));
    let frame = LayoutBoxId::frame(id.index);

    let tree = compose(&doc.document, &theme());
    let preview = LayoutBoxId::preview(frame.block().unwrap());
    let spine = FlowSpine::flatten(&tree, 400.0, &|id, _| {
        HeightState::Exact(if id.role == BoxRole::Preview {
            1000.0
        } else {
            20.0
        })
    });
    let preview_item = spine.content_id(preview).unwrap();
    let ptop = spine.item_top(preview_item).unwrap();

    let visible = spine.next_release_targets(&tree, ptop + 100.0, ptop + 200.0, None, &[], 8);
    let pinned = spine.next_release_targets(&tree, 2000.0, 2100.0, None, &[preview], 8);
    let anchored = spine.next_release_targets(&tree, 2000.0, 2100.0, Some(preview_item), &[], 8);
    println!(
        "preview top={ptop}; frame selected: visible={} pinned={} anchored={}",
        visible.contains(&frame),
        pinned.contains(&frame),
        anchored.contains(&frame)
    );
    assert!(
        !visible.contains(&frame) && !pinned.contains(&frame) && !anchored.contains(&frame),
        "deleting a Frame takes its Preview with it; whenever the preview is protected the host must not be chosen as a release target"
    );
}

#[test]
fn column_count_survives_released_header_row() {
    let doc = load_markdown("| a | b |\n| --- | --- |\n| x | y |\n", editor_options());
    let table = doc.arena.children(doc.root).next().unwrap();
    let tid = LayoutBoxId::frame(table.index);
    let mut tree = compose(&doc, &theme());
    let header = match tree.get(tid).children() {
        BoxChildren::Vertical(rows) => rows[0],
        _ => panic!("table fixture"),
    };
    assert_eq!(md_layout::assembly::first_row_col_count(&tree, tid), 2);
    defer_composed(&mut tree, header, 20.0);
    println!(
        "released header={header:?}; live table={}",
        tree.nodes().contains_key(&tid)
    );
    assert_eq!(
        md_layout::assembly::first_row_col_count(&tree, tid),
        2,
        "after the first row is released, a column count query must not panic and must not degrade to 1"
    );
}

#[test]
fn column_count_survives_fully_released_table() {
    let doc = load_markdown("| a | b |\n| --- | --- |\n| x | y |\n", editor_options());
    let table = doc.arena.children(doc.root).next().unwrap();
    let tid = LayoutBoxId::frame(table.index);
    let mut tree = compose(&doc, &theme());
    let rows = match tree.get(tid).children() {
        BoxChildren::Vertical(rows) => rows.clone(),
        _ => panic!("table fixture"),
    };
    for r in rows {
        defer_composed(&mut tree, r, 20.0);
    }
    let count = md_layout::assembly::first_row_col_count(&tree, tid);
    println!("fully cold table column count={count}");
    assert!(
        count >= 1,
        "a column count query over a fully cold table must not panic"
    );
}

#[test]
fn cold_attrs_recompute_height() {
    let window = ComposeWindow {
        top: -100.0,
        bottom: -50.0,
        avail_width: 400.0,
    };
    let mut doc = load_markdown("prefix\n\n## heading\n", editor_options());
    let id = doc.arena.children(doc.root).last().unwrap();
    let bid = LayoutBoxId::frame(id.index);
    let mut tree = compose_window(&doc, &theme(), window, &metrics());
    let old = tree.deferred_height(bid).unwrap();
    let old_extra = doc.extra(id);

    let node = doc.arena.get_mut(id).unwrap();
    node.kind = BlockKind::Heading(1);
    node.content_revision += 1;
    let changes = ChangeSet {
        before_revision: 1,
        after_revision: 2,
        changes: vec![DocChange::AttrsChanged {
            node: id,
            old_kind: BlockKind::Heading(2),
            new_kind: BlockKind::Heading(1),
            old_extra,
            new_extra: old_extra,
        }],
    };
    sync_layout(&mut tree, &doc, &changes, &theme());
    let fresh = compose_window(&doc, &theme(), window, &metrics());
    println!(
        "cold attrs old={old} updated={:?} fresh={:?}",
        tree.deferred_height(bid),
        fresh.deferred_height(bid)
    );
    assert_eq!(
        tree.deferred_height(bid),
        fresh.deferred_height(bid),
        "an attribute change on a cold block must re-estimate its deferred height: the incremental estimate must match a fresh window bit for bit"
    );
}

#[test]
fn cold_new_first_child_resets_top_margin() {
    let window = ComposeWindow {
        top: -100.0,
        bottom: -50.0,
        avail_width: 400.0,
    };
    let mut doc = load_markdown("first\n\nsecond\n\nthird\n", editor_options());
    let first = doc.arena.children(doc.root).next().unwrap();
    let theme = theme();
    let mut tree = compose_window(&doc, &theme, window, &metrics());
    doc.arena.detach(first);
    let changes = ChangeSet {
        before_revision: 1,
        after_revision: 2,
        changes: vec![DocChange::TreeSpliced {
            parent: doc.root,
            before: None,
            removed: vec![first],
            inserted: vec![],
        }],
    };
    sync_layout(&mut tree, &doc, &changes, &theme);
    let fresh = compose_window(&doc, &theme, window, &metrics());
    let height = |tree: &md_layout::box_tree::BoxTree| {
        FlowSpine::flatten(tree, 400.0, &|_, _| HeightState::Estimated(20.0)).total_height()
    };
    println!(
        "cold first updated={} fresh={}",
        height(&tree),
        height(&fresh)
    );
    assert_eq!(
        height(&tree),
        height(&fresh),
        "a newly promoted first child in the cold zone must zero its top margin per doc_lead_zero; the incremental tree must match a fresh window bit for bit"
    );
}
