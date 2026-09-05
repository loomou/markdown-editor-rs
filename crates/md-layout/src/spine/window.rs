use super::FlowSpine;
use super::item::{FlowItemId, FlowItemKind};
use crate::box_tree::{BoxTree, LayoutBoxId};
use md_core::Px;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct FlowWindowEntry {
    pub top: Px,
    pub height: Px,
    pub kind: FlowItemKind,
}

#[derive(Clone, Copy, Debug)]
pub struct ContainerSpan {
    pub box_id: LayoutBoxId,
    pub top: Px,
    pub bottom: Px,
}

#[derive(Clone, Debug, Default)]
pub struct FlowWindow {
    pub total_height: Px,
    pub items_visited: u64,
    pub entries: Vec<FlowWindowEntry>,
    pub extra_spans: BTreeMap<LayoutBoxId, Px>,
    pub container_spans: Vec<ContainerSpan>,
}

impl FlowSpine {
    pub fn resolve(&self, item: FlowItemId, within: Px) -> Px {
        if item.is_none() {
            return within.max(0.0);
        }
        match self.item_top(item) {
            Some(t) => (t + within).max(0.0),
            None => within.max(0.0),
        }
    }

    fn box_span(&self, box_id: LayoutBoxId) -> Option<(Px, Px)> {
        let open = self.open_of.get(&box_id).copied()?;
        let close = self.close_of.get(&box_id).copied()?;
        let top = self.item_top(open)?;
        let cpos = self.location(close)?;
        let bottom = self.fenwick.prefix(cpos + 1);
        Some((top, bottom))
    }

    pub fn window(&self, tree: &BoxTree, top: Px, bottom: Px, extra: &[LayoutBoxId]) -> FlowWindow {
        let range = self.visible(top, bottom);
        let mut entries = Vec::with_capacity(range.len());
        let mut container_spans: Vec<ContainerSpan> = Vec::new();
        let mut seen: BTreeMap<LayoutBoxId, ()> = BTreeMap::new();
        let mut anchors = Vec::new();
        for pos in range.clone() {
            let item = self.items[pos];
            let t = self.fenwick.prefix(pos);
            entries.push(FlowWindowEntry {
                top: t,
                height: item.height.px(),
                kind: item.kind,
            });
            if let Some(box_id) = match item.kind {
                FlowItemKind::ContainerOpen { box_id }
                | FlowItemKind::ContainerClose { box_id }
                | FlowItemKind::Collapsed { box_id }
                | FlowItemKind::Content { box_id } => Some(box_id),
                FlowItemKind::Gap => None,
            } {
                anchors.push(box_id);
            }
            if let FlowItemKind::ContainerOpen { box_id, .. } = item.kind
                && seen.insert(box_id, ()).is_none()
                && let Some((ct, cb)) = self.box_span(box_id)
            {
                container_spans.push(ContainerSpan {
                    box_id,
                    top: ct,
                    bottom: cb,
                });
            }
        }
        for anchor in anchors {
            if tree.nodes().get(&anchor).is_none() {
                continue;
            }
            for anc in tree.ancestor_chain(anchor) {
                if seen.insert(anc, ()).is_some() {
                    continue;
                }
                if let Some((ct, cb)) = self.box_span(anc)
                    && ct < bottom
                    && cb > top
                {
                    container_spans.push(ContainerSpan {
                        box_id: anc,
                        top: ct,
                        bottom: cb,
                    });
                }
            }
        }
        let mut extra_spans = BTreeMap::new();
        for box_id in extra {
            let Some(id) = self.content_of.get(box_id).copied() else {
                continue;
            };
            let Some(pos) = self.location(id) else {
                continue;
            };
            if range.contains(&pos) {
                continue;
            }
            extra_spans.insert(*box_id, self.fenwick.prefix(pos));
        }
        FlowWindow {
            total_height: self.total_height(),
            items_visited: range.len() as u64 + extra_spans.len() as u64,
            entries,
            extra_spans,
            container_spans,
        }
    }
}
