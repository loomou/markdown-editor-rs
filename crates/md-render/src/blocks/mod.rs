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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Radii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Radii {
    pub fn all(radius: f32) -> Self {
        Radii {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub fn any(self) -> bool {
        self.top_left > 0.0
            || self.top_right > 0.0
            || self.bottom_right > 0.0
            || self.bottom_left > 0.0
    }
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
        radii: Radii,
    },
    RoundBorder {
        rect: (Px, Px, Px, Px),
        fill: Hsla,
        radii: Radii,
        border_width: f32,
        border_color: Hsla,
    },
    Arc {
        corner: Corner,
        at: (Px, Px),
        radius: f32,
        width: f32,
        color: Hsla,
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
