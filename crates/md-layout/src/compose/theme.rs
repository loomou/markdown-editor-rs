use crate::box_tree::TypeSlot;
use crate::style::BoxLayoutStyle;
use md_core::block::BlockKind;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowMetrics {
    pub paragraph_lead_top: md_core::Px,
    pub quote_paragraph_top: md_core::Px,

    pub quote_alert_lead: md_core::Px,
    pub quote_paragraph_slot: TypeSlot,
    pub list_item_lead_zero: bool,
    pub doc_lead_zero: bool,
    pub footnote_item_top: md_core::Px,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutTheme {
    doc_root: BoxLayoutStyle,
    doc_start: BoxLayoutStyle,
    paragraph: BoxLayoutStyle,
    heading: [BoxLayoutStyle; 6],
    code_block: BoxLayoutStyle,
    block_quote: BoxLayoutStyle,
    list: BoxLayoutStyle,
    list_item: BoxLayoutStyle,
    table: BoxLayoutStyle,
    table_row: BoxLayoutStyle,
    table_cell: BoxLayoutStyle,
    thematic_break: BoxLayoutStyle,
    footnote: BoxLayoutStyle,
    image: BoxLayoutStyle,
    mermaid: BoxLayoutStyle,
    math: BoxLayoutStyle,
    pub(crate) list_gutter_min: md_core::Px,
    pub(crate) list_digit_width: md_core::Px,
    pub(crate) list_task_extra: md_core::Px,
    pub(crate) list_marker_gap: md_core::Px,
    pub(crate) list_tight_gap: md_core::Px,
    pub(crate) list_loose_gap: md_core::Px,
    pub(crate) list_nested_top: md_core::Px,
    pub(crate) list_nested_indent: md_core::Px,
    pub(crate) paragraph_lead_top: md_core::Px,
    pub(crate) quote_paragraph_top: md_core::Px,
    pub(crate) quote_alert_lead: md_core::Px,
    pub(crate) quote_paragraph_slot: TypeSlot,
    pub(crate) list_item_lead_zero: bool,
    pub(crate) doc_lead_zero: bool,
    pub(crate) footnote_item_top: md_core::Px,
}

pub struct ListMetrics {
    pub list_gutter_min: md_core::Px,
    pub list_digit_width: md_core::Px,
    pub list_task_extra: md_core::Px,
    pub list_marker_gap: md_core::Px,
    pub list_tight_gap: md_core::Px,
    pub list_loose_gap: md_core::Px,
    pub list_nested_top: md_core::Px,
    pub list_nested_indent: md_core::Px,
}

impl LayoutTheme {
    pub fn from_resolver(mut resolve: impl FnMut(BlockKind) -> BoxLayoutStyle) -> Self {
        let mut theme = LayoutTheme {
            doc_root: resolve(BlockKind::DocRoot),
            doc_start: resolve(BlockKind::DocStart),
            paragraph: resolve(BlockKind::Paragraph),
            heading: [
                resolve(BlockKind::Heading(1)),
                resolve(BlockKind::Heading(2)),
                resolve(BlockKind::Heading(3)),
                resolve(BlockKind::Heading(4)),
                resolve(BlockKind::Heading(5)),
                resolve(BlockKind::Heading(6)),
            ],
            code_block: resolve(BlockKind::CodeBlock),
            block_quote: resolve(BlockKind::BlockQuote),
            list: resolve(BlockKind::List),
            list_item: resolve(BlockKind::ListItem),
            table: resolve(BlockKind::Table),
            table_row: resolve(BlockKind::TableRow),
            table_cell: resolve(BlockKind::TableCell),
            thematic_break: resolve(BlockKind::ThematicBreak),
            footnote: resolve(BlockKind::FootnoteDefinition),
            image: resolve(BlockKind::Image),
            mermaid: resolve(BlockKind::Mermaid),
            math: resolve(BlockKind::Math),
            list_gutter_min: 28.0,
            list_digit_width: 10.0,
            list_task_extra: 16.0,
            list_marker_gap: 8.0,
            list_tight_gap: 0.0,
            list_loose_gap: 20.0,
            list_nested_top: 8.0,
            list_nested_indent: 0.0,
            paragraph_lead_top: 0.0,
            quote_paragraph_top: 0.0,
            quote_alert_lead: 28.0,
            quote_paragraph_slot: TypeSlot::Quote,
            list_item_lead_zero: true,
            doc_lead_zero: true,
            footnote_item_top: 12.0,
        };
        theme.paragraph_lead_top = theme.paragraph.margin.top;
        theme
    }

    pub fn with_list_metrics(mut self, m: ListMetrics) -> Self {
        self.list_gutter_min = m.list_gutter_min;
        self.list_digit_width = m.list_digit_width;
        self.list_task_extra = m.list_task_extra;
        self.list_marker_gap = m.list_marker_gap;
        self.list_tight_gap = m.list_tight_gap;
        self.list_loose_gap = m.list_loose_gap;
        self.list_nested_top = m.list_nested_top;
        self.list_nested_indent = m.list_nested_indent;
        self
    }

    pub fn with_flow_metrics(mut self, m: FlowMetrics) -> Self {
        self.paragraph_lead_top = m.paragraph_lead_top;
        self.quote_paragraph_top = m.quote_paragraph_top;
        self.quote_alert_lead = m.quote_alert_lead;
        self.quote_paragraph_slot = m.quote_paragraph_slot;
        self.list_item_lead_zero = m.list_item_lead_zero;
        self.doc_lead_zero = m.doc_lead_zero;
        self.footnote_item_top = m.footnote_item_top;
        self
    }

    pub(crate) fn style_for(&self, kind: BlockKind) -> BoxLayoutStyle {
        match kind {
            BlockKind::DocRoot => self.doc_root,
            BlockKind::DocStart => self.doc_start,
            BlockKind::Paragraph => self.paragraph,
            BlockKind::Heading(n) => self.heading[n.clamp(1, 6) as usize - 1],
            BlockKind::CodeBlock | BlockKind::MetadataBlock => self.code_block,
            BlockKind::BlockQuote => self.block_quote,
            BlockKind::List => self.list,
            BlockKind::ListItem => self.list_item,
            BlockKind::Table => self.table,
            BlockKind::TableRow => self.table_row,
            BlockKind::TableCell => self.table_cell,
            BlockKind::ThematicBreak => self.thematic_break,
            BlockKind::FootnoteDefinition => self.footnote,
            BlockKind::Image => self.image,
            BlockKind::Mermaid => self.mermaid,
            BlockKind::Math => self.math,
        }
    }
}
