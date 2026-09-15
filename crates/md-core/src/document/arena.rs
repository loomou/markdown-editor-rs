use crate::block::{BlockKind, NodeExtra};
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroU32;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId {
    pub index: u32,
    pub generation: NonZeroU32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TextId {
    pub index: u32,
    pub generation: NonZeroU32,
}

impl NodeId {
    pub fn at(index: u32, generation: u32) -> Self {
        NodeId {
            index,
            generation: issued_gen(generation),
        }
    }

    pub fn text_id(self) -> TextId {
        TextId {
            index: self.index,
            generation: self.generation,
        }
    }
}

fn issued_gen(g: u32) -> NonZeroU32 {
    NonZeroU32::new(g).expect("issued NodeId generation is never 0")
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: BlockKind,
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub prev_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub text: Option<TextId>,
    pub content_revision: u64,
    pub structure_revision: u64,
    pub extra: NodeExtra,
}

impl Node {
    fn new(kind: BlockKind) -> Self {
        Node {
            kind,
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            text: None,
            content_revision: 1,
            structure_revision: 1,
            extra: NodeExtra::None,
        }
    }
}

#[derive(Clone)]
struct Slot {
    generation: u32,
    live: bool,
    retired: bool,
    node: Node,
}

#[derive(Clone)]
pub struct DocumentArena {
    slots: Vec<Slot>,
    frozen: HashMap<u32, Node>,
    retained: Vec<u32>,
    free_list: Vec<u32>,
}

impl DocumentArena {
    pub fn new() -> Self {
        DocumentArena {
            slots: vec![Slot {
                generation: 0,
                live: false,
                retired: false,
                node: Node::new(BlockKind::DocRoot),
            }],
            frozen: HashMap::new(),
            retained: vec![0],
            free_list: Vec::new(),
        }
    }

    pub(crate) fn retain(&mut self, id: NodeId) {
        self.retain_adjust(id, 1);
    }

    pub(crate) fn release(&mut self, id: NodeId) {
        self.retain_adjust(id, -1);
    }

    fn retain_adjust(&mut self, id: NodeId, delta: i64) {
        let Some(slot) = self.slots.get(id.index as usize) else {
            debug_assert!(false, "retain/release points beyond the slot table");
            return;
        };
        debug_assert_eq!(
            slot.generation,
            id.generation.get(),
            "retain/release generation mismatch: a stack entry survived slot reuse"
        );
        let count = &mut self.retained[id.index as usize];
        let next = *count as i64 + delta;
        debug_assert!(next >= 0, "release underflowed the retained count");
        *count = next.max(0) as u32;
    }

    #[cfg(test)]
    pub(crate) fn retained_count(&self, index: u32) -> u32 {
        self.retained.get(index as usize).copied().unwrap_or(0)
    }

    pub(crate) fn frozen_closure(&self, roots: &[NodeId]) -> HashSet<u32> {
        let mut seen: HashSet<u32> = HashSet::new();
        let mut queue: VecDeque<u32> = VecDeque::new();
        for id in roots {
            let Some(slot) = self.slots.get(id.index as usize) else {
                continue;
            };
            if slot.live {
                continue;
            }
            if seen.insert(id.index) {
                queue.push_back(id.index);
            }
        }
        while let Some(index) = queue.pop_front() {
            let Some(frozen) = self.frozen.get(&index) else {
                continue;
            };
            for next in [
                frozen.first_child,
                frozen.last_child,
                frozen.prev_sibling,
                frozen.next_sibling,
            ]
            .into_iter()
            .flatten()
            {
                if seen.insert(next.index) {
                    queue.push_back(next.index);
                }
            }
        }
        seen
    }

    pub(crate) fn retire_unreferenced(&mut self, protected: &HashSet<u32>) -> Vec<u32> {
        let mut out = Vec::new();
        for index in 1..self.slots.len() as u32 {
            let slot = &self.slots[index as usize];
            if slot.live || slot.retired {
                continue;
            }
            if self.retained[index as usize] != 0 || protected.contains(&index) {
                continue;
            }
            self.slots[index as usize].retired = true;
            self.frozen.remove(&index);
            self.free_list.push(index);
            out.push(index);
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn free_list_len(&self) -> usize {
        self.free_list.len()
    }
}

impl Default for DocumentArena {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentArena {
    pub fn live_count(&self) -> usize {
        self.slots.iter().skip(1).filter(|s| s.live).count()
    }

    pub fn tombstone_count(&self) -> usize {
        self.slots
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, s)| !s.live)
            .count()
    }

    pub(crate) fn alloc(&mut self, kind: BlockKind) -> NodeId {
        if let Some(index) = self.free_list.pop() {
            let slot = &mut self.slots[index as usize];
            slot.generation += 1;
            slot.live = true;
            slot.retired = false;
            slot.node = Node::new(kind);
            let generation = slot.generation;
            self.retained[index as usize] = 0;
            return NodeId {
                index,
                generation: issued_gen(generation),
            };
        }
        let index = self.slots.len() as u32;
        let generation = 1;
        self.slots.push(Slot {
            generation,
            live: true,
            retired: false,
            node: Node::new(kind),
        });
        self.retained.push(0);
        NodeId {
            index,
            generation: issued_gen(generation),
        }
    }

    pub(crate) fn tombstone(&mut self, id: NodeId) {
        let Some(slot) = self.slots.get(id.index as usize) else {
            return;
        };
        if !slot.live || slot.generation != id.generation.get() {
            return;
        }
        self.freeze(id.index, false);
        self.slots[id.index as usize].live = false;
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.live && slot.generation == id.generation.get() {
            Some(&slot.node)
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.live && slot.generation == id.generation.get() {
            Some(&mut slot.node)
        } else {
            None
        }
    }

    pub(crate) fn snapshot(&mut self, id: NodeId) {
        let Some(slot) = self.slots.get(id.index as usize) else {
            return;
        };
        if !slot.live || slot.generation != id.generation.get() {
            return;
        }
        self.freeze(id.index, true);
    }

    pub(crate) fn resurrect(&mut self, id: NodeId) -> bool {
        let Some(slot) = self.slots.get(id.index as usize) else {
            return false;
        };
        if slot.generation != id.generation.get() {
            return false;
        }
        debug_assert!(
            !slot.retired,
            "resurrect reached a retired slot: the history bookkeeping lost a reference"
        );
        let index = id.index;
        if let Some(frozen) = self.frozen.remove(&index) {
            self.slots[index as usize].node = frozen;
        }
        let slot = &mut self.slots[index as usize];
        slot.node.parent = None;
        slot.node.prev_sibling = None;
        slot.node.next_sibling = None;
        slot.live = true;
        true
    }

    fn freeze(&mut self, index: u32, overwrite: bool) {
        if !overwrite && self.frozen.contains_key(&index) {
            return;
        }
        let node = self.slots[index as usize].node.clone();
        self.frozen.insert(index, node);
    }

    pub(crate) fn node_any(&self, id: NodeId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation == id.generation.get() {
            Some(&slot.node)
        } else {
            None
        }
    }

    pub(crate) fn frozen_node(&self, id: NodeId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation != id.generation.get() {
            return None;
        }
        self.frozen.get(&id.index)
    }

    pub(crate) fn live_at(&self, index: u32) -> Option<NodeId> {
        let slot = self.slots.get(index as usize)?;
        if slot.live {
            Some(NodeId {
                index,
                generation: issued_gen(slot.generation),
            })
        } else {
            None
        }
    }

    pub fn detach(&mut self, id: NodeId) {
        let Some(node) = self.get(id) else {
            return;
        };
        let parent = node.parent;
        let prev = node.prev_sibling;
        let next = node.next_sibling;
        if let Some(p) = parent
            && let Some(pn) = self.get_mut(p)
        {
            if pn.first_child == Some(id) {
                pn.first_child = next;
            }
            if pn.last_child == Some(id) {
                pn.last_child = prev;
            }
        }
        if let Some(prev) = prev
            && let Some(n) = self.get_mut(prev)
        {
            n.next_sibling = next;
        }
        if let Some(next) = next
            && let Some(n) = self.get_mut(next)
        {
            n.prev_sibling = prev;
        }
        if let Some(n) = self.get_mut(id) {
            n.parent = None;
            n.prev_sibling = None;
            n.next_sibling = None;
        }
    }

    pub(crate) fn insert_after(
        &mut self,
        parent: NodeId,
        after: Option<NodeId>,
        child: NodeId,
    ) -> bool {
        let valid_parent = self.get(parent).is_some();
        let valid_child = self.get(child).is_some() && child != parent && Some(child) != after;
        let valid_anchor =
            after.is_none_or(|id| self.get(id).is_some_and(|node| node.parent == Some(parent)));
        if !valid_parent || !valid_child || !valid_anchor {
            return false;
        }

        self.detach(child);
        let next = match after {
            Some(prev) => self.get(prev).and_then(|n| n.next_sibling),
            None => self.get(parent).and_then(|n| n.first_child),
        };
        if let Some(c) = self.get_mut(child) {
            c.parent = Some(parent);
            c.prev_sibling = after;
            c.next_sibling = next;
        }
        match after {
            Some(prev) => {
                if let Some(n) = self.get_mut(prev) {
                    n.next_sibling = Some(child);
                }
            }
            None => {
                if let Some(p) = self.get_mut(parent) {
                    p.first_child = Some(child);
                }
            }
        }
        if let Some(next) = next
            && let Some(n) = self.get_mut(next)
        {
            n.prev_sibling = Some(child);
        }
        if let Some(p) = self.get_mut(parent) {
            if after == p.last_child || p.last_child.is_none() {
                p.last_child = Some(child);
            }
            if p.first_child.is_none() {
                p.first_child = Some(child);
            }
        }
        true
    }

    pub(crate) fn append_child(&mut self, parent: NodeId, child: NodeId) -> bool {
        let last = self.get(parent).and_then(|p| p.last_child);
        if last == Some(child) {
            return self
                .get(child)
                .is_some_and(|node| node.parent == Some(parent));
        }
        self.insert_after(parent, last, child)
    }

    pub fn children(&self, parent: NodeId) -> Children<'_> {
        Children {
            arena: self,
            next: self.get(parent).and_then(|n| n.first_child),
        }
    }
}

pub struct Children<'a> {
    arena: &'a DocumentArena,
    next: Option<NodeId>,
}

impl<'a> Iterator for Children<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        self.next = self.arena.get(id).and_then(|n| n.next_sibling);
        Some(id)
    }
}

