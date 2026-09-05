mod block_quote;
mod code_block;
mod document;
mod heading;
mod image;
mod list;
mod paragraph;
pub mod table;
mod thematic_break;

use crate::snapshot::DecorationPiece;
use gpui::Hsla;
use md_core::Px;
use md_core::block::BlockKind;
use md_theme::DocumentTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationScope {
    None,
    Leaf,
}

#[derive(Clone, Copy, Debug)]
pub enum PaintOp {
    Fill {
        rect: (Px, Px, Px, Px),
        color: Hsla,
    },
    Round {
        rect: (Px, Px, Px, Px),
        color: Hsla,
        radius: f32,
    },
    RoundBorder {
        rect: (Px, Px, Px, Px),
        fill: Hsla,
        radius: f32,
        border_width: f32,
        border_color: Hsla,
    },
    Check {
        origin: (Px, Px),
        size: Px,
        color: Hsla,
    },
}

pub trait BlockComponent {
    fn decoration_scope(&self) -> DecorationScope {
        DecorationScope::None
    }
    fn paint_decoration(&self, _piece: &DecorationPiece, _theme: &DocumentTheme) -> Vec<PaintOp> {
        Vec::new()
    }
}

pub fn for_kind(kind: BlockKind) -> &'static dyn BlockComponent {
    match kind {
        BlockKind::DocRoot => &document::DocRootBlock,
        BlockKind::DocStart => &document::DocStartBlock,
        BlockKind::Paragraph => &paragraph::ParagraphBlock,
        BlockKind::Heading(_) => &heading::HeadingBlock,
        BlockKind::CodeBlock | BlockKind::MetadataBlock => &code_block::CodeBlock,
        BlockKind::BlockQuote => &block_quote::BlockQuoteBlock,
        BlockKind::List => &list::ListBlock,
        BlockKind::ListItem => &list::ListItemBlock,
        BlockKind::Table => &table::TableBlock,
        BlockKind::TableRow => &table::TableRowBlock,
        BlockKind::TableCell => &table::TableCellBlock,
        BlockKind::ThematicBreak => &thematic_break::ThematicBreakBlock,
        BlockKind::FootnoteDefinition => &block_quote::BlockQuoteBlock,
        BlockKind::Image => &image::ImageBlock,
        BlockKind::Mermaid => &code_block::WellBlock,
        BlockKind::Math => &code_block::WellBlock,
    }
}
