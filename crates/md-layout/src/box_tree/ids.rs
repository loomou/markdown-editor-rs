use md_core::block::{BlockId, BlockKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum BoxOwner {
    Block(BlockId),
    DocStart,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, Default)]
pub enum BoxRole {
    #[default]
    Frame,

    Preview,
    Cell,
    Bar,
    Slot,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct LayoutBoxId {
    pub owner: BoxOwner,
    pub role: BoxRole,
    pub local_key: u32,
}

impl LayoutBoxId {
    pub fn frame(block: BlockId) -> Self {
        LayoutBoxId {
            owner: BoxOwner::Block(block),
            role: BoxRole::Frame,
            local_key: 0,
        }
    }

    pub fn block(self) -> Option<BlockId> {
        match self.owner {
            BoxOwner::Block(b) => Some(b),
            BoxOwner::DocStart => None,
        }
    }

    pub fn doc_start() -> Self {
        LayoutBoxId {
            owner: BoxOwner::DocStart,
            role: BoxRole::Frame,
            local_key: 0,
        }
    }

    pub fn bar(block: BlockId) -> Self {
        LayoutBoxId {
            owner: BoxOwner::Block(block),
            role: BoxRole::Bar,
            local_key: 0,
        }
    }

    pub fn slot(block: BlockId) -> Self {
        LayoutBoxId {
            owner: BoxOwner::Block(block),
            role: BoxRole::Slot,
            local_key: 0,
        }
    }

    pub fn preview(block: BlockId) -> Self {
        LayoutBoxId {
            owner: BoxOwner::Block(block),
            role: BoxRole::Preview,
            local_key: 0,
        }
    }

    pub fn for_kind(kind: BlockKind, block: BlockId) -> Self {
        match kind {
            BlockKind::DocStart => Self::doc_start(),
            BlockKind::TableCell => LayoutBoxId {
                owner: BoxOwner::Block(block),
                role: BoxRole::Cell,
                local_key: 0,
            },
            _ => Self::frame(block),
        }
    }

    pub fn chrome(kind: BlockKind, block: BlockId) -> Option<Self> {
        match kind {
            BlockKind::BlockQuote | BlockKind::FootnoteDefinition => Some(Self::bar(block)),
            BlockKind::ListItem => Some(Self::slot(block)),
            _ => None,
        }
    }

    pub fn is_materializable_role(self) -> bool {
        matches!(self.role, BoxRole::Frame | BoxRole::Cell | BoxRole::Preview)
    }

    pub fn is_caret_role(self) -> bool {
        matches!(self.role, BoxRole::Frame | BoxRole::Cell)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TypeSlot {
    #[default]
    FromKind,
    Quote,
    TableHeader,
    Footnote,
    TaskDone,
}
