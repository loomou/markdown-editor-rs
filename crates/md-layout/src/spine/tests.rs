use super::FlowSpine;
use super::fenwick::Fenwick;
use crate::box_tree::LayoutBoxId;
use crate::box_tree::{BoxOwner, BoxRole};
use crate::flow::HeightState;
use crate::spine::item::{FlowItem, FlowItemId, FlowItemKind};
use md_core::Px;
use std::collections::BTreeMap;

fn box_id(n: u32) -> LayoutBoxId {
    LayoutBoxId {
        owner: BoxOwner::Block(n),
        role: BoxRole::Frame,
        local_key: 0,
    }
}

fn content(index: u32, n: u32, h: Px) -> FlowItem {
    FlowItem::with_epoch(
        FlowItemId {
            index,
            generation: 1,
        },
        FlowItemKind::Content { box_id: box_id(n) },
        HeightState::Exact(h),
        0,
    )
}

#[test]
fn prefix_and_set_height() {
    let mut spine = FlowSpine::from_items(vec![
        content(1, 1, 10.0),
        content(2, 2, 20.0),
        content(3, 3, 30.0),
    ]);
    assert_eq!(spine.total_height(), 60.0);
    assert_eq!(
        spine
            .item_top(FlowItemId {
                index: 2,
                generation: 1
            })
            .unwrap(),
        10.0
    );
    let id = FlowItemId {
        index: 2,
        generation: 1,
    };
    spine.set_height(id, HeightState::Exact(25.0));
    assert_eq!(spine.total_height(), 65.0);
    assert_eq!(
        spine
            .item_top(FlowItemId {
                index: 3,
                generation: 1
            })
            .unwrap(),
        35.0
    );
    let found = spine.y_to_item(12.0).unwrap();
    assert_eq!(found.index, 2);
    assert_eq!(spine.location(id), Some(1));
}

#[test]
fn splice_replaces_middle() {
    let mut spine = FlowSpine::from_items(vec![
        content(1, 1, 10.0),
        content(2, 2, 20.0),
        content(3, 3, 30.0),
    ]);
    spine.splice(1, 1, vec![content(4, 9, 5.0)]);
    assert_eq!(spine.len(), 3);
    assert_eq!(spine.total_height(), 45.0);
    assert_eq!(
        spine.content_id(box_id(9)).unwrap(),
        FlowItemId {
            index: 4,
            generation: 1
        }
    );
    assert!(spine.content_id(box_id(2)).is_none());
    assert_eq!(
        spine.location(FlowItemId {
            index: 3,
            generation: 1
        }),
        Some(2)
    );
}

#[test]
fn visible_range_is_local() {
    let items: Vec<FlowItem> = (0..100).map(|i| content(i + 1, i + 1, 10.0)).collect();
    let spine = FlowSpine::from_items(items);
    let r = spine.visible(250.0, 280.0);
    assert_eq!(r.start, 25);
    assert_eq!(r.end, 28);
}

#[test]
fn gap_item_roundtrip() {
    let spine = FlowSpine::from_items(vec![FlowItem::with_epoch(
        FlowItemId {
            index: 1,
            generation: 1,
        },
        FlowItemKind::Gap,
        HeightState::Exact(4.0),
        0,
    )]);
    assert_eq!(spine.total_height(), 4.0);
}

fn expected(spine: &FlowSpine) -> (Vec<Px>, Px, u32, u32, u32, u32) {
    let mut tops = Vec::with_capacity(spine.items.len() + 1);
    let mut y = 0.0;
    for item in &spine.items {
        tops.push(y);
        y += item.height.px();
    }
    tops.push(y);
    let mut content = 0u32;
    let mut exact = 0u32;
    let mut noncontent = 0u32;
    for item in &spine.items {
        if item.is_content() {
            content += 1;
            if item.height.is_exact() && item.height_epoch == spine.layout_epoch {
                exact += 1;
            }
        } else if !item.height.is_exact() {
            noncontent += 1;
        }
    }
    let estimated = noncontent + content.saturating_sub(exact);
    (tops, y, content, exact, noncontent, estimated)
}

