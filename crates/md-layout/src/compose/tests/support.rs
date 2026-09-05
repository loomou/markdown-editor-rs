use crate::box_tree::TypeSlot;
use crate::compose::{FlowMetrics, LayoutTheme};
use crate::style::{BoxDisplay, BoxLayoutStyle, Edges};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::document::Document;

pub(super) fn layout() -> LayoutTheme {
    spacing_theme(|_| 0.0)
}

pub(super) fn spacing_theme(top: impl Fn(BlockKind) -> Px) -> LayoutTheme {
    LayoutTheme::from_resolver(|kind| BoxLayoutStyle {
        display: BoxDisplay::FlowStack,
        margin: Edges {
            top: top(kind),
            ..Edges::ZERO
        },
        padding: Edges::ZERO,
        border: Edges::ZERO,
        gap: 0.0,
    })
}

pub(super) fn flow_metrics(paragraph_lead_top: Px) -> FlowMetrics {
    FlowMetrics {
        paragraph_lead_top,
        quote_paragraph_top: 0.0,
        quote_alert_lead: 0.0,
        quote_paragraph_slot: TypeSlot::Quote,
        list_item_lead_zero: true,
        doc_lead_zero: true,
        footnote_item_top: 12.0,
    }
}

pub(super) fn heading_spacing_theme() -> LayoutTheme {
    spacing_theme(|kind| match kind {
        BlockKind::Paragraph => 20.0,
        BlockKind::Heading(1) => 0.0,
        BlockKind::Heading(2) => 48.0,
        BlockKind::Heading(3) => 32.0,
        BlockKind::Heading(4) => 24.0,
        BlockKind::Heading(5 | 6) => 20.0,
        _ => 0.0,
    })
    .with_flow_metrics(flow_metrics(24.0))
}

pub(super) fn kind_count(doc: &Document, kind: BlockKind) -> usize {
    doc.preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(kind))
        .count()
}
