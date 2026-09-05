use super::block_edit::emit_preview;
use super::emit;
use super::flow_policy::flow_top_margin;
use super::theme::LayoutTheme;
use crate::box_tree::{
    BoxChildren, BoxIntern, BoxNode, BoxStore, BoxStyleStore, BoxTree, DeferredBox, LayoutBoxId,
    TypeSlot,
};
use crate::style::BoxLayoutStyle;
use md_core::Px;
use md_core::block::BlockKind;
use md_core::document::Document;
use md_core::document::NodeId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct ComposeWindow {
    pub top: Px,
    pub bottom: Px,
    pub avail_width: Px,
}

#[derive(Clone, Copy, Debug)]
pub struct LeafMetrics {
    pub line_height: Px,
    pub em_width: Px,
    pub heading1_mult: Px,
    pub heading_mult: Px,
    pub table_row_mult: Px,
    pub mermaid_max_height: Px,
    pub image_placeholder_height: Px,
    pub code_max_height: Px,
    pub math_max_height: Px,
    pub image_max_height: Px,
}

impl LeafMetrics {
    fn leaf_height(
        self,
        theme: &LayoutTheme,
        doc: &Document,
        id: NodeId,
        kind: BlockKind,
        avail: Px,
    ) -> Px {
        let style = theme.style_for(kind);
        if kind == BlockKind::Mermaid {
            return self.mermaid_max_height
                + style.top_border_padding()
                + style.bottom_border_padding();
        }
        if kind == BlockKind::Image {
            let inner = (avail - style.inline_border_padding()).max(1.0);
            let text_width = estimated_text_width(self.em_width, doc.display(id));
            let cap_rows = if text_width == 0.0 {
                0.0
            } else {
                (text_width / inner).ceil().max(1.0)
            };
            return self.image_placeholder_height.min(self.image_max_height)
                + cap_rows * self.line_height
                + style.top_border_padding()
                + style.bottom_border_padding();
        }
        if kind == BlockKind::Math {
            return (self.line_height * 2.0).min(self.math_max_height)
                + style.top_border_padding()
                + style.bottom_border_padding();
        }
        let inner = (avail - style.inline_border_padding()).max(1.0);
        let text = doc.display(id);
        let text_width = estimated_text_width(self.em_width, text);
        let est_rows = if text_width == 0.0 {
            1.0
        } else {
            (text_width / inner).ceil().max(1.0)
        };
        let kind_mult = match kind {
            BlockKind::Heading(1) => self.heading1_mult,
            BlockKind::Heading(_) => self.heading_mult,
            BlockKind::TableRow => self.table_row_mult,
            _ => 1.0,
        };
        let mut content = est_rows * self.line_height * kind_mult;
        if kind == BlockKind::CodeBlock || kind == BlockKind::MetadataBlock {
            content = content.min(self.code_max_height);
        }
        content + style.top_border_padding() + style.bottom_border_padding()
    }
}

fn estimated_text_width(em_width: Px, text: &str) -> Px {
    let ems: Px = text
        .chars()
        .map(|ch| if ch.is_ascii() { 0.5 } else { 1.0 })
        .sum();
    ems * em_width
}

pub fn compose_window(
    doc: &Document,
    theme: &LayoutTheme,
    window: ComposeWindow,
    metrics: &LeafMetrics,
) -> BoxTree {
    let mut nodes = BoxStore::default();
    let mut intern = BoxIntern::default();
    let mut styles = BoxStyleStore::default();
    let mut deferred = HashMap::new();
    let mut heights = HashMap::new();
    let root = emit_root_windowed(
        doc,
        theme,
        window,
        metrics,
        &mut nodes,
        &mut styles,
        &mut intern,
        &mut deferred,
        &mut heights,
    );
    BoxTree {
        nodes,
        intern,
        styles,
        root,
        deferred,
    }
}

