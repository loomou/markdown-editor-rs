use md_content::shaper::ShapeArtifact;
use md_core::Px;
use md_core::block::{AlertKind, BlockId, BlockKind};
use md_core::inline::InlineAlign;
use md_layout::assembly::Assembly;
use md_layout::box_tree::{BoxRole, LayoutBoxId};
use std::collections::BTreeMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

pub type DeviceRect = (Px, Px, Px, Px);

static GEOMETRY_REV: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_geometry_revision() -> u64 {
    GEOMETRY_REV.fetch_add(1, AtomicOrdering::Relaxed)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SnapshotRevs {
    pub document: u64,
    pub layout: u64,
    pub viewport: u64,
}

#[derive(Clone)]
pub struct TextPiece {
    pub box_id: LayoutBoxId,
    pub block: BlockId,
    pub kind: BlockKind,

    pub edit_source: bool,

    pub content_origin_device: (Px, Px),
    pub content_width: Px,
    pub view_height: Px,
    pub art: Rc<ShapeArtifact>,
    pub align: InlineAlign,
}

impl TextPiece {
    pub(crate) fn accepts_caret(&self) -> bool {
        self.box_id.is_caret_role()
    }

    pub(crate) fn is_display_surface(&self) -> bool {
        !self.edit_source
    }
}

#[derive(Clone)]
pub struct CellPiece {
    pub cell_box: LayoutBoxId,
    pub block: BlockId,
    pub table: BlockId,

    pub rect_device: DeviceRect,
    pub content_origin_device: (Px, Px),
    pub art: Rc<ShapeArtifact>,
    pub content_width: Px,
    pub align: InlineAlign,
    pub header: bool,
}

#[derive(Clone)]
pub struct DecorationPiece {
    pub rect_device: DeviceRect,
    pub clip_device: Option<DeviceRect>,
    pub kind: BlockKind,
    pub role: BoxRole,
    pub hit_block: BlockId,
    pub gutter_dot: Option<(Px, Px)>,
    pub gutter_label: Option<String>,
    pub gutter_label_at: Option<(Px, Px)>,
    pub gutter_label_size: f32,
    pub list_nest: u8,
    pub task: Option<bool>,
    pub alert: Option<AlertKind>,
}

#[derive(Clone)]
pub struct AtomSpan {
    pub top: Px,
}

#[derive(Clone)]
pub struct LayoutSnapshot {
    pub document_revision: u64,
    pub layout_revision: u64,
    pub viewport_revision: u64,
    pub geometry_revision: u64,
    pub total_height: Px,
    pub scroll: Px,
    pub viewport: (Px, Px),
    pub texts: Vec<TextPiece>,
    pub decorations: Vec<DecorationPiece>,
    pub cells: Vec<CellPiece>,
    pub caret_device: Option<DeviceRect>,
    pub caret_logical_y: Option<Px>,
    pub selection_device: Vec<DeviceRect>,
    pub inline_code_device: Vec<DeviceRect>,
    pub search_device: Vec<DeviceRect>,
    pub search_active_device: Vec<DeviceRect>,
    pub ime_device: Vec<DeviceRect>,
    pub spans: BTreeMap<LayoutBoxId, AtomSpan>,
    pub content_atoms_painted: u64,
    pub absent_visible: Vec<LayoutBoxId>,
}

pub struct Frame {
    pub snapshot: Rc<LayoutSnapshot>,
    pub assembly: Assembly,
}

impl Deref for Frame {
    type Target = LayoutSnapshot;
    fn deref(&self) -> &LayoutSnapshot {
        self.snapshot.as_ref()
    }
}