fn assert_index_matches_scratch(spine: &FlowSpine) {
    let (tops, total, content, exact, noncontent, estimated) = expected(spine);
    assert_eq!(spine.total_height(), total, "total height");
    for (pos, top) in tops.iter().enumerate() {
        assert_eq!(spine.fenwick.prefix(pos), *top, "prefix at {pos}");
    }
    assert_eq!(spine.content_count, content, "content_count");
    assert_eq!(
        spine.exact_content_in_epoch, exact,
        "exact_content_in_epoch"
    );
    assert_eq!(
        spine.noncontent_estimated, noncontent,
        "noncontent_estimated"
    );
    assert_eq!(spine.estimated_count(), estimated, "estimated_count");
    for (pos, item) in spine.items.iter().enumerate() {
        assert_eq!(spine.location(item.id), Some(pos), "location of item {pos}");
    }
    assert_box_maps_match_items(spine);
}

fn assert_box_maps_match_items(spine: &FlowSpine) {
    let mut content_of = BTreeMap::new();
    let mut open_of = BTreeMap::new();
    let mut close_of = BTreeMap::new();
    let mut collapsed_of = BTreeMap::new();
    for item in &spine.items {
        match item.kind {
            FlowItemKind::Content { box_id } => {
                content_of.insert(box_id, item.id);
            }
            FlowItemKind::ContainerOpen { box_id } => {
                open_of.insert(box_id, item.id);
            }
            FlowItemKind::ContainerClose { box_id } => {
                close_of.insert(box_id, item.id);
            }
            FlowItemKind::Collapsed { box_id } => {
                collapsed_of.insert(box_id, item.id);
            }
            FlowItemKind::Gap => {}
        }
    }
    assert_eq!(spine.content_of, content_of, "content_of");
    assert_eq!(spine.open_of, open_of, "open_of");
    assert_eq!(spine.close_of, close_of, "close_of");
    assert_eq!(spine.collapsed_of, collapsed_of, "collapsed_of");
}

fn assert_records_on_tree(spine: &FlowSpine, tree: &crate::box_tree::BoxTree) {
    for item in &spine.items {
        let (box_id, allow_deferred) = match item.kind {
            FlowItemKind::Content { box_id }
            | FlowItemKind::ContainerOpen { box_id }
            | FlowItemKind::ContainerClose { box_id } => (box_id, false),
            FlowItemKind::Collapsed { box_id } => (box_id, true),
            FlowItemKind::Gap => continue,
        };
        if tree.nodes.contains_key(&box_id) {
            continue;
        }
        assert!(
            allow_deferred && tree.deferred(box_id).is_some(),
            "spine holds a record for {box_id:?} that is not on the tree"
        );
    }
}

fn box_on_tree(
    tree: &crate::box_tree::BoxTree,
    block: md_core::block::BlockId,
) -> Option<LayoutBoxId> {
    tree.nodes()
        .into_iter()
        .find(|(id, _)| id.owner == BoxOwner::Block(block))
        .map(|(id, _)| *id)
}

fn long_spine(len: u32) -> FlowSpine {
    let items = (0..len)
        .map(|i| {
            let h = Px::from(i % 7) + 1.0;
            let mut item = content(i + 1, i + 1, h);
            if i % 3 == 0 {
                item.height = HeightState::Estimated(h);
            }
            item
        })
        .collect();
    FlowSpine::from_items(items)
}

#[test]
fn splice_patches_index_without_full_rebuild() {
    let mut spine = long_spine(900);
    assert_index_matches_scratch(&spine);

    let mut next = 5000u32;
    let mut fresh = |h: Px| {
        next += 1;
        content(next, next, h)
    };

    spine.splice(300, 0, vec![fresh(11.0), fresh(12.0)]);
    assert_index_matches_scratch(&spine);

    spine.splice(250, 20, Vec::new());
    assert_index_matches_scratch(&spine);

    let replacement: Vec<FlowItem> = (0..3).map(|_| fresh(3.0)).collect();
    spine.splice(100, 400, replacement);
    assert_index_matches_scratch(&spine);

    spine.splice(0, 1, vec![fresh(9.0)]);
    assert_index_matches_scratch(&spine);
    let end = spine.len();
    spine.splice(end, 0, vec![fresh(6.0)]);
    assert_index_matches_scratch(&spine);
    spine.splice(spine.len() - 1, 1, Vec::new());
    assert_index_matches_scratch(&spine);

    let id = spine.item_at(spine.len() / 2).id;
    spine.set_height(id, HeightState::Exact(41.0));
    assert_index_matches_scratch(&spine);

    spine.splice(0, spine.len(), Vec::new());
    assert_eq!(spine.total_height(), 0.0);
    assert_index_matches_scratch(&spine);
    spine.splice(0, 0, vec![fresh(4.0), fresh(5.0)]);
    assert_index_matches_scratch(&spine);
}

