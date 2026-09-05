use super::FlowSpine;
use super::fenwick::Fenwick;
use super::item::{FlowItem, FlowItemKind};
use crate::box_tree::{BoxTree, LayoutBoxId};
use crate::flow::HeightState;
use md_core::Px;
use std::collections::BTreeMap;

impl FlowSpine {
    pub fn flatten(
        tree: &BoxTree,
        viewport_width: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
    ) -> Self {
        Self::flatten_with(tree, viewport_width, heights, true)
    }

    pub(crate) fn flatten_complete(
        tree: &BoxTree,
        viewport_width: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
    ) -> Self {
        Self::flatten_with(tree, viewport_width, heights, false)
    }

    fn flatten_with(
        tree: &BoxTree,
        viewport_width: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
        collapse_nested: bool,
    ) -> Self {
        crate::hot_path::add_flow_lower();
        let mut tmp = Self::blank();
        let cap = if collapse_nested {
            tree.nodes.len().saturating_add(tree.deferred_len())
        } else {
            tree.nodes
                .len()
                .saturating_add(tree.deferred_len())
                .saturating_mul(3)
        };
        let mut items = Vec::with_capacity(cap);
        tmp.push_lowered(
            tree,
            tree.root,
            viewport_width,
            heights,
            collapse_nested,
            &mut items,
        );
        let mut spine = Self::from_items(items);
        spine.viewport_width = viewport_width;
        spine
    }

    fn blank() -> Self {
        FlowSpine {
            items: Vec::new(),
            fenwick: Fenwick::empty(),
            loc: vec![0],
            gens: vec![0],
            content_of: BTreeMap::new(),
            open_of: BTreeMap::new(),
            close_of: BTreeMap::new(),
            collapsed_of: BTreeMap::new(),
            estimated_count: 0,
            content_count: 0,
            exact_content_in_epoch: 0,
            noncontent_estimated: 0,
            layout_epoch: 1,
            next_index: 1,
            free_ids: Vec::new(),
            viewport_width: 0.0,
        }
    }

    pub(crate) fn from_items(items: Vec<FlowItem>) -> Self {
        let mut loc = vec![0u32; 1];
        let mut gens = vec![0u32; 1];
        let mut content_of = BTreeMap::new();
        let mut open_of = BTreeMap::new();
        let mut close_of = BTreeMap::new();
        let mut collapsed_of = BTreeMap::new();
        let mut estimated_count = 0u32;
        let mut content_count = 0u32;
        let mut exact_content_in_epoch = 0u32;
        let mut noncontent_estimated = 0u32;
        let mut heights = Vec::with_capacity(items.len());
        let mut next_index = 1u32;
        let layout_epoch = 1u64;
        let mut items = items;
        for (pos, item) in items.iter_mut().enumerate() {
            item.height_epoch = layout_epoch;
            Self::grow_id_tables(&mut loc, &mut gens, item.id.index);
            loc[item.id.index as usize] = (pos as u32) + 1;
            gens[item.id.index as usize] = item.id.generation;
            match item.kind {
                FlowItemKind::Content { box_id } => {
                    content_of.insert(box_id, item.id);
                    content_count += 1;
                    if item.height.is_exact() {
                        exact_content_in_epoch += 1;
                    }
                }
                FlowItemKind::ContainerOpen { box_id, .. } => {
                    open_of.insert(box_id, item.id);
                    if !item.height.is_exact() {
                        noncontent_estimated += 1;
                    }
                }
                FlowItemKind::ContainerClose { box_id, .. } => {
                    close_of.insert(box_id, item.id);
                    if !item.height.is_exact() {
                        noncontent_estimated += 1;
                    }
                }
                FlowItemKind::Collapsed { box_id } => {
                    collapsed_of.insert(box_id, item.id);
                    if !item.height.is_exact() {
                        noncontent_estimated += 1;
                    }
                }
                FlowItemKind::Gap => {
                    if !item.height.is_exact() {
                        noncontent_estimated += 1;
                    }
                }
            }
            if !item.height.is_exact() {
                estimated_count += 1;
            }
            heights.push(item.height.px());
            next_index = next_index.max(item.id.index.saturating_add(1));
        }
        debug_assert_eq!(
            estimated_count,
            noncontent_estimated + content_count.saturating_sub(exact_content_in_epoch)
        );
        FlowSpine {
            fenwick: Fenwick::from_heights(&heights),
            items,
            loc,
            gens,
            content_of,
            open_of,
            close_of,
            collapsed_of,
            estimated_count,
            content_count,
            exact_content_in_epoch,
            noncontent_estimated,
            layout_epoch,
            next_index,
            free_ids: Vec::new(),
            viewport_width: 0.0,
        }
    }

    pub(super) fn grow_id_tables(loc: &mut Vec<u32>, gens: &mut Vec<u32>, index: u32) {
        let need = index as usize + 1;
        if loc.len() < need {
            loc.resize(need, 0);
            gens.resize(need, 0);
        }
    }
}
