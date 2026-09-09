use super::ids::LayoutBoxId;
use super::intern::BoxIntern;
use super::metrics::LeafMetrics;
use super::node::{BoxChildren, BoxNode};
use super::store::BoxStore;
use super::styles::BoxStyleStore;
use crate::style::BoxLayoutStyle;
use md_core::Px;
use md_core::block::BlockKind;
use md_core::document::LeafSnapshot;
use md_core::inline::InlineRun;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct DeferredBox {
    pub parent: Option<LayoutBoxId>,
    pub height: Px,
    pub margin_top: Px,
    pub margin_bottom: Px,
}

#[derive(Clone, Debug)]
pub struct LazyEstimator {
    pub(crate) metrics: LeafMetrics,
    pub(crate) viewport: std::cell::Cell<Px>,
}

#[derive(Clone, Debug)]
pub struct BoxTree {
    pub(crate) nodes: BoxStore,
    pub(crate) intern: BoxIntern,
    pub(crate) styles: BoxStyleStore,
    pub(crate) root: LayoutBoxId,
    pub(crate) deferred: HashMap<LayoutBoxId, DeferredBox>,
    pub(crate) lazy: Option<LazyEstimator>,
}

impl BoxTree {
    pub fn nodes(&self) -> &BoxStore {
        &self.nodes
    }

    pub fn intern(&self) -> &BoxIntern {
        &self.intern
    }

    pub fn root(&self) -> LayoutBoxId {
        self.root
    }

    pub fn deferred_len(&self) -> usize {
        self.deferred.len()
    }

    pub(crate) fn deferred(&self, id: LayoutBoxId) -> Option<&DeferredBox> {
        self.deferred.get(&id)
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }

    pub fn set_lazy_viewport(&self, w: Px) {
        if let Some(lazy) = &self.lazy {
            lazy.viewport.set(w);
        }
    }

    pub(crate) fn flow_margins(&self, id: LayoutBoxId) -> (Px, Px) {
        if let Some(node) = self.nodes.get(&id) {
            let s = self.style_of(node);
            return (s.margin.top, s.margin.bottom);
        }
        if let Some(d) = self.deferred.get(&id) {
            return (d.margin_top, d.margin_bottom);
        }
        panic!("unknown LayoutBoxId {id:?}");
    }

    pub fn deferred_height(&self, id: LayoutBoxId) -> Option<Px> {
        self.deferred.get(&id).map(|d| d.height)
    }

    pub fn get(&self, id: LayoutBoxId) -> &BoxNode {
        self.nodes
            .get(&id)
            .unwrap_or_else(|| panic!("unknown LayoutBoxId {id:?}"))
    }

    pub fn style(&self, id: LayoutBoxId) -> &BoxLayoutStyle {
        let sid = self.get(id).style_id;
        self.styles.get(sid)
    }

    pub fn style_of(&self, node: &BoxNode) -> &BoxLayoutStyle {
        self.styles.get(node.style_id)
    }

    #[cfg(test)]
    pub(crate) fn style_count(&self) -> usize {
        self.styles.len()
    }

    pub fn text(&self, id: LayoutBoxId) -> &str {
        self.text_of(self.get(id))
    }

    pub fn text_of(&self, node: &BoxNode) -> &str {
        self.intern.text(node.text_id)
    }

    pub fn runs_of(&self, node: &BoxNode) -> &[InlineRun] {
        self.intern.runs(node.text_id)
    }

    pub(crate) fn set_leaf_text(&mut self, id: LayoutBoxId, snapshot: Option<Arc<LeafSnapshot>>) {
        let existing = self.nodes.get(&id).and_then(|n| n.text_id);
        let text_id = match (existing, snapshot) {
            (Some(tid), Some(snapshot))
                if !snapshot.display.is_empty() || !snapshot.runs.is_empty() =>
            {
                self.intern.replace(tid, Some(snapshot));
                Some(tid)
            }
            (Some(tid), _) => {
                self.intern.release(Some(tid));
                None
            }
            (None, snapshot) => self.intern.push(snapshot),
        };
        if let Some(n) = self.nodes.get_mut(&id) {
            n.text_id = text_id;
        }
    }

    pub fn ancestor_chain(&self, id: LayoutBoxId) -> Vec<LayoutBoxId> {
        let mut chain = vec![id];
        let mut cur = id;
        while let Some(p) = self.get(cur).parent {
            chain.push(p);
            cur = p;
        }
        chain.reverse();
        chain
    }

    pub fn avail_width(&self, id: LayoutBoxId, viewport_width: md_core::Px) -> md_core::Px {
        let mut w = viewport_width;
        let chain = self.ancestor_chain(id);

        for anc in &chain[..chain.len().saturating_sub(1)] {
            w -= self.style(*anc).inline_border_padding();
        }
        w.max(0.0)
    }

    pub fn content_width(&self, id: LayoutBoxId, viewport_width: md_core::Px) -> md_core::Px {
        (self.avail_width(id, viewport_width) - self.style(id).inline_border_padding()).max(0.0)
    }

    pub fn island_boxes(&self) -> Vec<LayoutBoxId> {
        let mut out = Vec::new();
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            if id.is_materializable_role()
                && matches!(node.children, BoxChildren::Island(_) | BoxChildren::None)
            {
                out.push(id);
            }
            if let BoxChildren::Vertical(children) = &node.children {
                pending.extend(children.iter().rev().copied());
            }
        }
        out
    }

    pub fn ancestor_table(&self, id: LayoutBoxId) -> Option<LayoutBoxId> {
        let mut cur = id;
        loop {
            if self.get(cur).kind == BlockKind::Table {
                return Some(cur);
            }
            match self.get(cur).parent {
                Some(p) => cur = p,
                None => return None,
            }
        }
    }
}
