use super::theme::LayoutTheme;
use crate::box_tree::{BoxStore, BoxStyleStore, LayoutBoxId, TypeSlot};
use md_core::block::{BlockKind, NodeExtra};
use md_core::document::Document;
use md_core::document::NodeId;

pub(super) fn apply_list_metrics(
    doc: &Document,
    id: NodeId,
    box_id: LayoutBoxId,
    theme: &LayoutTheme,
    nodes: &mut BoxStore,
    styles: &mut BoxStyleStore,
) {
    let extra = doc.extra(id);
    let kids: Vec<NodeId> = doc.arena.children(id).collect();
    let n = kids.len() as u64;
    let has_task = kids.iter().any(|c| doc.extra(*c).task_checked().is_some());
    let mut pad = theme.list_gutter_min;
    if let Some(start) = extra.ordered_start() {
        let last = start.saturating_add(n.saturating_sub(1));
        let digits = (last.max(1).ilog10() + 1) as md_core::Px;
        pad = pad.max(digits * theme.list_digit_width + theme.list_marker_gap);
    }
    if has_task {
        pad += theme.list_task_extra;
    }
    let gap = if extra.list_loose() {
        theme.list_loose_gap
    } else {
        theme.list_tight_gap
    };
    let nested = doc.arena.get(id).and_then(|n| n.parent).is_some_and(|p| {
        doc.arena
            .get(p)
            .is_some_and(|pn| pn.kind == BlockKind::ListItem)
    });
    let Some(current_style_id) = nodes.get(&box_id).map(|node| node.style_id) else {
        return;
    };
    let original = *styles.get(current_style_id);
    let mut style = original;
    style.padding.left = pad;
    style.gap = gap;
    if nested {
        style.margin.top = theme.list_nested_top;
        if theme.list_nested_indent > 0.0 {
            style.padding.left = theme.list_nested_indent.max(pad);
        }
    }
    let style_id = if style == original {
        current_style_id
    } else {
        styles.intern(style)
    };
    if let Some(node) = nodes.get_mut(&box_id) {
        node.style_id = style_id;
        node.extra = extra;
    }
}

fn is_list_item_lead(doc: &Document, id: NodeId) -> bool {
    is_kind_lead(doc, id, BlockKind::ListItem)
}

fn is_kind_lead(doc: &Document, id: NodeId, parent_kind: BlockKind) -> bool {
    doc.arena.get(id).is_some_and(|n| {
        n.parent.is_some_and(|p| {
            doc.arena
                .get(p)
                .is_some_and(|pn| pn.kind == parent_kind && pn.first_child == Some(id))
        })
    })
}

fn has_checked_task_ancestor(doc: &Document, id: NodeId) -> bool {
    let mut cur = doc.arena.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        let Some(n) = doc.arena.get(p) else {
            break;
        };
        if n.kind == BlockKind::ListItem && doc.extra(p).task_checked() == Some(true) {
            return true;
        }
        cur = n.parent;
    }
    false
}

fn has_ancestor_kind(doc: &Document, id: NodeId, kind: BlockKind) -> bool {
    let mut cur = doc.arena.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        let Some(n) = doc.arena.get(p) else {
            break;
        };
        if n.kind == kind {
            return true;
        }
        cur = n.parent;
    }
    false
}

fn prev_is_heading(doc: &Document, id: NodeId, level: u8) -> bool {
    doc.arena.get(id).is_some_and(|n| {
        n.prev_sibling.is_some_and(|p| {
            doc.arena
                .get(p)
                .is_some_and(|pn| pn.kind == BlockKind::Heading(level))
        })
    })
}

fn is_prose(kind: BlockKind) -> bool {
    matches!(kind, BlockKind::Paragraph | BlockKind::Heading(_))
}

fn is_doc_lead_flush(kind: BlockKind) -> bool {
    is_prose(kind) || kind == BlockKind::Table
}

pub(super) fn flow_top_margin(
    theme: &LayoutTheme,
    doc: &Document,
    id: NodeId,
    kind: BlockKind,
) -> md_core::Px {
    let base = if matches!(kind, BlockKind::Heading(_) | BlockKind::Table) {
        theme.style_for(BlockKind::Paragraph).margin.top
    } else {
        theme.style_for(kind).margin.top
    };
    if theme.doc_lead_zero && is_doc_lead_flush(kind) && is_kind_lead(doc, id, BlockKind::DocRoot) {
        return 0.0;
    }
    if is_list_item_lead(doc, id) {
        if kind == BlockKind::List {
            return base;
        }
        if theme.list_item_lead_zero {
            return 0.0;
        }
    }
    if is_prose(kind) && has_ancestor_kind(doc, id, BlockKind::BlockQuote) {
        return theme.quote_paragraph_top;
    }
    if is_prose(kind) && prev_is_heading(doc, id, 1) {
        return theme.paragraph_lead_top;
    }
    if is_prose(kind) && has_ancestor_kind(doc, id, BlockKind::FootnoteDefinition) {
        return theme.footnote_item_top;
    }
    base
}

pub(super) fn type_slot_for(
    theme: &LayoutTheme,
    doc: &Document,
    id: NodeId,
    kind: BlockKind,
    extra: NodeExtra,
) -> TypeSlot {
    if extra.table_header() {
        return TypeSlot::TableHeader;
    }
    if has_checked_task_ancestor(doc, id) {
        return TypeSlot::TaskDone;
    }
    if kind == BlockKind::FootnoteDefinition
        || (kind == BlockKind::Paragraph
            && has_ancestor_kind(doc, id, BlockKind::FootnoteDefinition))
    {
        return TypeSlot::Footnote;
    }
    if kind == BlockKind::Paragraph && has_ancestor_kind(doc, id, BlockKind::BlockQuote) {
        return theme.quote_paragraph_slot;
    }
    TypeSlot::FromKind
}
