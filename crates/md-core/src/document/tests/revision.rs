use crate::doc::Doc;
use crate::document::change::{ChangeSet, DocChange};
use crate::document::edit::{Caret, Command, Sel, apply};
use crate::document::{DocumentArena, editor_options, load_markdown};

#[test]
fn tombstone_slots_stay_reserved_for_history() {
    let mut arena = DocumentArena::new();
    let old = arena.alloc(crate::block::BlockKind::Paragraph);
    arena.tombstone(old);
    let new = arena.alloc(crate::block::BlockKind::Paragraph);

    assert_ne!(new.index, old.index);
    assert!(arena.resurrect(old));
    assert!(arena.get(old).is_some());
    assert!(arena.get(new).is_some());
}

#[test]
fn text_changed_bumps_revision() {
    let mut doc = load_markdown("hello\n", editor_options());
    let _ = doc.take_changes();
    let leaf = doc.text_leaves().into_iter().next().expect("leaf");
    let before = doc.revision;
    let changes = doc.replace_text(leaf, 0..0, "X");
    assert_eq!(doc.revision, before + 1);
    assert!(!changes.is_structural());
    assert!(doc.text_of(leaf).unwrap().starts_with('X'));
    assert!(matches!(
        changes.changes.first(),
        Some(DocChange::TextChanged { .. })
    ));
}

#[test]
fn revision_saturation_does_not_reenter_pristine_value() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc
        .text_leaves()
        .into_iter()
        .next()
        .and_then(|index| doc.live_id(index))
        .expect("leaf");
    if let Some(node) = doc.arena.get_mut(leaf) {
        node.content_revision = u64::MAX;
        node.structure_revision = u64::MAX;
    }
    doc.texts
        .get_mut(leaf.text_id())
        .expect("leaf text")
        .revision = u64::MAX;
    doc.max_content_revision = u64::MAX;

    doc.bump_structure(leaf);
    assert_eq!(
        doc.arena.get(leaf).expect("node").structure_revision,
        u64::MAX
    );
    assert_eq!(doc.bump_content(leaf), u64::MAX);
    assert_eq!(
        doc.arena.get(leaf).expect("node").content_revision,
        u64::MAX
    );
}

#[test]
fn max_content_revision_cache_tracks_leaf_edits() {
    let mut doc = load_markdown("first\n\nsecond\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = doc.take_changes();

    doc.replace_text(leaves[0], 0..0, "a");
    doc.replace_text(leaves[0], 0..0, "b");
    doc.replace_text(leaves[1], 0..0, "c");

    let arena_max = doc
        .preorder()
        .into_iter()
        .filter_map(|id| doc.arena.get(id).map(|node| node.content_revision))
        .max()
        .unwrap_or(1);
    assert_eq!(doc.max_content_revision, arena_max);
}

#[test]
fn split_leaf_emits_tree_spliced() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves().into_iter().next().expect("leaf");
    let parent = doc.arena.get(doc.live_id(leaf).unwrap()).unwrap().parent;
    let before = doc
        .arena
        .get(parent.unwrap())
        .map(|n| n.structure_revision)
        .unwrap();
    let (changes, _) = doc.split_leaf(leaf, 2);
    assert!(
        !changes.is_replace(),
        "the operation delta must exclude the pending load replacement"
    );
    assert!(
        changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::TextChanged { .. }))
    );
    assert!(
        changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::TreeSpliced { .. }))
    );
    let after = doc
        .arena
        .get(parent.unwrap())
        .map(|n| n.structure_revision)
        .unwrap();
    assert!(after > before);
}

#[test]
fn undo_and_redo_rebuild_a_pasted_split_tail() {
    let mut doc = Doc::new(load_markdown(
        "# heading\n\nalph**\na **bold** b\n\n> quoted\n",
        editor_options(),
    ));
    let leaves = doc.text_leaves();
    let block = leaves[1];
    let _ = doc.apply(
        Sel::collapsed(Caret { block, offset: 8 }),
        Command::Paste {
            text: "para one\n\npara two".into(),
            intent: crate::document::PasteIntent::IndependentFragment,
        },
    );
    let edited = doc.document.to_markdown();
    assert!(edited.contains("para two"));
    assert!(
        edited.contains("bold"),
        "the pasted tail must keep its text"
    );
    let set = doc.take_changes();
    assert!(
        set.changes.iter().any(|c| matches!(
            c,
            DocChange::TextChanged { inserted, .. }
                if inserted.contains("bold")
        )),
        "the split tail fill must be recorded: {set:?}"
    );
    while doc.undo().is_some() {}
    while doc.redo().is_some() {}
    assert_eq!(doc.document.to_markdown(), edited);
}

