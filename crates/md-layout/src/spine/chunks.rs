use super::item::{FlowItem, FlowItemId};
use std::ops::{Index, IndexMut, Range};

const CHUNK_CAPACITY: usize = 256;

pub(super) struct Chunked {
    slots: Vec<Vec<FlowItem>>,
    free: Vec<u32>,
    order: Vec<u32>,
    ends: Vec<usize>,
    base: Vec<usize>,
    total: usize,
    loc: Vec<u32>,
    owner: Vec<u32>,
    gens: Vec<u32>,
}

impl Chunked {
    pub(super) fn new() -> Self {
        Chunked {
            slots: vec![Vec::new()],
            free: Vec::new(),
            order: Vec::new(),
            ends: Vec::new(),
            base: vec![0],
            total: 0,
            loc: vec![0],
            owner: vec![0],
            gens: vec![0],
        }
    }

    pub(super) fn from_items(items: Vec<FlowItem>) -> Self {
        let mut chunked = Chunked::new();
        let len = items.len();
        if len > 0 {
            let count = len.div_ceil(CHUNK_CAPACITY);
            let mut it = items.into_iter();
            let mut offset = 0usize;
            for i in 0..count {
                let span = (len - offset) / (count - i);
                let slot = chunked.alloc_slot();
                chunked.slots[slot].extend(it.by_ref().take(span));
                chunked.order.push(slot as u32);
                offset += span;
            }
            debug_assert_eq!(offset, len);
            chunked.reindex();
            let slots = chunked.order.clone();
            chunked.remap_slots(&slots);
        }
        chunked
    }

    pub(super) fn len(&self) -> usize {
        self.total
    }

    pub(super) fn is_empty(&self) -> bool {
        self.total == 0
    }

    pub(super) fn get(&self, pos: usize) -> Option<&FlowItem> {
        (pos < self.total).then(|| self.index(pos))
    }

    pub(super) fn location(&self, id: FlowItemId) -> Option<usize> {
        if id.is_none() {
            return None;
        }
        let i = id.index as usize;
        if *self.gens.get(i)? != id.generation {
            return None;
        }
        let local = *self.loc.get(i)?;
        if local == 0 {
            return None;
        }
        let slot = *self.owner.get(i)? as usize;
        Some(self.base[slot] + (local - 1) as usize)
    }

    pub(super) fn clear_id(&mut self, id: FlowItemId) {
        let i = id.index as usize;
        if i < self.loc.len() {
            self.loc[i] = 0;
            self.owner[i] = 0;
        }
    }

    #[cfg(test)]
    pub(super) fn iter(&self) -> impl Iterator<Item = &FlowItem> {
        self.order
            .iter()
            .flat_map(move |&slot| self.slots[slot as usize].iter())
    }