pub fn compose_into(tree: &mut BoxTree, doc: &Document, theme: &LayoutTheme, id: LayoutBoxId) {
    if tree.nodes.contains_key(&id) {
        tree.deferred.remove(&id);
        return;
    }
    let Some(deferred) = tree.deferred.remove(&id) else {
        return;
    };
    let Some(index) = id.block() else {
        return;
    };
    let Some(node_id) = doc.live_id(index) else {
        return;
    };
    let cid = emit(
        doc,
        node_id,
        deferred.parent,
        theme,
        &mut tree.nodes,
        &mut tree.styles,
        &mut tree.intern,
    );
    debug_assert_eq!(cid, id);
    if let Some(parent) = deferred.parent
        && let Some(p) = emit_preview(
            doc,
            node_id,
            cid,
            Some(parent),
            theme,
            &mut tree.nodes,
            &mut tree.styles,
            &mut tree.intern,
        )
    {
        insert_preview_after(&mut tree.nodes, parent, cid, p);
    }
}

fn insert_preview_after(
    nodes: &mut BoxStore,
    parent: LayoutBoxId,
    frame: LayoutBoxId,
    preview: LayoutBoxId,
) {
    let Some(node) = nodes.get_mut(&parent) else {
        return;
    };
    match &mut node.children {
        BoxChildren::Vertical(kids) | BoxChildren::Island(kids) => {
            if kids.contains(&preview) {
                return;
            }
            if let Some(i) = kids.iter().position(|c| *c == frame) {
                kids.insert(i + 1, preview);
            } else {
                kids.push(preview);
            }
        }
        BoxChildren::None => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_root_windowed(
    doc: &Document,
    theme: &LayoutTheme,
    window: ComposeWindow,
    metrics: &LeafMetrics,
    nodes: &mut BoxStore,
    styles: &mut BoxStyleStore,
    intern: &mut BoxIntern,
    deferred: &mut HashMap<LayoutBoxId, DeferredBox>,
    heights: &mut HashMap<NodeId, Px>,
) -> LayoutBoxId {
    let node = doc.arena.get(doc.root).expect("live root");
    if node.content_revision == 0 || node.structure_revision == 0 {
        panic!("node {} missing revision", doc.root.index);
    }
    let box_id = LayoutBoxId::for_kind(node.kind, doc.root.index);
    let style = container_style(theme, doc, doc.root, node.kind);
    let pad_top = style.top_border_padding();
    let pad_bottom = style.bottom_border_padding();
    let child_avail = (window.avail_width - style.inline_border_padding()).max(0.0);
    let parent_gap = container_gap(theme, doc, doc.root, node.kind);
    let kids: Vec<NodeId> = doc.arena.children(doc.root).collect();
    let mut child_ids: Vec<LayoutBoxId> = Vec::with_capacity(kids.len());
    let mut y = pad_top;
    let mut prev_mb: Option<Px> = None;
    for c in kids {
        let cn = doc.arena.get(c).expect("live child");
        let cid = LayoutBoxId::for_kind(cn.kind, c.index);
        let mt = node_margin_top(theme, doc, c, cn.kind);
        let mb = theme.style_for(cn.kind).margin.bottom;
        let h = subtree_height(doc, theme, metrics, c, child_avail, heights);
        let gap = match prev_mb {
            None => mt,
            Some(prev) => prev + parent_gap + mt,
        };
        let start = y + gap;
        let end = start + h;
        if start < window.bottom && end > window.top {
            let emitted = emit(doc, c, Some(box_id), theme, nodes, styles, intern);
            child_ids.push(emitted);
            if let Some(p) =
                emit_preview(doc, c, emitted, Some(box_id), theme, nodes, styles, intern)
            {
                child_ids.push(p);
            }
        } else {
            deferred.insert(
                cid,
                DeferredBox {
                    parent: Some(box_id),
                    height: h,
                    margin_top: mt,
                    margin_bottom: mb,
                },
            );
            child_ids.push(cid);
        }
        y = end;
        prev_mb = Some(mb);
    }
    let _ = (y, pad_bottom);
    let children = if node.kind.is_vertical_container() {
        BoxChildren::Vertical(child_ids)
    } else if child_ids.is_empty() {
        BoxChildren::None
    } else {
        panic!(
            "root kind {:?} cannot take {} windowed children",
            node.kind,
            child_ids.len()
        );
    };
    let mut root_style = theme.style_for(node.kind);
    root_style.margin.top = flow_top_margin(theme, doc, doc.root, node.kind);
    nodes.insert(
        box_id,
        BoxNode {
            id: box_id,
            kind: node.kind,
            style_id: styles.intern(root_style),
            parent: None,
            children,
            text_id: intern.push(super::block_edit::box_snapshot(doc, doc.root, false)),
            extra: node.extra,
            content_revision: node.content_revision,
            content_generation: doc.root.generation.get(),
            edit_source: false,
            type_slot: TypeSlot::FromKind,
        },
    );
    box_id
}

fn subtree_height(
    doc: &Document,
    theme: &LayoutTheme,
    metrics: &LeafMetrics,
    id: NodeId,
    avail: Px,
    cache: &mut HashMap<NodeId, Px>,
) -> Px {
    if let Some(h) = cache.get(&id) {
        return *h;
    }
    let node = doc.arena.get(id).expect("live node");
    let height = if node.kind.is_vertical_container() {
        let style = container_style(theme, doc, id, node.kind);
        let child_avail = (avail - style.inline_border_padding()).max(0.0);
        let gap = container_gap(theme, doc, id, node.kind);
        let kids: Vec<NodeId> = doc.arena.children(id).collect();
        let mut h = style.top_border_padding() + style.bottom_border_padding();
        if kids.is_empty() {
            h
        } else {
            let mut prev_mb: Option<Px> = None;
            for c in &kids {
                let ck = doc
                    .arena
                    .get(*c)
                    .map(|n| n.kind)
                    .unwrap_or(BlockKind::Paragraph);
                let mt = node_margin_top(theme, doc, *c, ck);
                let mb = theme.style_for(ck).margin.bottom;
                h += match prev_mb {
                    None => mt,
                    Some(prev) => prev + gap + mt,
                };
                h += subtree_height(doc, theme, metrics, *c, child_avail, cache);
                prev_mb = Some(mb);
            }
            h += prev_mb.unwrap_or(0.0);
            h
        }
    } else {
        metrics.leaf_height(theme, doc, id, node.kind, avail)
    };
    cache.insert(id, height);
    height
}

fn container_style(
    theme: &LayoutTheme,
    doc: &Document,
    id: NodeId,
    kind: BlockKind,
) -> BoxLayoutStyle {
    let mut style = theme.style_for(kind);
    if kind == BlockKind::List {
        style.gap = container_gap(theme, doc, id, kind);
        let nested = doc.arena.get(id).and_then(|n| n.parent).is_some_and(|p| {
            doc.arena
                .get(p)
                .is_some_and(|pn| pn.kind == BlockKind::ListItem)
        });
        if nested {
            style.margin.top = theme.list_nested_top;
        }
    }
    if kind == BlockKind::BlockQuote && doc.extra(id).quote_alert().is_some() {
        style.padding.top = theme.quote_alert_lead;
    }
    style
}

fn container_gap(theme: &LayoutTheme, doc: &Document, id: NodeId, kind: BlockKind) -> Px {
    if kind == BlockKind::List {
        if doc.extra(id).list_loose() {
            theme.list_loose_gap
        } else {
            theme.list_tight_gap
        }
    } else {
        theme.style_for(kind).gap
    }
}

fn node_margin_top(theme: &LayoutTheme, doc: &Document, id: NodeId, kind: BlockKind) -> Px {
    if kind == BlockKind::List {
        let nested = doc.arena.get(id).and_then(|n| n.parent).is_some_and(|p| {
            doc.arena
                .get(p)
                .is_some_and(|pn| pn.kind == BlockKind::ListItem)
        });
        if nested {
            return theme.list_nested_top;
        }
    }
    flow_top_margin(theme, doc, id, kind)
}
