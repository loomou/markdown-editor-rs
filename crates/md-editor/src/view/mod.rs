use self::find_bar::FindBar;
use self::media_zoom::{MediaChrome, MediaZoom};
use self::table_toolbar::TableChrome;
use crate::keymap::Keymap;
use gpui::{AnyWindowHandle, WeakEntity};
use md_content::images::{self, ImageCache};
use md_content::math::{self, MathCache};
use md_content::mermaid::MermaidCache;
use md_core::Px;
use md_core::block::{BlockId, BlockKind, TableCellAlign};
use md_core::doc::{Cursor, Doc};
use md_core::document::TableOp;
use md_core::inline::InlineAlign;
use md_layout::style::BoxLayoutEnvironment;
use md_render::search::SearchMatch;
use md_render::snapshot::{Frame, LayoutSnapshot};
use md_theme::{DocumentTheme, ThemeColor};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

mod clipboard;
mod draw;
mod drop;
mod editor;
mod element;
mod events;
mod input;
mod insert_table;
mod jobs;
mod keys;
pub(crate) mod media_zoom;
mod open;
mod paint;
mod popover;
mod prepaint;
mod recover;
mod save;
mod schedule;
mod scrollbar;
mod search;
mod selection;
mod table_cols;
mod table_reorder;
mod unsaved;

pub mod find_bar;
pub(crate) mod table_commands;
pub(crate) mod table_toolbar;