    pub(super) fn range(&self, range: Range<usize>) -> impl Iterator<Item = &FlowItem> + '_ {
        let end = range.end;
        let mut cursor = range.start;
        let mut li = if self.order.is_empty() {
            usize::MAX
        } else {
            self.ends.partition_point(|&e| e <= cursor)
        };
        std::iter::from_fn(move || {
            if cursor >= end || li >= self.order.len() {
                return None;
            }
            let slot = self.order[li] as usize;
            let start = if li == 0 { 0 } else { self.ends[li - 1] };
            let chunk = &self.slots[slot];
            let local = cursor - start;
            let take = (end - cursor).min(chunk.len() - local);
            let out = &chunk[local..local + take];
            cursor += take;
            li += 1;
            Some(out.iter())
        })
        .flatten()
    }

    pub(super) fn splice(&mut self, at: usize, delete: usize, insert: Vec<FlowItem>) {
        assert!(at + delete <= self.total, "chunked splice out of bounds");
        if self.order.is_empty() {
            debug_assert_eq!(at, 0);
            debug_assert_eq!(delete, 0);
            let new_slots = self.build_chunks(insert);
            self.order.extend(new_slots.iter().copied());
            self.reindex();
            self.remap_slots(&new_slots);
            return;
        }
        let lo = self.logical_at(at);
        let hi = if delete == 0 {
            lo
        } else {
            self.logical_at(at + delete - 1)
        };
        let lo_slot = self.order[lo] as usize;
        let lo_start = if lo == 0 { 0 } else { self.ends[lo - 1] };
        let prefix_len = at - lo_start;
        let hi_slot = self.order[hi] as usize;
        let hi_start = if hi == 0 { 0 } else { self.ends[hi - 1] };
        let suffix_off = at + delete - hi_start;
        let suffix_len = self.slots[hi_slot].len() - suffix_off;

        let mut merged: Vec<FlowItem> = Vec::with_capacity(prefix_len + insert.len() + suffix_len);
        merged.extend_from_slice(&self.slots[lo_slot][..prefix_len]);
        merged.extend(insert);
        merged.extend_from_slice(&self.slots[hi_slot][suffix_off..]);

        for li in lo..=hi {
            let slot = self.order[li] as usize;
            self.slots[slot].clear();
            self.free.push(slot as u32);
        }
        let new_slots = self.build_chunks(merged);
        self.order.splice(lo..=hi, new_slots.iter().copied());
        self.reindex();
        self.remap_slots(&new_slots);
    }

    fn logical_at(&self, pos: usize) -> usize {
        self.ends
            .partition_point(|&e| e <= pos)
            .min(self.order.len() - 1)
    }

    fn locate(&self, pos: usize) -> (usize, usize) {
        let li = self.ends.partition_point(|&e| e <= pos);
        let slot = self.order[li] as usize;
        let start = if li == 0 { 0 } else { self.ends[li - 1] };
        (slot, pos - start)
    }

    fn alloc_slot(&mut self) -> usize {
        if let Some(slot) = self.free.pop() {
            let slot = slot as usize;
            self.slots[slot].clear();
            slot
        } else {
            self.slots.push(Vec::new());
            self.base.push(0);
            self.slots.len() - 1
        }
    }

    fn build_chunks(&mut self, items: Vec<FlowItem>) -> Vec<u32> {
        let len = items.len();
        if len == 0 {
            return Vec::new();
        }
        let count = len.div_ceil(CHUNK_CAPACITY);
        let mut it = items.into_iter();
        let mut out = Vec::with_capacity(count);
        let mut offset = 0usize;
        for i in 0..count {
            let span = (len - offset) / (count - i);
            let slot = self.alloc_slot();
            self.slots[slot].extend(it.by_ref().take(span));
            offset += span;
            out.push(slot as u32);
        }
        debug_assert_eq!(offset, len);
        out
    }

    fn reindex(&mut self) {
        self.ends.clear();
        self.ends.reserve(self.order.len());
        let mut acc = 0usize;
        for &slot in &self.order {
            let slot = slot as usize;
            let len = self.slots[slot].len();
            self.base[slot] = acc;
            acc += len;
            self.ends.push(acc);
        }
        self.total = acc;
    }

    fn remap_slots(&mut self, slot_ids: &[u32]) {
        for &slot in slot_ids {
            let slot = slot as usize;
            let len = self.slots[slot].len();
            for local in 0..len {
                let id = self.slots[slot][local].id;
                let idx = id.index as usize;
                if self.loc.len() <= idx {
                    self.loc.resize(idx + 1, 0);
                    self.owner.resize(idx + 1, 0);
                    self.gens.resize(idx + 1, 0);
                }
                self.owner[idx] = slot as u32;
                self.loc[idx] = (local as u32) + 1;
                self.gens[idx] = id.generation;
            }
        }
    }
}

impl Index<usize> for Chunked {
    type Output = FlowItem;

    fn index(&self, pos: usize) -> &FlowItem {
        let (slot, local) = self.locate(pos);
        &self.slots[slot][local]
    }
}

impl IndexMut<usize> for Chunked {
    fn index_mut(&mut self, pos: usize) -> &mut FlowItem {
        let (slot, local) = self.locate(pos);
        &mut self.slots[slot][local]
    }
}
