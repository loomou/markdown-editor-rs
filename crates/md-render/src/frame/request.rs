use crate::search::SearchMatch;
use crate::snap::SnapOperator;
use md_content::shaper::GpuiShaper;
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::{Cursor, Doc};
use md_layout::assembly::Assembly;
use md_layout::style::BoxLayoutEnvironment;
use md_theme::DocumentTheme;
use std::ops::Range;

#[derive(Clone, Copy)]
pub struct FrameContext<'a> {
    pub doc: &'a Doc,
    pub env: BoxLayoutEnvironment,
    pub shaper: &'a GpuiShaper,
    pub snap: &'a SnapOperator,
    pub theme: &'a DocumentTheme,
}

#[derive(Clone)]
pub struct FrameRequest<'a> {
    pub viewport: (Px, Px),
    pub scroll: Px,
    pub cursor: Cursor,
    pub selection: Option<(Cursor, Cursor)>,
    pub marked: Option<(BlockId, Range<usize>)>,
    pub search_query: &'a str,
    pub search_skip: Option<SearchMatch>,
}

#[derive(Clone, Copy)]
pub(super) struct Pass<'a> {
    pub(super) assembly: &'a Assembly,
    pub(super) env: BoxLayoutEnvironment,
    pub(super) shaper: &'a GpuiShaper,
    pub(super) snap: &'a SnapOperator,
    pub(super) theme: &'a DocumentTheme,
    pub(super) scroll: Px,
    pub(super) viewport: (Px, Px),
}

impl<'a> Pass<'a> {
    pub(super) fn new(
        cx: FrameContext<'a>,
        assembly: &'a Assembly,
        req: &FrameRequest<'_>,
    ) -> Pass<'a> {
        Pass {
            assembly,
            env: cx.env,
            shaper: cx.shaper,
            snap: cx.snap,
            theme: cx.theme,
            scroll: req.scroll,
            viewport: req.viewport,
        }
    }

    pub(super) fn view(&self) -> (Px, Px) {
        (self.scroll, self.scroll + self.viewport.1)
    }
}