fn assert_chunks_are_occupied(spine: &FlowSpine) {
    let sizes = spine.fenwick.chunk_sizes();
    let total: usize = sizes.iter().sum();
    assert_eq!(total, spine.len(), "chunk sizes must cover every item");
    if sizes.len() < 2 {
        return;
    }
    for (i, size) in sizes[..sizes.len() - 1].iter().enumerate() {
        assert!(
            *size >= Fenwick::MIN_CHUNK,
            "chunk {i} of {} holds only {size} items: {sizes:?}",
            sizes.len()
        );
    }
}

#[test]
fn repeated_splices_at_one_spot_keep_the_index_compact() {
    let mut spine = long_spine(600);
    assert_chunks_are_occupied(&spine);
    let mut next = 9000u32;

    for _ in 0..300 {
        next += 1;
        spine.splice(300, 1, vec![content(next, next, 2.0)]);
        assert_chunks_are_occupied(&spine);
    }
    assert_index_matches_scratch(&spine);

    while spine.len() > 4 {
        spine.splice(spine.len() / 2, 3, Vec::new());
        assert_chunks_are_occupied(&spine);
    }
    assert_index_matches_scratch(&spine);
}

#[test]
fn splice_does_not_rebuild_the_whole_index() {
    let mut spine = long_spine(600);
    let before = crate::hot_path::height_index_builds();
    spine.splice(300, 2, vec![content(7001, 7001, 3.0)]);
    assert_eq!(
        crate::hot_path::height_index_builds(),
        before,
        "structural splice must patch the index, not rebuild it"
    );
    assert_index_matches_scratch(&spine);
}

#[test]
fn patched_index_equals_a_full_rebuild() {
    let mut patched = long_spine(700);
    patched.splice(
        120,
        30,
        (0..4).map(|i| content(8000 + i, 8000 + i, 5.0)).collect(),
    );
    patched.splice(
        600,
        0,
        (0..3).map(|i| content(8100 + i, 8100 + i, 2.0)).collect(),
    );
    let tops: Vec<Px> = (0..=patched.len())
        .map(|pos| patched.fenwick.prefix(pos))
        .collect();
    let total = patched.total_height();
    let estimated = patched.estimated_count();
    patched.rebuild_height_index();
    for (pos, top) in tops.iter().enumerate() {
        assert_eq!(patched.fenwick.prefix(pos), *top, "prefix at {pos}");
    }
    assert_eq!(patched.total_height(), total);
    assert_eq!(patched.estimated_count(), estimated);
}

fn nested_doc(blocks: usize) -> md_core::document::Document {
    use std::fmt::Write;
    let mut md = String::new();
    for i in 0..blocks {
        let _ = writeln!(md, "> > quote {i} with some words to measure\n");
    }
    md_core::document::load_markdown(&md, md_core::document::editor_options())
}

fn padded_theme() -> crate::compose::LayoutTheme {
    use crate::style::{BoxDisplay, BoxLayoutStyle, Edges};
    crate::compose::LayoutTheme::from_resolver(|_| BoxLayoutStyle {
        display: BoxDisplay::FlowStack,
        margin: Edges::vh(3.0, 0.0),
        padding: Edges::vh(2.0, 6.0),
        border: Edges::ZERO,
        gap: 4.0,
    })
}

fn estimate(_id: LayoutBoxId, _avail: Px) -> HeightState {
    HeightState::Estimated(20.0)
}

