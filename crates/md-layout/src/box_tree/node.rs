use super::BoxStyleId;
use super::ids::{LayoutBoxId, TypeSlot};
use crate::shaper::ShapeIdentity;
use md_core::block::{BlockKind, NodeExtra};
use md_core::inline::InlineAlign;

#[derive(Clone, Debug)]
pub struct BoxNode {
    pub(crate) id: LayoutBoxId,
    pub(crate) kind: BlockKind,
    pub(crate) style_id: BoxStyleId,
    pub(crate) parent: Option<LayoutBoxId>,
    pub(crate) children: BoxChildren,

    pub(crate) text_id: Option<u32>,
    pub(crate) extra: NodeExtra,
    pub(crate) content_revision: u64,
    pub(crate) content_generation: u32,

    pub(crate) edit_source: bool,
    pub(crate) type_slot: TypeSlot,
}

impl BoxNode {
    pub fn id(&self) -> LayoutBoxId {
        self.id
    }

    pub fn kind(&self) -> BlockKind {
        self.kind
    }

    pub fn parent(&self) -> Option<LayoutBoxId> {
        self.parent
    }

    pub fn children(&self) -> &BoxChildren {
        &self.children
    }

    pub fn text_id(&self) -> Option<u32> {
        self.text_id
    }

    pub fn extra(&self) -> NodeExtra {
        self.extra
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub fn content_generation(&self) -> u32 {
        self.content_generation
    }

    pub fn edit_source(&self) -> bool {
        self.edit_source
    }

    pub fn type_slot(&self) -> TypeSlot {
        self.type_slot
    }

    pub fn shape_kind(&self) -> BlockKind {
        if self.edit_source {
            BlockKind::CodeBlock
        } else {
            self.kind
        }
    }

    pub fn shape_ident(&self) -> ShapeIdentity {
        let index = self
            .id
            .block()
            .expect("DocStart box carries no block index; shaping it would borrow block 0");
        ShapeIdentity {
            index,
            role: self.id.role,
            edit_source: self.edit_source,
            generation: self.content_generation,
            revision: self.content_revision,
            type_slot: self.type_slot,
        }
    }

    pub fn inline_align(&self) -> InlineAlign {
        self.extra.inline_align()
    }
}

#[derive(Clone, Debug)]
pub enum BoxChildren {
    Vertical(Vec<LayoutBoxId>),

    Island(Vec<LayoutBoxId>),
    None,
}
