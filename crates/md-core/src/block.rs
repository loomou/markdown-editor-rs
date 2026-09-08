pub type BlockId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BlockKind {
    DocRoot,

    DocStart,
    Paragraph,
    Heading(u8),
    CodeBlock,
    MetadataBlock,
    BlockQuote,
    List,
    ListItem,
    Table,

    TableRow,

    TableCell,
    ThematicBreak,
    FootnoteDefinition,
    Image,
    Mermaid,
    Math,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TextEditStrategy {
    Phrasing,
    BlockSource,
    Literal,
}

impl BlockKind {
    pub(crate) fn text_edit_strategy(self) -> TextEditStrategy {
        match self {
            BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::TableCell => {
                TextEditStrategy::Phrasing
            }
            BlockKind::Image => TextEditStrategy::BlockSource,
            BlockKind::DocRoot
            | BlockKind::DocStart
            | BlockKind::CodeBlock
            | BlockKind::MetadataBlock
            | BlockKind::BlockQuote
            | BlockKind::List
            | BlockKind::ListItem
            | BlockKind::Table
            | BlockKind::TableRow
            | BlockKind::ThematicBreak
            | BlockKind::FootnoteDefinition
            | BlockKind::Mermaid
            | BlockKind::Math => TextEditStrategy::Literal,
        }
    }

    pub fn is_vertical_container(self) -> bool {
        matches!(
            self,
            BlockKind::DocRoot
                | BlockKind::BlockQuote
                | BlockKind::List
                | BlockKind::ListItem
                | BlockKind::Table
                | BlockKind::FootnoteDefinition
        )
    }

    pub fn is_text_leaf(self) -> bool {
        matches!(
            self,
            BlockKind::Paragraph
                | BlockKind::Heading(_)
                | BlockKind::CodeBlock
                | BlockKind::MetadataBlock
                | BlockKind::TableCell
                | BlockKind::Image
                | BlockKind::Mermaid
                | BlockKind::Math
        )
    }

    pub fn supports_block_edit(self) -> bool {
        matches!(
            self,
            BlockKind::Mermaid | BlockKind::Math | BlockKind::Image
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListMarker {
    #[default]
    Dash,
    Plus,
    Star,
    Period,
    Parenthesis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeFenceMarker {
    #[default]
    Backtick,
    Tilde,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableCellAlign {
    #[default]
    Start,
    Center,
    End,
}

impl TableCellAlign {
    pub(crate) fn bits(self) -> u8 {
        match self {
            TableCellAlign::Start => 0,
            TableCellAlign::Center => 2,
            TableCellAlign::End => 3,
        }
    }
}

impl From<u8> for TableCellAlign {
    fn from(bits: u8) -> Self {
        match bits & 3 {
            2 => TableCellAlign::Center,
            3 => TableCellAlign::End,
            _ => TableCellAlign::Start,
        }
    }
}

impl AlertKind {
    pub fn label(self) -> &'static str {
        match self {
            AlertKind::Note => "NOTE",
            AlertKind::Tip => "TIP",
            AlertKind::Important => "IMPORTANT",
            AlertKind::Warning => "WARNING",
            AlertKind::Caution => "CAUTION",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeExtra {
    #[default]
    None,
    List {
        start: Option<u64>,
        marker: ListMarker,
        loose: bool,
        source_loose: bool,
    },
    TaskItem {
        checked: bool,
    },
    HeaderRow,
    Table {
        alignments: u64,
        source: Option<(u32, u32)>,
    },
    Cell {
        align: TableCellAlign,
        header: bool,
    },
    QuoteAlert {
        kind: AlertKind,
        lowercase_mask: u16,
        blank_after_marker: bool,
    },
    Image {
        dest: u32,
        source: Option<(u32, u32)>,
    },
    CodeFence {
        lang: Option<u32>,
        marker: CodeFenceMarker,
        len: u16,
    },
    MathFence,
    IndentedCode,
    FootnoteLabel {
        label: u32,
    },
}

impl NodeExtra {
    pub fn inline_align(self) -> super::inline::InlineAlign {
        match self {
            NodeExtra::Cell {
                align: TableCellAlign::Center,
                ..
            } => super::inline::InlineAlign::Center,
            NodeExtra::Cell {
                align: TableCellAlign::End,
                ..
            } => super::inline::InlineAlign::End,
            _ => super::inline::InlineAlign::Start,
        }
    }

    pub fn ordered_start(self) -> Option<u64> {
        match self {
            NodeExtra::List { start, .. } => start,
            _ => None,
        }
    }

    pub fn list_marker(self) -> ListMarker {
        match self {
            NodeExtra::List { marker, .. } => marker,
            _ => ListMarker::Dash,
        }
    }

    pub fn list_loose(self) -> bool {
        match self {
            NodeExtra::List { loose, .. } => loose,
            _ => false,
        }
    }

    pub(crate) fn list_source_loose(self) -> bool {
        match self {
            NodeExtra::List { source_loose, .. } => source_loose,
            _ => false,
        }
    }

    pub fn task_checked(self) -> Option<bool> {
        match self {
            NodeExtra::TaskItem { checked } => Some(checked),
            _ => None,
        }
    }

    pub fn table_header(self) -> bool {
        matches!(
            self,
            NodeExtra::HeaderRow | NodeExtra::Cell { header: true, .. }
        )
    }

    pub fn quote_alert(self) -> Option<AlertKind> {
        match self {
            NodeExtra::QuoteAlert { kind, .. } => Some(kind),
            _ => None,
        }
    }

    pub(crate) fn quote_alert_style(self) -> Option<(AlertKind, u16, bool)> {
        match self {
            NodeExtra::QuoteAlert {
                kind,
                lowercase_mask,
                blank_after_marker,
            } => Some((kind, lowercase_mask, blank_after_marker)),
            _ => None,
        }
    }

    pub fn image_dest(self) -> Option<u32> {
        match self {
            NodeExtra::Image { dest, .. } => Some(dest),
            _ => None,
        }
    }

    pub fn code_fence_lang(self) -> Option<u32> {
        match self {
            NodeExtra::CodeFence { lang, .. } => lang,
            _ => None,
        }
    }

    pub(crate) fn code_fence_style(self) -> Option<(CodeFenceMarker, usize)> {
        match self {
            NodeExtra::CodeFence { marker, len, .. } => Some((marker, usize::from(len))),
            _ => None,
        }
    }

    pub(crate) fn math_fenced(self) -> bool {
        matches!(self, NodeExtra::MathFence)
    }

    pub fn code_is_indented(self) -> bool {
        matches!(self, NodeExtra::IndentedCode)
    }

    pub(crate) fn footnote_label(self) -> Option<u32> {
        match self {
            NodeExtra::FootnoteLabel { label } => Some(label),
            _ => None,
        }
    }
}

pub(crate) fn pack_alignments(bits: impl IntoIterator<Item = u8>) -> u64 {
    let mut v = 0u64;
    for (i, b) in bits.into_iter().take(32).enumerate() {
        v |= u64::from(b & 3) << (i * 2);
    }
    v
}

pub(crate) fn alignment_at(packed: u64, col: usize) -> u8 {
    if col >= 32 {
        0
    } else {
        ((packed >> (col * 2)) & 3) as u8
    }
}

pub(crate) const TABLE_ALIGN_COLS: usize = 32;

pub(crate) fn set_alignment(packed: u64, col: usize, bits: u8) -> u64 {
    if col >= TABLE_ALIGN_COLS {
        return packed;
    }
    let shift = col * 2;
    let mask = 3u64 << shift;
    (packed & !mask) | (u64::from(bits & 3) << shift)
}

pub(crate) fn insert_alignment(packed: u64, col: usize, bits: u8) -> u64 {
    if col >= TABLE_ALIGN_COLS {
        return packed;
    }
    let shift = col * 2;
    let low_mask = if shift == 0 { 0 } else { (1u64 << shift) - 1 };
    let low = packed & low_mask;
    let high = if shift >= 62 {
        0
    } else {
        (packed >> shift) << (shift + 2)
    };
    low | (u64::from(bits & 3) << shift) | high
}

pub(crate) fn remove_alignment(packed: u64, col: usize) -> u64 {
    if col >= TABLE_ALIGN_COLS {
        return packed;
    }
    let shift = col * 2;
    let low_mask = if shift == 0 { 0 } else { (1u64 << shift) - 1 };
    let low = packed & low_mask;
    let high = if shift >= 62 {
        0
    } else {
        packed >> (shift + 2)
    };
    low | (high << shift)
}

pub(crate) fn swap_alignment(packed: u64, a: usize, b: usize) -> u64 {
    if a == b {
        return packed;
    }
    let ba = alignment_at(packed, a);
    let bb = alignment_at(packed, b);
    set_alignment(set_alignment(packed, a, bb), b, ba)
}

#[cfg(test)]
mod tests {
    use super::{
        alignment_at, insert_alignment, pack_alignments, remove_alignment, set_alignment,
        swap_alignment,
    };

    #[test]
    fn packed_insert_remove_and_swap_keep_column_bits() {
        let p = pack_alignments([0, 2, 3]);
        assert_eq!(alignment_at(p, 0), 0);
        assert_eq!(alignment_at(p, 1), 2);
        assert_eq!(alignment_at(p, 2), 3);
        let inserted = insert_alignment(p, 1, 0);
        assert_eq!(alignment_at(inserted, 0), 0);
        assert_eq!(alignment_at(inserted, 1), 0);
        assert_eq!(alignment_at(inserted, 2), 2);
        assert_eq!(alignment_at(inserted, 3), 3);
        assert_eq!(remove_alignment(inserted, 1), p);
        let at_front = insert_alignment(p, 0, 3);
        assert_eq!(alignment_at(at_front, 0), 3);
        assert_eq!(alignment_at(at_front, 1), 0);
        assert_eq!(alignment_at(at_front, 2), 2);
        assert_eq!(alignment_at(at_front, 3), 3);
        let swapped = swap_alignment(p, 1, 2);
        assert_eq!(alignment_at(swapped, 1), 3);
        assert_eq!(alignment_at(swapped, 2), 2);
        assert_eq!(alignment_at(set_alignment(p, 0, 3), 0), 3);
    }
}