#[test]
fn expand_visible_patches_index_without_full_rebuild() {
    let doc = nested_doc(400);
    let theme = padded_theme();
    let tree = crate::compose::compose(&doc, &theme);
    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);
    let collapsed_before = spine
        .items
        .iter()
        .filter(|item| matches!(item.kind, FlowItemKind::Collapsed { .. }))
        .count();
    assert!(collapsed_before > 0, "fixture must start collapsed");
    assert_index_matches_scratch(&spine);

    let builds = crate::hot_path::height_index_builds();
    let rounds = spine.expand_visible(&tree, 0.0, 600.0, &estimate);
    assert!(rounds > 0, "visible collapsed items must expand");
    assert_eq!(
        crate::hot_path::height_index_builds(),
        builds,
        "expansion must patch the index, not rebuild it"
    );
    assert_index_matches_scratch(&spine);

    assert_eq!(spine.expand_visible(&tree, 0.0, 600.0, &estimate), 0);
    assert_index_matches_scratch(&spine);

    let rounds = spine.expand_visible(&tree, 4_000.0, 4_600.0, &estimate);
    assert!(rounds > 0);
    assert_index_matches_scratch(&spine);
}

#[test]
fn expand_to_patches_index_without_full_rebuild() {
    let doc = nested_doc(400);
    let theme = padded_theme();
    let tree = crate::compose::compose(&doc, &theme);
    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);

    let target = doc
        .text_leaves()
        .into_iter()
        .rev()
        .filter_map(|index| Some(LayoutBoxId::for_kind(doc.kind(index)?, index)))
        .find(|id| tree.nodes.contains_key(id) && spine.content_id(*id).is_none())
        .expect("a collapsed leaf");
    let builds = crate::hot_path::height_index_builds();
    assert!(spine.expand_to(&tree, target, &estimate));
    assert_eq!(
        crate::hot_path::height_index_builds(),
        builds,
        "expand_to must patch the index, not rebuild it"
    );
    assert!(spine.content_id(target).is_some());
    assert_index_matches_scratch(&spine);

    assert!(spine.expand_to(&tree, target, &estimate));
    assert_index_matches_scratch(&spine);
}

fn drain_collapse(
    spine: &mut FlowSpine,
    tree: &crate::box_tree::BoxTree,
    lo: Px,
    hi: Px,
    keep: Option<FlowItemId>,
    pins: &[LayoutBoxId],
) -> u32 {
    let mut n = 0u32;
    for _ in 0..10_000 {
        let k = spine.collapse_far(tree, lo, hi, keep, pins);
        n += k;
        if k == 0 {
            break;
        }
    }
    n
}

fn visible_has_collapsed(spine: &FlowSpine, top: Px, bottom: Px) -> bool {
    spine
        .visible(top, bottom)
        .any(|pos| matches!(spine.item_at(pos).kind, FlowItemKind::Collapsed { .. }))
}

#[test]
fn collapse_far_recollapses_offscreen_quotes_without_changing_height() {
    let doc = nested_doc(400);
    let theme = padded_theme();
    let tree = crate::compose::compose(&doc, &theme);
    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);
    let flatten_len = spine.len();
    let _ = spine.expand_visible(&tree, 0.0, 600.0, &estimate);
    let _ = spine.expand_visible(&tree, 4_000.0, 4_600.0, &estimate);
    assert_index_matches_scratch(&spine);
    let expanded_len = spine.len();
    assert!(
        expanded_len > flatten_len,
        "fixture must expand far quotes: flatten={flatten_len} expanded={expanded_len}"
    );
    let total = spine.total_height();
    let builds = crate::hot_path::height_index_builds();
    let collapsed = drain_collapse(&mut spine, &tree, 0.0, 1_800.0, None, &[]);
    assert!(collapsed > 0, "offscreen expanded quotes must recollapse");
    assert_eq!(
        crate::hot_path::height_index_builds(),
        builds,
        "collapse must patch the index, not rebuild it"
    );
    assert_eq!(spine.total_height(), total);
    assert!(
        spine.len() < expanded_len,
        "spine.len() did not drop: before={expanded_len} after={}",
        spine.len()
    );
    assert!(
        !visible_has_collapsed(&spine, 0.0, 600.0),
        "visible window must not contain Collapsed after recollapse"
    );
    assert_index_matches_scratch(&spine);
    assert_records_on_tree(&spine, &tree);

    let rounds = spine.expand_visible(&tree, 4_000.0, 4_600.0, &estimate);
    assert!(rounds > 0, "scroll-back must expand the recollapsed quotes");
    assert_eq!(spine.total_height(), total);
    assert_index_matches_scratch(&spine);
    assert_records_on_tree(&spine, &tree);
}