pub(crate) use insert_table::InsertTableField;
pub(crate) use save::SaveConflictChoice;
pub(crate) use unsaved::{PendingNav, UnsavedChoice, unsaved_file_name};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarGeom {
    hit_x: Px,
    hit_w: Px,
    track_y: Px,
    track_h: Px,
    thumb_x: Px,
    thumb_y: Px,
    thumb_w: Px,
    thumb_h: Px,
    max_scroll: Px,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WellScroll {
    x: Px,
    y: Px,
}

#[derive(Clone, Copy, Debug)]
struct WellHit {
    id: BlockId,
    x: Px,
    y: Px,
    view_w: Px,
    view_h: Px,
    content_w: Px,
    content_h: Px,
}

#[derive(Clone, Copy, Debug)]
struct WellBar {
    vertical: bool,
    hit_x: Px,
    hit_y: Px,
    hit_w: Px,
    hit_h: Px,
    thumb_x: Px,
    thumb_y: Px,
    thumb_w: Px,
    thumb_h: Px,
    max_scroll: Px,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct CompatKey {
    pub surface_q: (i64, i64),
    pub scale_q: i64,

    pub doc_epoch: u64,
    pub params_gen: u64,
}

pub(crate) struct StableFrame {
    pub snapshot: Rc<LayoutSnapshot>,
    pub compat: CompatKey,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum PaintFault {
    #[default]
    None,

    DropVisibleGeometry,

    GlyphPaintError,
}

pub struct EditorState {
    pub doc: Doc,
    pub cursor: Cursor,
    pub selection: Option<(Cursor, Cursor)>,
    pub marked: Option<(BlockId, Range<usize>)>,
    pub scroll: Px,

    pub resolved_top: Px,
    pub env: BoxLayoutEnvironment,

    pub shape_cache: Rc<md_content::shaper::ShapeCache>,

    pub(crate) diag: Rc<RefCell<Diagnostics>>,

    pub incremental: Option<md_render::incremental::IncrementalEngine>,
    pub incremental_enabled: bool,

    pub incremental_anchor_override: Option<md_render::incremental::ScrollAnchor>,

    pub incremental_last_anchor: Option<md_render::incremental::ScrollAnchor>,

    pub frame_times: std::collections::VecDeque<std::time::Instant>,

    pub show_fps: bool,

    pub stress_redraw: bool,

    pub(crate) last_stable: Option<StableFrame>,

    pub(crate) paint_fault: PaintFault,

    pub theme: DocumentTheme,
}

#[derive(Default, Clone)]
pub(crate) struct Diagnostics {
    pub fps: u32,

    pub frame_ms: f64,
    pub caret: Option<(Px, Px, Px, Px)>,
    pub materialized: usize,
    pub last_exact: usize,
}

pub(crate) fn overlay_diag_labels(
    show_fps: bool,
    stress: bool,
    fps: u32,
    frame_ms: f64,
    materialized: usize,
    last_exact: usize,
) -> Vec<String> {
    let mut labels = Vec::new();
    if show_fps {
        labels.push(format!("{fps} fps"));
        labels.push(format!("{frame_ms:.1} ms"));
        labels.push(format!("store={materialized}"));
        labels.push(format!("last_exact={last_exact}"));
    }
    if stress {
        labels.push("stress".to_string());
    }
    labels
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Direction {
    Prev,
    Next,
}

impl Direction {
    fn sign(self) -> i32 {
        match self {
            Direction::Prev => -1,
            Direction::Next => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LineEdge {
    Start,
    End,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CursorMotion {
    Move,
    Extend,
}

impl CursorMotion {
    fn from_shift(shift: bool) -> Self {
        if shift {
            CursorMotion::Extend
        } else {
            CursorMotion::Move
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PendingClick {
    x: Px,
    y: Px,
    motion: CursorMotion,
    follow_link: bool,
    select_word: bool,
}

pub(crate) struct PendingVertical {
    dir: Direction,
    motion: CursorMotion,
}

pub(crate) struct EditorElement {
    state: gpui::Entity<EditorView>,
}

pub(crate) struct PrepaintState {
    frame: Frame,
    hitbox: gpui::Hitbox,

    caret_on: bool,

    refresh: bool,

    popover: Option<ImagePopover>,

    math_popover: Option<MathPopover>,
}

#[derive(Clone, Debug)]
struct ImagePopover {
    dest: String,

    plan: Option<PopoverPlan>,
}

#[derive(Clone, Debug)]
struct MathPopover {
    plan: Option<MathPopoverPlan>,
}

#[derive(Clone, Debug)]
struct MathPopoverPlan {
    plate: (Px, Px, Px, Px),

    math_at: (Px, Px),
    key: math::MathKey,

    color: ThemeColor,
}

#[derive(Clone, Debug)]
struct PopoverPlan {
    plate: (Px, Px, Px, Px),

    image_at: (Px, Px),

    image_size: (Px, Px),
    key: images::DisplayKey,
}

#[derive(Default)]
pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) matches: Vec<SearchMatch>,
    pub(crate) active: Option<usize>,
    pub(crate) capped: bool,
    pub(crate) reveal: Option<(i32, u8)>,
    pub(crate) refresh: u8,
}

#[derive(Default)]
struct SaveState {
    in_flight: bool,
    epoch: u64,
    source_disk_state: Option<crate::platform::fs_atomic::DiskState>,
    autosave_epoch: u64,
    autosave_retry: bool,

    save_as_retry: Option<std::path::PathBuf>,
    pending_after_save: Option<PendingNav>,
}

#[derive(Default)]
struct TableUiState {
    chrome: Option<TableChrome>,
    more_open: bool,
    align_hover: Option<TableCellAlign>,
    picker_open: bool,
    picker_hover: Option<(usize, usize)>,
    picker_drag: bool,
    col_widths: HashMap<BlockId, Vec<Px>>,
    col_resize: Option<table_cols::ColResizeDrag>,
    grip_hover: Option<table_reorder::GripHover>,
    reorder: Option<table_reorder::ReorderDrag>,
    toolbar_hover: bool,
    menu_op: Option<TableOp>,
}

pub struct EditorView {
    pub state: EditorState,
    pub focus: gpui::FocusHandle,

    pub(crate) pending_click: Option<PendingClick>,

    pub(crate) drag_pointer: Option<(Px, Px)>,

    pub(crate) pending_vertical: Option<PendingVertical>,

    pub(crate) select_anchor: Option<Cursor>,

    pub(crate) dragging: bool,

    enter_block_edit_on_click: bool,

    pub(crate) scrollbar_drag: Option<Px>,

    pub(crate) follow_caret: bool,

    park_caret_top: Option<u8>,

    stale_paint: bool,

    pub(crate) blink: crate::ui::blink::Blink,

    pub(crate) keymap: Keymap,

    pub(crate) find_bar: Option<WeakEntity<FindBar>>,
    pub(crate) search_open: bool,
    pub(crate) search: SearchState,

    mermaid: MermaidCache,

    pub(crate) zoom_raster: Option<media_zoom::ZoomRaster>,
    pub(crate) zoom_raster_job: Option<media_zoom::ZoomRasterJob>,
    math: MathCache,
    images: ImageCache,
    remote_images: bool,
    doc_maps: DocShapeMaps,
    well_scroll: HashMap<BlockId, WellScroll>,
    well_bar_drag: Option<(BlockId, bool, Px)>,
    pending_open: Option<String>,
    open_epoch: u64,
    ime_stale: bool,
    save: SaveState,
    pub(crate) save_conflict: Option<std::path::PathBuf>,
    pub(crate) save_conflict_focus: gpui::FocusHandle,
    autosave: bool,
    host_window: Option<AnyWindowHandle>,
    force_close: bool,
    pub(crate) unsaved_nav: Option<PendingNav>,
    pub(crate) unsaved_focus: gpui::FocusHandle,
    pub(crate) insert_table: Option<insert_table::InsertTableState>,
    pub(crate) insert_table_focus: gpui::FocusHandle,
    os_title: Option<String>,
    pub(crate) notice: Option<crate::Error>,
    recovery: Option<crate::store::recovery::Recovery>,
    recovery_epoch: u64,

    table_ui: TableUiState,

    media_hover: Option<BlockId>,
    pub(crate) media_chrome: Option<MediaChrome>,
    media_chrome_hover: bool,
    pub(crate) media_zoom: Option<MediaZoom>,
}

struct DocShapeMaps {
    revision: u64,
    path: Option<std::path::PathBuf>,

    links_len: usize,
    link_dests: Rc<HashMap<u32, String>>,

    link_raw: Rc<HashMap<u32, (String, String)>>,

    data_source_links: Rc<HashMap<String, u32>>,
    block_image_dest: Rc<HashMap<u32, String>>,
    block_code_lang: Rc<HashMap<u32, String>>,
}

struct DisplayJob {
    source: std::sync::Arc<gpui::RenderImage>,
    blocks: Vec<BlockId>,
}

struct ImageSchedule<'a> {
    dpr: f64,
    cache: &'a ImageCache,
    protect: &'a mut Vec<images::DisplayKey>,
    source_jobs: &'a mut HashMap<String, Vec<BlockId>>,
    display_jobs: &'a mut HashMap<images::DisplayKey, DisplayJob>,
}

#[derive(Clone, Copy)]
struct Viewfinder {
    top: Px,
    height: Px,
    dpr: f64,
}

#[derive(Clone, Copy)]
struct ArtifactPaint<'a> {
    x: f32,
    y: f32,
    inner: Px,
    kind: BlockKind,
    inline_align: InlineAlign,
    align: gpui::TextAlign,
    theme: &'a DocumentTheme,
    dpr: f64,
    lookup: &'a HashMap<math::MathKey, math::ReadyImage>,
    images: &'a HashMap<images::DisplayKey, images::ReadyImage>,
    failed_math: &'a HashSet<math::MathKey>,
}
