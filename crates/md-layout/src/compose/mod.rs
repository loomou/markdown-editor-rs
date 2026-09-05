mod block_edit;
mod flow_policy;
mod sync;
mod theme;
mod window;

#[cfg(test)]
mod tests;

pub use block_edit::{PreviewSplice, sync_block_edit};
pub use sync::{defer_composed, patch_text, sync_layout};
pub use theme::{FlowMetrics, LayoutTheme, ListMetrics};
pub use window::{ComposeWindow, LeafMetrics, compose_into, compose_window};

use block_edit::{box_snapshot, emit_preview, wants_preview};
use flow_policy::{apply_list_metrics, flow_top_margin, type_slot_for};

#[cfg(any(test, feature = "dump"))]
use crate::box_tree::BoxOwner;
use crate::box_tree::{
    BoxChildren, BoxIntern, BoxNode, BoxRole, BoxStore, BoxStyleStore, BoxTree, LayoutBoxId,
    TypeSlot,
};
use crate::style::{BoxDisplay, BoxLayoutStyle, Edges};
use md_core::block::BlockKind;
use md_core::document::Document;
use md_core::document::NodeId;
#[cfg(any(test, feature = "dump"))]
use std::fmt::Write;

pub fn compose(doc: &Document, theme: &LayoutTheme) -> BoxTree {
    let mut nodes = BoxStore::default();
    let mut intern = BoxIntern::default();
    let mut styles = BoxStyleStore::default();
    let root = emit(
        doc,
        doc.root,
        None,
        theme,
        &mut nodes,
        &mut styles,
        &mut intern,
    );
    BoxTree {
        nodes,
        intern,
        styles,
        root,
        deferred: std::collections::HashMap::new(),
    }
}

pub(super) fn restyle(
    nodes: &mut BoxStore,
    styles: &mut BoxStyleStore,
    id: LayoutBoxId,
    mutate: impl FnOnce(&mut BoxLayoutStyle),
) {
    let mut style = *styles.get(
        nodes
            .get(&id)
            .expect("restyle requires a live BoxNode")
            .style_id,
    );
    let original = style;
    mutate(&mut style);
    if style == original {
        return;
    }
    let style_id = styles.intern(style);
    nodes
        .get_mut(&id)
        .expect("restyle requires a live BoxNode")
        .style_id = style_id;
}

pub(super) fn emit(
    doc: &Document,
    id: NodeId,
    parent: Option<LayoutBoxId>,
    theme: &LayoutTheme,
    nodes: &mut BoxStore,
    styles: &mut BoxStyleStore,
    intern: &mut BoxIntern,
) -> LayoutBoxId {
    let node = doc.arena.get(id).expect("live node");
    if node.content_revision == 0 || node.structure_revision == 0 {
        panic!("node {} missing revision", id.index);
    }
    let box_id = LayoutBoxId::for_kind(node.kind, id.index);
    let mut child_ids: Vec<LayoutBoxId> = Vec::new();
    for c in doc.arena.children(id) {
        let cid = emit(doc, c, Some(box_id), theme, nodes, styles, intern);
        child_ids.push(cid);
        if let Some(p) = emit_preview(doc, c, cid, Some(box_id), theme, nodes, styles, intern) {
            child_ids.push(p);
        }
    }
    let children = if node.kind == BlockKind::TableRow {
        BoxChildren::Island(child_ids)
    } else if node.kind.is_vertical_container() {
        BoxChildren::Vertical(child_ids)
    } else if child_ids.is_empty() {
        BoxChildren::None
    } else {
        panic!(
            "leaf kind {:?} (node {}) has {} children; compose undefined",
            node.kind,
            id.index,
            child_ids.len()
        );
    };
    let mut style = theme.style_for(node.kind);
    style.margin.top = flow_top_margin(theme, doc, id, node.kind);
    let type_slot = type_slot_for(theme, doc, id, node.kind, node.extra);
    let edit_source = wants_preview(doc, id);
    nodes.insert(
        box_id,
        BoxNode {
            id: box_id,
            kind: node.kind,
            style_id: styles.intern(style),
            parent,
            children,
            text_id: intern.push(box_snapshot(doc, id, edit_source)),
            extra: node.extra,
            content_revision: node.content_revision,
            content_generation: id.generation.get(),
            edit_source,
            type_slot,
        },
    );
    emit_chrome(id.index, box_id, theme, nodes, styles);
    if node.kind == BlockKind::List {
        apply_list_metrics(doc, id, box_id, theme, nodes, styles);
    }
    if node.kind == BlockKind::BlockQuote && node.extra.quote_alert().is_some() {
        restyle(nodes, styles, box_id, |style| {
            style.padding.top = theme.quote_alert_lead
        });
    }
    box_id
}

fn emit_chrome(
    index: u32,
    parent: LayoutBoxId,
    theme: &LayoutTheme,
    nodes: &mut BoxStore,
    styles: &mut BoxStyleStore,
) {
    let host = nodes.get(&parent).expect("host box");
    let Some(chrome_id) = LayoutBoxId::chrome(host.kind, index) else {
        return;
    };
    let kind = host.kind;
    let extra = host.extra;
    let content_revision = host.content_revision;
    let content_generation = host.content_generation;
    let style = match chrome_id.role {
        BoxRole::Bar => {
            let host_style = theme.style_for(kind);
            BoxLayoutStyle {
                display: BoxDisplay::FlowStack,
                margin: Edges::ZERO,
                padding: Edges::ZERO,
                border: Edges {
                    left: host_style.border.left,
                    ..Edges::ZERO
                },
                gap: 0.0,
            }
        }
        BoxRole::Slot => BoxLayoutStyle {
            display: BoxDisplay::FlowStack,
            margin: Edges::ZERO,
            padding: Edges::ZERO,
            border: Edges::ZERO,
            gap: 0.0,
        },
        _ => return,
    };
    nodes.insert(
        chrome_id,
        BoxNode {
            id: chrome_id,
            kind,
            style_id: styles.intern(style),
            parent: Some(parent),
            children: BoxChildren::None,
            text_id: None,
            extra,
            content_revision,
            content_generation,
            edit_source: false,
            type_slot: TypeSlot::FromKind,
        },
    );
}

#[cfg(any(test, feature = "dump"))]
pub fn dump_compose(tree: &BoxTree) -> String {
    let mut out = String::new();
    for (id, node) in tree.nodes() {
        let owner = match id.owner {
            BoxOwner::Block(b) => b.to_string(),
            BoxOwner::DocStart => "start".to_string(),
        };
        let parent = match node.parent() {
            Some(p) => match p.owner {
                BoxOwner::Block(b) => b.to_string(),
                BoxOwner::DocStart => "start".to_string(),
            },
            None => "-".to_string(),
        };
        let _ = writeln!(
            out,
            "{owner}\t{:?}\t{:?}\t{parent}\t{}",
            id.role,
            node.kind(),
            escape_compose(tree.intern().text(node.text_id()))
        );
    }
    out
}

#[cfg(any(test, feature = "dump"))]
fn escape_compose(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}
