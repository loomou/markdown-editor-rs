use crate::box_tree::LayoutBoxId;
use md_core::Px;
use std::collections::BTreeMap;

mod collapse;
mod expand;
mod fenwick;
mod flatten;
mod gap;
mod height;
mod item;
mod query;
mod splice;
mod tree_walk;
mod window;

#[cfg(test)]
mod tests;

pub use collapse::COLLAPSE_QUOTA;
use fenwick::Fenwick;
pub use item::{FlowItem, FlowItemId, FlowItemKind};
pub use window::{ContainerSpan, FlowWindow, FlowWindowEntry};

pub struct FlowSpine {
    items: Vec<FlowItem>,
    fenwick: Fenwick,
    loc: Vec<u32>,
    gens: Vec<u32>,
    content_of: BTreeMap<LayoutBoxId, FlowItemId>,
    open_of: BTreeMap<LayoutBoxId, FlowItemId>,
    close_of: BTreeMap<LayoutBoxId, FlowItemId>,
    collapsed_of: BTreeMap<LayoutBoxId, FlowItemId>,
    estimated_count: u32,
    content_count: u32,
    exact_content_in_epoch: u32,
    noncontent_estimated: u32,
    layout_epoch: u64,
    next_index: u32,
    free_ids: Vec<FlowItemId>,
    viewport_width: Px,
}
