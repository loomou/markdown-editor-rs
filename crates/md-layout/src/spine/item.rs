use crate::box_tree::LayoutBoxId;
use crate::flow::HeightState;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FlowItemId {
    pub index: u32,
    pub generation: u32,
}

impl FlowItemId {
    pub const NONE: Self = FlowItemId {
        index: 0,
        generation: 0,
    };

    pub fn is_none(self) -> bool {
        self.index == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FlowItemKind {
    ContainerOpen { box_id: LayoutBoxId },
    Gap,
    Content { box_id: LayoutBoxId },
    Collapsed { box_id: LayoutBoxId },
    ContainerClose { box_id: LayoutBoxId },
}

#[derive(Clone, Copy, Debug)]
pub struct FlowItem {
    pub id: FlowItemId,
    pub kind: FlowItemKind,
    pub height: HeightState,
    pub(super) height_epoch: u64,
}

impl FlowItem {
    pub fn is_content(self) -> bool {
        matches!(self.kind, FlowItemKind::Content { .. })
    }

    pub(super) fn with_epoch(
        id: FlowItemId,
        kind: FlowItemKind,
        height: HeightState,
        height_epoch: u64,
    ) -> Self {
        FlowItem {
            id,
            kind,
            height,
            height_epoch,
        }
    }
}
