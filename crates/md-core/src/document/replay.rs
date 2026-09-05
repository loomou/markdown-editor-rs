use super::Document;
use super::arena::{DocumentArena, NodeId};
use super::change::{ChangeSet, DocChange};
use std::collections::{HashMap, HashSet};

struct LaterView {
    first_seen_from_tail: HashMap<NodeId, usize>,
}

impl LaterView {
    fn new(changes: &[DocChange]) -> Self {
        let mut first_seen_from_tail = HashMap::new();
        for (index, change) in changes.iter().enumerate().rev() {
            if let DocChange::TreeSpliced { inserted, .. } = change {
                for id in inserted {
                    first_seen_from_tail.entry(*id).or_insert(index);
                }
            }
        }
        Self {
            first_seen_from_tail,
        }
    }

    fn appears_after(&self, id: NodeId, index: usize) -> bool {
        self.first_seen_from_tail
            .get(&id)
            .is_some_and(|&at| at > index)
    }
}

struct Keep<'a> {
    local: HashSet<NodeId>,
    view: &'a LaterView,
    index: usize,
}

impl<'a> Keep<'a> {
    fn new(view: &'a LaterView, index: usize) -> Self {
        Self {
            local: HashSet::new(),
            view,
            index,
        }
    }

    fn contains(&self, id: NodeId) -> bool {
        self.local.contains(&id) || self.view.appears_after(id, self.index)
    }
}

trait SpliceHost {
    fn arena(&mut self) -> &mut DocumentArena;

    fn on_resurrect(&mut self, _id: NodeId) {}

    fn on_spliced(&mut self, _parent: NodeId) {}
}

impl SpliceHost for DocumentArena {
    fn arena(&mut self) -> &mut DocumentArena {
        self
    }
}

impl SpliceHost for Document {
    fn arena(&mut self) -> &mut DocumentArena {
        &mut self.arena
    }

    fn on_resurrect(&mut self, id: NodeId) {
        let _ = self.stamp_past_max(id);
    }

    fn on_spliced(&mut self, parent: NodeId) {
        self.bump_structure(parent);
    }
}

impl Document {
    pub(crate) fn apply_changes(&mut self, cs: ChangeSet) -> bool {
        if cs.is_empty() || cs.is_replace() {
            return true;
        }
        if cs.is_structural() && !self.preflight_tree_splices(&cs) {
            return false;
        }
        let focused = self.focus.take().map(|focus| focus.node);

        self.block_edit = None;
        let before = self.revision;
        let view = LaterView::new(&cs.changes);
        let mut out = Vec::new();
        if let Some(node) = focused {
            out.push(self.focus_text_change(node));
        }
        for (index, change) in cs.changes.into_iter().enumerate() {
            match change {
                DocChange::DocumentReplaced => {}
                DocChange::TextChanged {
                    node,
                    range,
                    deleted,
                    inserted,
                    ..
                } => {
                    let old_revision = self
                        .arena
                        .get(node)
                        .map(|n| n.content_revision)
                        .unwrap_or(1);
                    self.apply_recorded_text(node, range.clone(), &inserted);
                    let new_revision = self.stamp_past_max(node);
                    out.push(DocChange::text(
                        node,
                        old_revision,
                        new_revision,
                        range,
                        deleted,
                        inserted,
                    ));
                }
                DocChange::AttrsChanged {
                    node,
                    new_kind,
                    new_extra,
                    old_kind,
                    old_extra,
                } => {
                    let kind_changed = self.arena.get(node).is_some_and(|n| n.kind != new_kind);
                    if let Some(n) = self.arena.get_mut(node) {
                        n.kind = new_kind;
                        n.extra = new_extra;
                    }
                    if kind_changed && new_kind.is_text_leaf() {
                        self.reproject_current(node);
                    }
                    let _ = self.stamp_past_max(node);
                    out.push(DocChange::attrs(
                        node, old_kind, new_kind, old_extra, new_extra,
                    ));
                }
                DocChange::TreeSpliced {
                    parent,
                    before,
                    removed,
                    inserted,
                } => {
                    let mut keep = Keep::new(&view, index);
                    if !splice_tree(self, parent, before, &removed, &inserted, &mut keep) {
                        debug_assert!(false, "tree splice preflight diverged");
                        return false;
                    }
                    out.push(DocChange::TreeSpliced {
                        parent,
                        before,
                        removed,
                        inserted,
                    });
                }
            }
        }
        let _ = self.commit(before, out);
        true
    }

    fn preflight_tree_splices(&self, cs: &ChangeSet) -> bool {
        let mut arena = self.arena.clone();
        let view = LaterView::new(&cs.changes);
        for (index, change) in cs.changes.iter().enumerate() {
            let DocChange::TreeSpliced {
                parent,
                before,
                removed,
                inserted,
            } = change
            else {
                continue;
            };
            let mut keep = Keep::new(&view, index);
            if !splice_tree(&mut arena, *parent, *before, removed, inserted, &mut keep) {
                return false;
            }
        }
        true
    }

