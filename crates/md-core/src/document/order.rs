use super::{Document, NodeId};
use std::cmp::Ordering;

#[derive(Clone)]
pub(crate) struct OrderIndex {
    epoch: u64,
    rank: Vec<u32>,
}

impl OrderIndex {
    pub(crate) fn empty() -> Self {
        OrderIndex {
            epoch: u64::MAX,
            rank: Vec::new(),
        }
    }
}

#[cfg(test)]
std::thread_local! {
    pub(super) static REBUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn rebuilds() -> usize {
    REBUILDS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn reset_rebuilds() {
    REBUILDS.with(|count| count.set(0));
}

impl Document {
    pub(crate) fn order_compare(&self, a: NodeId, b: NodeId) -> Option<Ordering> {
        let epoch = self.arena.structure_epoch();
        let mut order = self.order.borrow_mut();
        if order.epoch != epoch {
            order.rank = self.build_rank();
            order.epoch = epoch;
        }
        let a_rank = *order.rank.get(a.index as usize)?;
        let b_rank = *order.rank.get(b.index as usize)?;
        if a_rank == 0 || b_rank == 0 {
            return None;
        }
        Some(a_rank.cmp(&b_rank))
    }

    fn build_rank(&self) -> Vec<u32> {
        #[cfg(test)]
        REBUILDS.with(|count| count.set(count.get() + 1));
        let mut rank = vec![0u32; self.arena.slot_count()];
        let mut next = 1u32;
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            rank[id.index as usize] = next;
            next += 1;
            let mut child = self.arena.get(id).and_then(|node| node.last_child);
            while let Some(c) = child {
                stack.push(c);
                child = self.arena.get(c).and_then(|node| node.prev_sibling);
            }
        }
        rank
    }
}