#[test]
fn a_split_point_inside_an_escaped_pair_steps_back() {
    let mut doc = load_markdown("a \\` b\n", editor_options());
    let leaf = doc.text_leaves().into_iter().next().expect("leaf");
    let head = doc.live_id(leaf).unwrap();
    let (_, new_leaf) = doc.split_leaf(leaf, 2);
    let tail = doc.live_id(new_leaf).unwrap();
    assert_eq!(doc.leaf_source(head), "a ");
    assert_eq!(doc.leaf_source(tail), "\\` b");
}

#[test]
fn merge_emits_tree_spliced_on_removed_parent() {
    let mut doc = load_markdown("aa\n\nbb\n", editor_options());
    let leaves = doc.text_leaves();
    assert!(leaves.len() >= 2);
    let cur = leaves[1];
    let prev = leaves[0];
    let cur_id = doc.live_id(cur).unwrap();
    let parent = doc.arena.get(cur_id).unwrap().parent;
    let (changes, keep, _) = doc.merge_into_prev(cur).expect("merge");
    assert_eq!(keep, prev);
    assert!(changes.changes.iter().any(
        |c| matches!(c, DocChange::TextChanged { node, .. } if *node == doc.live_id(prev).unwrap())
    ));
    let spliced = changes
        .changes
        .iter()
        .find_map(|c| match c {
            DocChange::TreeSpliced {
                parent: p,
                removed,
                inserted,
                ..
            } => Some((*p, removed.clone(), inserted.clone())),
            _ => None,
        })
        .expect("spliced");
    assert_eq!(spliced.0, parent.unwrap());
    assert_eq!(spliced.1.len(), 1);
    assert!(spliced.2.is_empty());
    assert!(doc.live_id(cur).is_none());
}

#[test]
fn merging_into_a_revealed_survivor_emits_its_focus_text_change() {
    let mut doc = load_markdown("alpha **bold** beta\n\nnext\n", editor_options());
    let leaves = doc.text_leaves();
    let first = leaves[0];
    let second = leaves[1];
    let _ = doc.retarget_inline_focus(Caret {
        block: first,
        offset: 8,
    });
    let _ = doc.take_changes();

    let (changes, keep, _) = doc.merge_into_prev(second).expect("merge");
    assert_eq!(keep, first);
    let first_id = doc.live_id(first).expect("surviving leaf");
    let text_changes: Vec<_> = changes
        .changes
        .iter()
        .filter(|change| matches!(change, DocChange::TextChanged { node, .. } if *node == first_id))
        .collect();
    assert_eq!(text_changes.len(), 2, "focus collapse plus text merge");
    assert!(matches!(
        text_changes[0],
        DocChange::TextChanged {
            deleted,
            inserted,
            ..
        } if deleted.is_empty() && inserted.is_empty()
    ));
}

#[test]
fn is_text_only_excludes_structure_and_attrs() {
    let mut doc = load_markdown("hello\n\nworld\n", editor_options());
    let _ = doc.take_changes();
    let leaf = doc.text_leaves()[0];

    let text = doc.replace_text(leaf, 0..0, "X");
    assert!(text.is_text_only());
    assert!(!text.is_structural());

    let (split, _) = doc.split_leaf(leaf, 2);
    assert!(
        !split.is_text_only(),
        "Enter carries a TreeSpliced, so it is not text-only"
    );
    assert!(
        !ChangeSet::empty(doc.revision).is_text_only(),
        "an empty set does not count"
    );
    assert!(!ChangeSet::document_replaced(1).is_text_only());
}

#[test]
fn attrs_changed_is_not_text_only_though_it_is_not_structural() {
    let mut doc = load_markdown("- [ ] task\n", editor_options());
    doc.reset_changes();
    let leaf = doc.text_leaves()[0];

    apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::ToggleTask,
    );
    let set = doc.take_changes();

    assert!(
        set.changes
            .iter()
            .any(|c| matches!(c, DocChange::AttrsChanged { .. })),
        "ticking a task item must emit an AttrsChanged, or this case tests nothing"
    );
    assert!(!set.is_structural(), "it is indeed not a structural change");
    assert!(
        !set.is_text_only(),
        "but it must not count as text-only either"
    );
}
