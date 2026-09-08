use super::GpuiShaper;
use super::bands::mermaid_fit;
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::InlineRun;
use md_layout::shaper::{MeasureKind, MeasureResult, ShapeIdentity, TextMeasure};

impl TextMeasure for GpuiShaper {
    fn begin_island(&self) {
        let mut s = self.stats.borrow_mut();
        s.measure_calls = 0;
        s.shape_calls = 0;
    }

    fn measure(
        &self,
        text: &str,
        runs: &[InlineRun],
        avail_width: Px,
        kind: MeasureKind,
        block_kind: BlockKind,
        ident: ShapeIdentity,
    ) -> MeasureResult {
        if kind == MeasureKind::Probe {
            self.stats.borrow_mut().probe_calls += 1;
            return MeasureResult {
                width: 0.0,
                height: 0.0,
                rows: 0,
                first_baseline: 0.0,
            };
        }
        let a = self.artifact(text, runs, avail_width, block_kind, ident);
        let width = if block_kind == BlockKind::Mermaid && !ident.edit_source {
            mermaid_fit(
                avail_width,
                self.decoration.mermaid_max_width,
                self.decoration.mermaid_max_height,
                self.mermaid_fitted
                    .get(&(ident.index, ident.generation))
                    .copied(),
            )
            .0
        } else {
            avail_width
        };
        MeasureResult {
            width,
            height: self.cap_well_height(block_kind, ident.edit_source, a.height),
            rows: a.rows,
            first_baseline: a.first_baseline,
        }
    }
}

impl GpuiShaper {
    pub fn well_view_height(&self, kind: BlockKind, edit_source: bool, height: Px) -> Px {
        self.cap_well_height(kind, edit_source, height)
    }

    fn cap_well_height(&self, kind: BlockKind, edit_source: bool, height: Px) -> Px {
        let max = if edit_source {
            self.decoration.code_max_height
        } else {
            match kind {
                BlockKind::CodeBlock | BlockKind::MetadataBlock => self.decoration.code_max_height,
                BlockKind::Math => self.decoration.math_max_height,
                BlockKind::Mermaid => self.decoration.mermaid_max_height,
                BlockKind::Image => self.decoration.image_max_height,
                _ => return height,
            }
        };
        height.min(max.max(1.0))
    }
}