#[test]
fn collapse_far_respects_quota_and_keep_item() {
    let doc = nested_doc(400);
    let theme = padded_theme();
    let tree = crate::compose::compose(&doc, &theme);
    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);
    let _ = spine.expand_visible(&tree, 0.0, 600.0, &estimate);
    let _ = spine.expand_visible(&tree, 4_000.0, 4_600.0, &estimate);
    let keep = spine.visible(4_000.0, 4_600.0).find_map(|pos| {
        spine
            .item_at(pos)
            .is_content()
            .then_some(spine.item_at(pos).id)
    });
    let Some(keep) = keep else {
        panic!("far window must contain a content item");
    };
    let n = spine.collapse_far(&tree, 0.0, 1_800.0, Some(keep), &[]);
    assert!(n <= super::collapse::COLLAPSE_QUOTA);
    assert!(spine.get(keep).is_some(), "keep_item must survive collapse");
    assert_index_matches_scratch(&spine);

    let before = spine.len();
    let n = spine.collapse_far(&tree, 0.0, 1_800.0, None, &[]);
    assert!(n <= super::collapse::COLLAPSE_QUOTA);
    if n > 0 {
        assert!(spine.len() < before);
    }
    assert_index_matches_scratch(&spine);
}

#[test]
fn collapse_far_is_noop_when_nothing_is_expanded() {
    let doc = nested_doc(400);
    let theme = padded_theme();
    let tree = crate::compose::compose(&doc, &theme);
    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);
    let len = spine.len();
    let total = spine.total_height();
    assert_eq!(spine.collapse_far(&tree, 0.0, 1_800.0, None, &[]), 0);
    assert_eq!(spine.len(), len);
    assert_eq!(spine.total_height(), total);
    assert_index_matches_scratch(&spine);
}

#[test]
fn splice_children_patches_index_without_full_rebuild() {
    let mut doc = nested_doc(200);
    let theme = padded_theme();
    let mut tree = crate::compose::compose(&doc, &theme);
    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);
    let _ = spine.expand_visible(&tree, 0.0, 1_200.0, &estimate);
    assert_index_matches_scratch(&spine);

    let leaf = doc.text_leaves()[0];
    let changes = doc.split_leaf(leaf, 3).0;
    let replaced = crate::compose::sync_layout(&mut tree, &doc, &changes, &theme);
    assert!(!replaced, "a split must not force a full recompose");
    let builds = crate::hot_path::height_index_builds();
    let mut spliced = 0;
    for change in &changes.changes {
        let md_core::document::DocChange::TreeSpliced {
            parent,
            before,
            removed,
            inserted,
        } = change
        else {
            continue;
        };
        let box_of = |node: md_core::document::NodeId| {
            let kind = doc.arena.get(node).map(|n| n.kind)?;
            Some(LayoutBoxId::for_kind(kind, node.index))
        };
        let Some(parent_box) = box_of(*parent) else {
            continue;
        };
        let removed_boxes: Vec<LayoutBoxId> = removed.iter().copied().filter_map(box_of).collect();
        let inserted_boxes: Vec<LayoutBoxId> =
            inserted.iter().copied().filter_map(box_of).collect();
        spliced += 1;
        let _ = spine.splice_children(
            &tree,
            parent_box,
            before.and_then(box_of),
            &removed_boxes,
            &inserted_boxes,
            &estimate,
        );
    }
    assert!(spliced > 0, "the split must report a tree splice");
    assert_eq!(
        crate::hot_path::height_index_builds(),
        builds,
        "splice_children must patch the index, not rebuild it"
    );
    assert_index_matches_scratch(&spine);
}