#[cfg(test)]
mod sizes {
    use super::{Node, NodeId, Slot, TextId};
    use std::mem::size_of;

    #[test]
    fn node_id_has_a_niche() {
        assert_eq!(size_of::<NodeId>(), 8);
        assert_eq!(size_of::<Option<NodeId>>(), 8);
        assert_eq!(size_of::<TextId>(), 8);
        assert_eq!(size_of::<Option<TextId>>(), 8);
    }

    #[test]
    fn node_and_slot_shrink_with_the_niche() {
        assert_eq!(size_of::<Node>(), 96);
        assert_eq!(size_of::<Option<Node>>(), 96);
        assert_eq!(size_of::<Slot>(), 104);
    }

    #[test]
    #[should_panic(expected = "issued NodeId generation is never 0")]
    fn a_zero_generation_cannot_be_issued() {
        let _ = NodeId::at(1, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentArena, NodeId};
    use crate::block::BlockKind;

    #[test]
    fn retained_tracks_the_stack_refs_of_a_slot() {
        let mut arena = DocumentArena::new();
        let a = arena.alloc(BlockKind::Paragraph);
        assert_eq!(arena.retained_count(a.index), 0);

        arena.retain(a);
        arena.retain(a);
        assert_eq!(arena.retained_count(a.index), 2);
        arena.release(a);
        assert_eq!(arena.retained_count(a.index), 1);
        arena.release(a);
        assert_eq!(arena.retained_count(a.index), 0);
    }

    #[test]
    #[should_panic(expected = "retain/release generation mismatch")]
    fn retain_rejects_a_stale_generation() {
        let mut arena = DocumentArena::new();
        let a = arena.alloc(BlockKind::Paragraph);
        arena.retain(NodeId::at(a.index, 2));
    }

    #[test]
    #[should_panic(expected = "release underflowed the retained count")]
    fn release_below_zero_panics() {
        let mut arena = DocumentArena::new();
        let a = arena.alloc(BlockKind::Paragraph);
        arena.release(a);
    }

    #[test]
    fn every_new_slot_starts_unretained() {
        let mut arena = DocumentArena::new();
        for _ in 0..17 {
            arena.alloc(BlockKind::Paragraph);
        }
        for i in 0..=17u32 {
            assert_eq!(arena.retained_count(i), 0);
        }
        assert_eq!(arena.retained_count(99), 0);
    }

    #[test]
    fn a_recycled_slot_issues_a_fresh_generation_and_old_ids_miss() {
        let mut arena = DocumentArena::new();
        let parent = arena.alloc(BlockKind::DocRoot);
        let old = arena.alloc(BlockKind::Paragraph);
        arena.append_child(parent, old);
        arena.tombstone(old);

        let closure = arena.frozen_closure(&[]);
        assert_eq!(arena.retire_unreferenced(&closure), vec![old.index]);
        assert_eq!(arena.free_list_len(), 1);
        let closure = arena.frozen_closure(&[]);
        assert!(arena.retire_unreferenced(&closure).is_empty());

        let fresh = arena.alloc(BlockKind::Heading(2));
        assert_eq!(fresh.index, old.index);
        assert!(fresh.generation.get() > old.generation.get());
        assert!(arena.get(old).is_none());
        assert!(arena.node_any(old).is_none());
        assert!(arena.frozen_node(old).is_none());
        assert!(!arena.resurrect(old));
        assert_eq!(arena.live_at(old.index), Some(fresh));
        arena.tombstone(old);
        assert!(arena.get(fresh).is_some());
    }

    #[test]
    fn retire_never_takes_a_live_slot() {
        let mut arena = DocumentArena::new();
        let _live = arena.alloc(BlockKind::Paragraph);
        let closure = arena.frozen_closure(&[]);
        assert!(arena.retire_unreferenced(&closure).is_empty());
        assert_eq!(arena.free_list_len(), 0);
    }

    #[test]
    fn counts_exclude_the_sentinel_slot() {
        let mut arena = DocumentArena::new();
        assert_eq!((arena.live_count(), arena.tombstone_count()), (0, 0));

        let node = arena.alloc(BlockKind::Paragraph);
        assert_eq!((arena.live_count(), arena.tombstone_count()), (1, 0));

        arena.tombstone(node);
        assert_eq!((arena.live_count(), arena.tombstone_count()), (0, 1));
    }

    #[test]
    fn alloc_does_not_fill_the_frozen_table() {
        let mut arena = DocumentArena::new();
        for _ in 0..32 {
            arena.alloc(BlockKind::Paragraph);
        }
        assert!(arena.frozen.is_empty());
    }

    #[test]
    fn tombstone_keeps_a_prior_snapshot() {
        let mut arena = DocumentArena::new();
        let id = arena.alloc(BlockKind::Paragraph);
        arena.snapshot(id);
        assert_eq!(arena.frozen.len(), 1);
        arena.tombstone(id);
        assert_eq!(arena.frozen.len(), 1);
    }

    #[test]
    fn insert_after_rejects_a_tombstoned_child_without_truncating_siblings() {
        let mut arena = DocumentArena::new();
        let parent = arena.alloc(BlockKind::DocRoot);
        let first = arena.alloc(BlockKind::Paragraph);
        let dead = arena.alloc(BlockKind::Paragraph);
        let last = arena.alloc(BlockKind::Paragraph);
        arena.append_child(parent, first);
        arena.append_child(parent, last);
        arena.tombstone(dead);

        assert!(!arena.insert_after(parent, Some(first), dead));
        assert_eq!(
            arena.children(parent).collect::<Vec<_>>(),
            vec![first, last]
        );
        assert_eq!(
            arena.get(first).and_then(|node| node.next_sibling),
            Some(last)
        );
        assert_eq!(
            arena.get(last).and_then(|node| node.prev_sibling),
            Some(first)
        );
    }

    #[test]
    fn append_child_detaches_a_child_from_its_previous_parent() {
        let mut arena = DocumentArena::new();
        let first_parent = arena.alloc(BlockKind::BlockQuote);
        let second_parent = arena.alloc(BlockKind::BlockQuote);
        let moved = arena.alloc(BlockKind::Paragraph);
        let left = arena.alloc(BlockKind::Paragraph);
        let right = arena.alloc(BlockKind::Paragraph);
        arena.append_child(first_parent, moved);
        arena.append_child(first_parent, left);
        arena.append_child(second_parent, right);

        assert!(arena.append_child(second_parent, moved));
        assert_eq!(arena.children(first_parent).collect::<Vec<_>>(), vec![left]);
        assert_eq!(
            arena.children(second_parent).collect::<Vec<_>>(),
            vec![right, moved]
        );
        assert_eq!(
            arena.get(moved).and_then(|node| node.parent),
            Some(second_parent)
        );
        assert_eq!(
            arena.get(moved).and_then(|node| node.prev_sibling),
            Some(right)
        );
        assert_eq!(
            arena.get(right).and_then(|node| node.next_sibling),
            Some(moved)
        );
    }

    #[test]
    fn resurrect_clears_stale_parent_and_sibling_links() {
        let mut arena = DocumentArena::new();
        let parent = arena.alloc(BlockKind::BlockQuote);
        let first = arena.alloc(BlockKind::Paragraph);
        let restored = arena.alloc(BlockKind::Paragraph);
        let last = arena.alloc(BlockKind::Paragraph);
        arena.append_child(parent, first);
        arena.append_child(parent, restored);
        arena.append_child(parent, last);
        arena.snapshot(restored);
        arena.detach(restored);
        arena.tombstone(restored);

        assert!(arena.resurrect(restored));
        let node = arena.get(restored).expect("restored node");
        assert_eq!(node.parent, None);
        assert_eq!(node.prev_sibling, None);
        assert_eq!(node.next_sibling, None);
        assert_eq!(
            arena.children(parent).collect::<Vec<_>>(),
            vec![first, last]
        );
    }

    #[test]
    fn a_second_death_refreezes_rather_than_replaying_the_first_lifes_frozen() {
        let mut arena = DocumentArena::new();
        let parent = arena.alloc(BlockKind::BlockQuote);
        let child = arena.alloc(BlockKind::Paragraph);
        arena.append_child(parent, child);

        arena.snapshot(parent);
        arena.detach(child);
        arena.tombstone(parent);
        assert!(arena.resurrect(parent));

        if let Some(node) = arena.get_mut(parent) {
            node.first_child = None;
            node.last_child = None;
        }

        arena.tombstone(parent);
        assert!(arena.resurrect(parent));
        let node = arena.get(parent).expect("parent");
        assert_eq!(
            node.first_child, None,
            "the second resurrection must not replay the first life's child links"
        );
    }
}
