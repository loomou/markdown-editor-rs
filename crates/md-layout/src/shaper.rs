use crate::box_tree::{BoxRole, TypeSlot};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::InlineRun;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ShapeIdentity {
    pub index: u32,
    pub role: BoxRole,

    pub edit_source: bool,
    pub generation: u32,
    pub revision: u64,
    pub type_slot: TypeSlot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureResult {
    pub width: Px,
    pub height: Px,
    pub rows: u32,
    pub first_baseline: Px,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeasureKind {
    Final,
    Probe,
}

pub trait TextMeasure {
    fn begin_island(&self);

    fn measure(
        &self,
        text: &str,
        runs: &[InlineRun],
        avail_width: Px,
        kind: MeasureKind,
        block_kind: BlockKind,
        ident: ShapeIdentity,
    ) -> MeasureResult;
}