    fn stamp_past_max(&mut self, id: NodeId) -> u64 {
        let next = self.max_content_revision.saturating_add(1);
        self.max_content_revision = next;
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.revision = next;
        }
        if let Some(n) = self.arena.get_mut(id) {
            n.content_revision = next;
        }
        next
    }
}

fn splice_tree<C: SpliceHost>(
    ctx: &mut C,
    parent: NodeId,
    before: Option<NodeId>,
    removed: &[NodeId],
    inserted: &[NodeId],
    keep: &mut Keep<'_>,
) -> bool {
    if ctx.arena().get(parent).is_none()
        || removed
            .iter()
            .any(|id| ctx.arena().get(*id).and_then(|node| node.parent) != Some(parent))
    {
        return false;
    }

    if removed == inserted {
        ctx.on_spliced(parent);
        return true;
    }

    for i in inserted {
        if removed.iter().any(|r| is_under(ctx.arena(), *i, *r)) {
            collect_descendants(ctx.arena(), *i, &mut keep.local);
        }
    }

    for r in removed {
        snapshot_dying(ctx.arena(), *r, keep);
    }
    for i in inserted {
        if keep.contains(*i) {
            ctx.arena().detach(*i);
        }
    }
    for r in removed {
        if ctx.arena().get(*r).and_then(|n| n.parent) == Some(parent) {
            ctx.arena().detach(*r);
        }
    }
    for i in inserted {
        if ctx.arena().get(*i).is_none() {
            resurrect_tree(ctx, *i);
        }
    }

    let mut at = before;
    let mut inserted_all = true;
    for i in inserted {
        if ctx.arena().insert_after(parent, at, *i) {
            at = Some(*i);
        } else {
            inserted_all = false;
        }
    }
    for r in removed {
        if ctx.arena().get(*r).and_then(|n| n.parent).is_none() && !inserted.contains(r) {
            tombstone_tree(ctx.arena(), *r, keep);
        }
    }

    ctx.on_spliced(parent);
    inserted_all
}

fn is_under(arena: &DocumentArena, id: NodeId, root: NodeId) -> bool {
    if id == root {
        return true;
    }
    let mut cur = arena.get(id).and_then(|n| n.parent);
    while let Some(parent) = cur {
        if parent == root {
            return true;
        }
        cur = arena.get(parent).and_then(|n| n.parent);
    }
    false
}

fn collect_descendants(arena: &DocumentArena, id: NodeId, out: &mut HashSet<NodeId>) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        if out.insert(id) {
            stack.extend(arena.children(id));
        }
    }
}

fn snapshot_dying(arena: &mut DocumentArena, id: NodeId, keep: &Keep<'_>) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        if keep.contains(id) {
            continue;
        }
        arena.snapshot(id);
        stack.extend(arena.children(id));
    }
}

fn resurrect_tree<C: SpliceHost>(ctx: &mut C, id: NodeId) {
    enum Action {
        Build(NodeId),
        Append(NodeId, NodeId),
    }
    let mut actions = vec![Action::Build(id)];
    while let Some(action) = actions.pop() {
        match action {
            Action::Build(id) => {
                let arena = ctx.arena();
                if !arena.resurrect(id) {
                    continue;
                }
                let children: Vec<NodeId> = arena
                    .get(id)
                    .map(|node| {
                        let mut out = Vec::new();
                        let mut cur = node.first_child;
                        while let Some(child) = cur {
                            out.push(child);
                            cur = arena.node_any(child).and_then(|n| n.next_sibling);
                        }
                        out
                    })
                    .unwrap_or_default();
                if let Some(node) = arena.get_mut(id) {
                    node.first_child = None;
                    node.last_child = None;
                }
                for child in children.into_iter().rev() {
                    actions.push(Action::Append(id, child));
                    if arena.get(child).is_none() {
                        actions.push(Action::Build(child));
                    } else {
                        arena.detach(child);
                    }
                }
                ctx.on_resurrect(id);
            }
            Action::Append(parent, child) => {
                ctx.arena().append_child(parent, child);
            }
        }
    }
}