#[test]
fn records_stay_on_the_tree_across_flatten_expand_and_splice() {
    let mut doc = nested_doc(200);
    let theme = padded_theme();
    let mut tree = crate::compose::compose(&doc, &theme);

    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);
    assert_records_on_tree(&spine, &tree);

    let _ = spine.expand_visible(&tree, 0.0, 1_200.0, &estimate);
    assert_records_on_tree(&spine, &tree);

    let leaf = doc.text_leaves()[0];

    let victim = doc.text_leaves()[1];
    let changes = doc.split_leaf(leaf, 3).0;
    let replaced = crate::compose::sync_layout(&mut tree, &doc, &changes, &theme);
    assert!(!replaced, "a split must not force a full recompose");
    let mut spliced = 0;
    for change in &changes.changes {
        let md_core::document::DocChange::TreeSpliced {
            parent,
            before,
            removed,
            inserted,
        } = change
        else {
            continue;
        };
        let box_of = |node: md_core::document::NodeId| {
            let kind = doc.arena.get(node).map(|n| n.kind)?;
            Some(LayoutBoxId::for_kind(kind, node.index))
        };
        let Some(parent_box) = box_of(*parent) else {
            continue;
        };
        let removed_boxes: Vec<LayoutBoxId> = removed.iter().copied().filter_map(box_of).collect();
        let inserted_boxes: Vec<LayoutBoxId> =
            inserted.iter().copied().filter_map(box_of).collect();
        spliced += 1;
        let _ = spine.splice_children(
            &tree,
            parent_box,
            before.and_then(box_of),
            &removed_boxes,
            &inserted_boxes,
            &estimate,
        );
    }
    assert!(spliced > 0, "the split must report a tree splice");
    assert_records_on_tree(&spine, &tree);

    let Some((changes, _, _)) = doc.merge_into_prev(victim) else {
        panic!("the merge must join two adjacent leaves");
    };
    let mut pending = Vec::new();
    for change in &changes.changes {
        let md_core::document::DocChange::TreeSpliced {
            parent,
            before,
            removed,
            inserted,
        } = change
        else {
            continue;
        };
        let box_of = |node: md_core::document::NodeId| {
            let kind = doc.arena.get(node).map(|n| n.kind)?;
            Some(LayoutBoxId::for_kind(kind, node.index))
        };
        let Some(parent_box) = box_of(*parent) else {
            continue;
        };
        let removed_boxes: Vec<LayoutBoxId> = removed
            .iter()
            .filter_map(|n| box_on_tree(&tree, n.index))
            .collect();
        let inserted_boxes: Vec<LayoutBoxId> =
            inserted.iter().copied().filter_map(box_of).collect();
        pending.push((
            parent_box,
            before.and_then(box_of),
            removed_boxes,
            inserted_boxes,
        ));
    }
    assert!(
        pending.iter().any(|(_, _, removed, _)| !removed.is_empty()),
        "the merge must report a removed box"
    );
    let replaced = crate::compose::sync_layout(&mut tree, &doc, &changes, &theme);
    assert!(!replaced, "a merge must not force a full recompose");
    for (parent, before, removed, inserted) in &pending {
        let _ = spine.splice_children(&tree, *parent, *before, removed, inserted, &estimate);
    }
    assert_records_on_tree(&spine, &tree);
}

