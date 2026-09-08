use super::ids::{BoxOwner, BoxRole, LayoutBoxId};
use super::node::BoxNode;

const BOX_NONE: u32 = u32::MAX;

#[derive(Clone, Debug, Default)]
struct BoxColumn {
    index: Vec<u32>,
    dense: Vec<Option<Box<BoxNode>>>,
    free: Vec<u32>,
}

impl BoxColumn {
    fn ensure_index(&mut self, block: u32) {
        let i = block as usize;
        if i >= self.index.len() {
            self.index.resize(i + 1, BOX_NONE);
        }
    }

    fn trim_trailing_none(&mut self) {
        while self.index.last().copied() == Some(BOX_NONE) {
            self.index.pop();
        }
    }

    fn alloc_dense(&mut self, boxed: Box<BoxNode>) -> u32 {
        if let Some(i) = self.free.pop() {
            self.dense[i as usize] = Some(boxed);
            return i;
        }
        let i = u32::try_from(self.dense.len()).expect("BoxStore dense exhausted");
        if i == BOX_NONE {
            panic!("BoxStore dense index would collide with the BOX_NONE sentinel");
        }
        self.dense.push(Some(boxed));
        i
    }

    fn get(&self, block: u32) -> Option<&BoxNode> {
        let dense = *self.index.get(block as usize)?;
        if dense == BOX_NONE {
            return None;
        }
        self.dense.get(dense as usize).and_then(|s| s.as_deref())
    }

    fn get_mut(&mut self, block: u32) -> Option<&mut BoxNode> {
        let dense = *self.index.get(block as usize)?;
        if dense == BOX_NONE {
            return None;
        }
        self.dense
            .get_mut(dense as usize)
            .and_then(|s| s.as_deref_mut())
    }

    fn insert(&mut self, block: u32, boxed: Box<BoxNode>) -> Option<BoxNode> {
        self.ensure_index(block);
        let slot = self.index[block as usize];
        if slot != BOX_NONE {
            return self.dense[slot as usize].replace(boxed).map(|b| *b);
        }
        let dense = self.alloc_dense(boxed);
        self.index[block as usize] = dense;
        None
    }

    fn remove(&mut self, block: u32) -> Option<BoxNode> {
        let slot = self.index.get_mut(block as usize)?;
        let dense = *slot;
        if dense == BOX_NONE {
            return None;
        }
        *slot = BOX_NONE;
        let node = self.dense[dense as usize].take().map(|b| *b);
        self.free.push(dense);
        self.trim_trailing_none();
        node
    }
}

#[derive(Clone, Debug, Default)]
pub struct BoxStore {
    frames: BoxColumn,
    previews: BoxColumn,
    cells: BoxColumn,
    bars: BoxColumn,
    slots: BoxColumn,
    doc_start: Option<Box<BoxNode>>,
    count: usize,
}

impl BoxStore {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn column(&self, role: BoxRole) -> &BoxColumn {
        match role {
            BoxRole::Frame => &self.frames,
            BoxRole::Preview => &self.previews,
            BoxRole::Cell => &self.cells,
            BoxRole::Bar => &self.bars,
            BoxRole::Slot => &self.slots,
        }
    }

    fn column_mut(&mut self, role: BoxRole) -> &mut BoxColumn {
        match role {
            BoxRole::Frame => &mut self.frames,
            BoxRole::Preview => &mut self.previews,
            BoxRole::Cell => &mut self.cells,
            BoxRole::Bar => &mut self.bars,
            BoxRole::Slot => &mut self.slots,
        }
    }

    pub fn get(&self, id: &LayoutBoxId) -> Option<&BoxNode> {
        match id.owner {
            BoxOwner::DocStart => self.doc_start.as_deref(),
            BoxOwner::Block(b) => self.column(id.role).get(b),
        }
    }

    pub(crate) fn get_mut(&mut self, id: &LayoutBoxId) -> Option<&mut BoxNode> {
        match id.owner {
            BoxOwner::DocStart => self.doc_start.as_deref_mut(),
            BoxOwner::Block(b) => self.column_mut(id.role).get_mut(b),
        }
    }

    pub fn contains_key(&self, id: &LayoutBoxId) -> bool {
        self.get(id).is_some()
    }

    pub(crate) fn insert(&mut self, id: LayoutBoxId, node: BoxNode) -> Option<BoxNode> {
        let boxed = Box::new(node);
        let old = match id.owner {
            BoxOwner::DocStart => self.doc_start.replace(boxed).map(|b| *b),
            BoxOwner::Block(b) => self.column_mut(id.role).insert(b, boxed),
        };
        if old.is_none() {
            self.count = self.count.saturating_add(1);
        }
        old
    }

    pub(crate) fn remove(&mut self, id: &LayoutBoxId) -> Option<BoxNode> {
        let old = match id.owner {
            BoxOwner::DocStart => self.doc_start.take().map(|b| *b),
            BoxOwner::Block(b) => self.column_mut(id.role).remove(b),
        };
        if old.is_some() {
            self.count = self.count.saturating_sub(1);
        }
        old
    }

    pub fn values(&self) -> impl Iterator<Item = &BoxNode> {
        self.into_iter().map(|(_, n)| n)
    }

    pub(crate) fn live_nodes(&self) -> impl Iterator<Item = &BoxNode> + '_ {
        let columns = [
            self.frames.dense.as_slice(),
            self.previews.dense.as_slice(),
            self.cells.dense.as_slice(),
            self.bars.dense.as_slice(),
            self.slots.dense.as_slice(),
        ];
        self.doc_start.as_deref().into_iter().chain(
            columns
                .into_iter()
                .flatten()
                .filter_map(|slot| slot.as_deref()),
        )
    }

    #[cfg(test)]
    pub(crate) fn column_index_len(&self, role: BoxRole) -> usize {
        self.column(role).index.len()
    }

    #[cfg(test)]
    pub(crate) fn column_dense_len(&self, role: BoxRole) -> usize {
        self.column(role).dense.len()
    }

    #[cfg(test)]
    pub(crate) fn column_free_len(&self, role: BoxRole) -> usize {
        self.column(role).free.len()
    }
}