fn tombstone_tree(arena: &mut DocumentArena, id: NodeId, keep: &Keep<'_>) {
    let mut stack = vec![(id, false)];
    while let Some((id, visited)) = stack.pop() {
        if keep.contains(id) {
            continue;
        }
        if visited {
            arena.tombstone(id);
        } else {
            stack.push((id, true));
            stack.extend(arena.children(id).map(|child| (child, false)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Keep, LaterView, splice_tree};
    use crate::block::{BlockKind, NodeExtra};
    use crate::document::Document;
    use crate::document::{ChangeSet, DocChange, NodeId, editor_options, load_markdown};
    use std::collections::HashSet;

    fn splice_alone(
        doc: &mut Document,
        parent: NodeId,
        before: Option<NodeId>,
        removed: &[NodeId],
        inserted: &[NodeId],
    ) -> bool {
        let view = LaterView::new(&[]);
        let mut keep = Keep::new(&view, 0);
        splice_tree(doc, parent, before, removed, inserted, &mut keep)
    }

    #[test]
    fn repeated_resurrection_uses_current_sibling_links() {
        let mut doc = load_markdown("a\n\nb\n\nc", editor_options());
        let root = doc.root;
        let original: Vec<_> = doc.arena.children(root).collect();
        let [a, b, c] = original.as_slice() else {
            panic!("expected three root children");
        };
        let (a, b, c) = (*a, *b, *c);

        splice_alone(&mut doc, root, Some(a), &[b], &[]);
        splice_alone(&mut doc, root, Some(a), &[], &[b]);

        let inserted = doc.alloc_leaf(BlockKind::Paragraph);
        splice_alone(&mut doc, root, Some(a), &[], &[inserted]);
        assert_eq!(
            doc.arena.children(root).collect::<Vec<_>>(),
            vec![a, inserted, b, c]
        );

        splice_alone(&mut doc, root, Some(inserted), &[b], &[]);
        splice_alone(&mut doc, root, Some(a), &[inserted], &[]);

        splice_alone(&mut doc, root, Some(a), &[], &[inserted]);
        assert_eq!(
            doc.arena.children(root).collect::<Vec<_>>(),
            vec![a, inserted, c],
            "restoring one node must not reconnect its still-dead old successor"
        );

        splice_alone(&mut doc, root, Some(inserted), &[], &[b]);
        assert_eq!(
            doc.arena.children(root).collect::<Vec<_>>(),
            vec![a, inserted, b, c]
        );
    }

    #[test]
    fn replay_keeps_the_last_live_anchor_after_a_failed_insert() {
        let mut doc = load_markdown("a\n\nb\n", editor_options());
        let root = doc.root;
        let original = doc.arena.children(root).collect::<Vec<_>>();
        let restored = doc.arena.alloc(BlockKind::Paragraph);
        doc.arena.tombstone(restored);
        let missing = NodeId::at(u32::MAX, 1);

        splice_alone(&mut doc, root, Some(original[0]), &[], &[missing, restored]);

        assert_eq!(
            doc.arena.children(root).collect::<Vec<_>>(),
            vec![original[0], restored, original[1]]
        );
    }

    #[test]
    fn replay_rolls_back_when_a_removed_node_has_a_different_parent() {
        let mut doc = load_markdown("a\n\nb\n", editor_options());
        let root = doc.root;
        let paragraphs = doc.arena.children(root).collect::<Vec<_>>();
        let quote = doc.arena.alloc(BlockKind::BlockQuote);
        doc.arena.append_child(root, quote);
        doc.arena.detach(paragraphs[1]);
        doc.arena.append_child(quote, paragraphs[1]);
        let revision = doc.revision();
        let before = doc.to_markdown();
        let changes = ChangeSet {
            before_revision: revision,
            after_revision: revision.saturating_add(1),
            changes: vec![DocChange::TreeSpliced {
                parent: root,
                before: Some(paragraphs[0]),
                removed: vec![paragraphs[1]],
                inserted: Vec::new(),
            }],
        };

        assert!(!doc.apply_changes(changes));
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.to_markdown(), before);
        assert_eq!(
            doc.arena.get(paragraphs[1]).and_then(|node| node.parent),
            Some(quote)
        );
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    #[test]
    fn later_view_matches_the_per_change_set_construction() {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        for round in 0..300 {
            let len = 1 + (rng.next() % 8) as usize;
            let mut changes = Vec::with_capacity(len);
            for _ in 0..len {
                if rng.next().is_multiple_of(3) {
                    changes.push(DocChange::AttrsChanged {
                        node: NodeId::at(0, 1),
                        new_kind: BlockKind::Paragraph,
                        new_extra: NodeExtra::None,
                        old_kind: BlockKind::Paragraph,
                        old_extra: NodeExtra::None,
                    });
                } else {
                    let mut inserted = Vec::new();
                    for _ in 0..(rng.next() % 3) {
                        inserted.push(NodeId::at((rng.next() % 6) as u32, 1));
                    }
                    changes.push(DocChange::TreeSpliced {
                        parent: NodeId::at(0, 1),
                        before: None,
                        removed: Vec::new(),
                        inserted,
                    });
                }
            }

            let mut later: HashSet<NodeId> = HashSet::new();
            let mut rehomed_at: Vec<HashSet<NodeId>> = vec![HashSet::new(); changes.len()];
            for (index, change) in changes.iter().enumerate().rev() {
                rehomed_at[index] = later.clone();
                if let DocChange::TreeSpliced { inserted, .. } = change {
                    later.extend(inserted.iter().copied());
                }
            }
            let view = LaterView::new(&changes);
            for (index, rehomed) in rehomed_at.iter().enumerate() {
                for raw in 0..6u32 {
                    let id = NodeId::at(raw, 1);
                    assert_eq!(
                        view.appears_after(id, index),
                        rehomed.contains(&id),
                        "round={round} index={index} id={raw}"
                    );
                }
            }
        }
    }
}