#[test]
fn splice_inside_collapsed_ancestor_refreshes_its_estimate() {
    let mut doc = nested_doc(2);
    let theme = padded_theme();
    let mut tree = crate::compose::compose(&doc, &theme);
    let mut spine = FlowSpine::flatten(&tree, 800.0, &estimate);
    let leaf = doc.text_leaves()[0];
    let changes = doc.split_leaf(leaf, 3).0;
    let replaced = crate::compose::sync_layout(&mut tree, &doc, &changes, &theme);
    assert!(!replaced);

    for change in &changes.changes {
        let md_core::document::DocChange::TreeSpliced {
            parent,
            before,
            removed,
            inserted,
        } = change
        else {
            continue;
        };
        let box_of = |node: md_core::document::NodeId| {
            let kind = doc.arena.get(node).map(|entry| entry.kind)?;
            Some(LayoutBoxId::for_kind(kind, node.index))
        };
        let parent_box = box_of(*parent).expect("live splice parent");
        assert!(
            spine.collapsed_ancestor(&tree, parent_box).is_some(),
            "fixture must splice below a collapsed container"
        );
        let removed_boxes: Vec<_> = removed.iter().copied().filter_map(box_of).collect();
        let inserted_boxes: Vec<_> = inserted.iter().copied().filter_map(box_of).collect();
        let _ = spine.splice_children(
            &tree,
            parent_box,
            before.and_then(box_of),
            &removed_boxes,
            &inserted_boxes,
            &estimate,
        );
    }

    let cold = FlowSpine::flatten(&tree, 800.0, &estimate);
    assert_eq!(spine.total_height(), cold.total_height());
    assert_index_matches_scratch(&spine);
}

#[test]
fn width_epoch_does_not_walk_or_rewrite_heights() {
    let mut spine = FlowSpine::from_items(vec![
        content(1, 1, 10.0),
        content(2, 2, 20.0),
        content(3, 3, 30.0),
    ]);
    assert_eq!(spine.estimated_count(), 0);
    assert_eq!(spine.total_height(), 60.0);
    let before = spine.layout_epoch;
    spine.invalidate_layout_epoch();
    assert_eq!(spine.layout_epoch, before + 1);
    assert_eq!(spine.estimated_count(), 3);
    assert_eq!(spine.total_height(), 60.0);
    assert!(spine.item_at(1).height.is_exact());
    assert!(!spine.effective_height(1).is_exact());
    let id = FlowItemId {
        index: 2,
        generation: 1,
    };
    spine.set_height(id, HeightState::Exact(22.0));
    assert_eq!(spine.estimated_count(), 2);
    assert!(spine.effective_height(1).is_exact());
    assert!(!spine.effective_height(0).is_exact());
    assert_eq!(spine.total_height(), 62.0);
}

#[test]
fn width_change_updates_expansion_input_and_epoch() {
    let mut spine = FlowSpine::from_items(vec![content(1, 1, 10.0)]);
    spine.viewport_width = 320.0;
    let before = spine.layout_epoch;

    spine.set_viewport_width(640.0);

    assert_eq!(spine.viewport_width, 640.0);
    assert_eq!(spine.layout_epoch, before + 1);
    assert!(!spine.effective_height(0).is_exact());
}

#[test]
fn removed_flow_item_ids_are_reused_with_a_new_generation() {
    let mut spine = FlowSpine::from_items(vec![content(1, 1, 10.0)]);
    let old = spine.item_at(0).id;

    spine.splice(0, 1, Vec::new());
    let reused = spine.alloc_id();

    assert_eq!(reused.index, old.index);
    assert_eq!(reused.generation, old.generation + 1);
    assert!(spine.location(old).is_none());
}

#[test]
fn demote_content_keeps_item_id_and_total_height() {
    use std::fmt::Write;
    let mut md = String::new();
    for i in 0..8 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(4));
    }
    let doc = md_core::document::load_markdown(&md, md_core::document::editor_options());
    let tree = crate::compose::compose(&doc, &padded_theme());
    let mut spine = FlowSpine::flatten(&tree, 800.0, &|_, _| HeightState::Estimated(20.0));
    let (box_id, fid, pos) = (0..spine.len())
        .find_map(|pos| {
            let item = spine.item_at(pos);
            match item.kind {
                FlowItemKind::Content { box_id } => Some((box_id, item.id, pos)),
                _ => None,
            }
        })
        .expect("content");
    let total = spine.total_height();
    let px = spine.demote_content_to_collapsed(box_id).expect("demote");
    assert_eq!(px, 20.0);
    assert_eq!(spine.total_height(), total);
    assert_eq!(spine.location(fid), Some(pos));
    assert!(spine.content_id(box_id).is_none());
    assert_eq!(spine.collapsed_id(box_id), Some(fid));
    assert_records_on_tree(&spine, &tree);
}