enum BoxStoreIterState {
    Start,
    Doc,
    Scan { role: u8, i: usize },
    Done,
}

pub struct BoxStoreIter<'a> {
    store: &'a BoxStore,
    state: BoxStoreIterState,
}

impl<'a> Iterator for BoxStoreIter<'a> {
    type Item = (&'a LayoutBoxId, &'a BoxNode);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.state {
                BoxStoreIterState::Start => {
                    self.state = BoxStoreIterState::Doc;
                }
                BoxStoreIterState::Doc => {
                    self.state = BoxStoreIterState::Scan { role: 0, i: 0 };
                    if let Some(n) = self.store.doc_start.as_deref() {
                        return Some((&n.id, n));
                    }
                }
                BoxStoreIterState::Scan { role, i } => {
                    if role > 4 {
                        self.state = BoxStoreIterState::Done;
                        continue;
                    }
                    let column = match role {
                        0 => &self.store.frames,
                        1 => &self.store.previews,
                        2 => &self.store.cells,
                        3 => &self.store.bars,
                        _ => &self.store.slots,
                    };
                    let Some(&dense) = column.index.get(i) else {
                        self.state = BoxStoreIterState::Scan {
                            role: role + 1,
                            i: 0,
                        };
                        continue;
                    };
                    self.state = BoxStoreIterState::Scan { role, i: i + 1 };
                    if dense == BOX_NONE {
                        continue;
                    }
                    if let Some(n) = column.dense.get(dense as usize).and_then(|s| s.as_deref()) {
                        return Some((&n.id, n));
                    }
                }
                BoxStoreIterState::Done => return None,
            }
        }
    }
}

impl<'a> IntoIterator for &'a BoxStore {
    type Item = (&'a LayoutBoxId, &'a BoxNode);
    type IntoIter = BoxStoreIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        BoxStoreIter {
            store: self,
            state: BoxStoreIterState::Start,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BoxStore;
    use crate::box_tree::{BoxChildren, BoxNode, BoxRole, BoxStyleId, LayoutBoxId, TypeSlot};
    use md_core::block::{BlockKind, NodeExtra};

    fn node(id: LayoutBoxId) -> BoxNode {
        BoxNode {
            id,
            kind: BlockKind::Paragraph,
            style_id: BoxStyleId::ZERO,
            parent: None,
            children: BoxChildren::None,
            text_id: None,
            extra: NodeExtra::None,
            content_revision: 1,
            content_generation: 1,
            edit_source: false,
            type_slot: TypeSlot::FromKind,
        }
    }

    #[test]
    fn dense_len_does_not_follow_block_id() {
        let mut store = BoxStore::default();
        let id = LayoutBoxId::frame(100_000);
        store.insert(id, node(id));
        assert_eq!(store.len(), 1);
        assert!(store.contains_key(&id));
        assert_eq!(store.column_dense_len(BoxRole::Frame), 1);
        assert!(store.column_index_len(BoxRole::Frame) >= 100_001);
        assert_eq!(store.column_free_len(BoxRole::Frame), 0);
    }

    #[test]
    fn remove_trims_index_and_reuses_dense_via_free_list() {
        let mut store = BoxStore::default();
        let high = LayoutBoxId::frame(100_000);
        store.insert(high, node(high));
        assert!(store.remove(&high).is_some());
        assert!(store.get(&high).is_none());
        assert_eq!(store.len(), 0);
        assert_eq!(store.column_index_len(BoxRole::Frame), 0);
        assert_eq!(store.column_dense_len(BoxRole::Frame), 1);
        assert_eq!(store.column_free_len(BoxRole::Frame), 1);

        let low = LayoutBoxId::frame(3);
        store.insert(low, node(low));
        assert_eq!(store.column_dense_len(BoxRole::Frame), 1);
        assert_eq!(store.column_free_len(BoxRole::Frame), 0);
        assert_eq!(store.column_index_len(BoxRole::Frame), 4);
        assert!(store.contains_key(&low));
    }

    #[test]
    fn roles_do_not_share_columns() {
        let mut store = BoxStore::default();
        let frame = LayoutBoxId::frame(9);
        let bar = LayoutBoxId::bar(9);
        store.insert(frame, node(frame));
        store.insert(bar, node(bar));
        assert_eq!(store.len(), 2);
        assert_eq!(store.column_dense_len(BoxRole::Frame), 1);
        assert_eq!(store.column_dense_len(BoxRole::Bar), 1);
        assert!(store.get(&frame).is_some());
        assert!(store.get(&bar).is_some());
    }

    #[test]
    fn iter_skips_holes_and_keeps_block_order() {
        let mut store = BoxStore::default();
        let a = LayoutBoxId::frame(2);
        let b = LayoutBoxId::frame(9);
        store.insert(b, node(b));
        store.insert(a, node(a));
        let ids: Vec<_> = store.into_iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![a, b]);
    }

    #[test]
    fn live_nodes_follows_dense_not_index() {
        let mut store = BoxStore::default();
        let id = LayoutBoxId::frame(50_000);
        store.insert(id, node(id));
        assert_eq!(store.live_nodes().count(), store.len());
        assert!(store.column_index_len(BoxRole::Frame) > store.live_nodes().count());
    }
}
