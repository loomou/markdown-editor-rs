use super::jobs::{
    collect_image_jobs, collect_math_jobs, collect_math_popover_job, collect_popover_job,
};
use super::{DisplayJob, EditorView, ImagePopover, ImageSchedule, MathPopover, Viewfinder};
use gpui::Context;
use md_content::{images, math, mermaid};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use std::collections::HashMap;

impl EditorView {
    fn warm_mermaid_blocks(&self, top: Px, viewport_h: Px) -> Vec<(BlockId, Px)> {
        let Some(eng) = self.state.incremental.as_ref() else {
            return Vec::new();
        };
        eng.warm_media_blocks(top, viewport_h)
            .into_iter()
            .filter(|(_, kind, _)| *kind == BlockKind::Mermaid)
            .map(|(block, _, width)| (block, width))
            .collect()
    }

    pub(super) fn schedule_mermaid(
        &mut self,
        texts: &[md_render::snapshot::TextPiece],
        viewfinder: Viewfinder,
        cx: &mut Context<'_, Self>,
    ) {
        let Viewfinder {
            top,
            height: viewport_h,
            dpr,
        } = viewfinder;
        let theme = self.state.theme;
        let theme_fp = mermaid::theme_fingerprint(&theme);
        self.mermaid.sync_theme(theme_fp, cx);
        let max_w = theme.decoration.mermaid_max_width;
        let max_h = theme.decoration.mermaid_max_height;
        let mut protect = Vec::new();
        let mut jobs = Vec::new();
        for t in texts {
            if t.kind != BlockKind::Mermaid || t.edit_source {
                continue;
            }
            let Some(key) = mermaid::key_for(
                &self.state.doc.document,
                t.block,
                t.content_width,
                max_w,
                max_h,
                dpr,
                theme_fp,
            ) else {
                continue;
            };
            protect.push(key);
            if self.mermaid.contains(&key) {
                continue;
            }
            if let Some(src) = self.state.doc.text(t.block) {
                jobs.push((key, src.to_string(), mermaid::spec_from_theme(&theme, key)));
            }
        }

        let mut warm = Vec::new();
        let mut warm_jobs = Vec::new();
        for (block, width) in self.warm_mermaid_blocks(top, viewport_h) {
            let Some(key) = mermaid::key_for(
                &self.state.doc.document,
                block,
                width,
                max_w,
                max_h,
                dpr,
                theme_fp,
            ) else {
                continue;
            };

            if protect.contains(&key) {
                continue;
            }
            warm.push(key);
            if self.mermaid.contains(&key) {
                continue;
            }
            if let Some(src) = self.state.doc.text(block) {
                warm_jobs.push((key, src.to_string(), mermaid::spec_from_theme(&theme, key)));
            }
        }
        self.mermaid.set_working_set(protect, warm, cx);

        let jobs: Vec<_> = jobs.into_iter().chain(warm_jobs).collect();
        for (key, src, spec) in jobs {
            if !self.mermaid.begin(key) {
                continue;
            }
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { mermaid::raster_with_svg(&src, &spec) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    if let Ok((_, svg)) = &result {
                        v.mermaid.store_svg(key.svg_key(), svg.clone());
                    }
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "mermaid");
                    }
                    let ready = result.is_ok();
                    v.mermaid.finish(key, result.map(|(image, _)| image), cx);
                    if ready && let Some(eng) = v.state.incremental.as_mut() {
                        eng.invalidate_island_for_block(key.block);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(super) fn schedule_math(
        &mut self,
        texts: &[md_render::snapshot::TextPiece],
        cells: &[md_render::snapshot::CellPiece],
        popover: Option<&MathPopover>,
        dpr: f64,
        cx: &mut Context<'_, Self>,
    ) {
        let theme = self.state.theme;
        let mut jobs: HashMap<math::MathKey, (String, math::RasterSpec, Vec<BlockId>)> =
            HashMap::new();
        let mut protect = Vec::new();
        for t in texts {
            collect_math_jobs(
                &t.art,
                t.kind,
                t.block,
                &theme,
                dpr,
                &mut jobs,
                &mut protect,
            );
        }
        for c in cells {
            collect_math_jobs(
                &c.art,
                BlockKind::TableCell,
                c.block,
                &theme,
                dpr,
                &mut jobs,
                &mut protect,
            );
        }

        if let Some(p) = popover {
            collect_math_popover_job(p, &mut jobs, &mut protect);
        }

        self.math.set_working_set(protect, std::iter::empty(), cx);
        for (key, (src, spec, blocks)) in jobs {
            if self.math.contains(&key) {
                continue;
            }
            if !self.math.begin(key.clone()) {
                continue;
            }
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { math::raster(&src, &spec) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "math");
                    }
                    let changed = v.math.finish(key, result, cx);
                    if changed && let Some(eng) = v.state.incremental.as_mut() {
                        for b in blocks {
                            eng.invalidate_island_for_block(b);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn warm_image_sources(&self, top: Px, viewport_h: Px) -> HashMap<String, Vec<BlockId>> {
        let Some(eng) = self.state.incremental.as_ref() else {
            return HashMap::new();
        };
        let blocks: Vec<BlockId> = eng
            .warm_media_blocks(top, viewport_h)
            .into_iter()
            .filter(|(_, kind, _)| *kind == BlockKind::Image)
            .map(|(block, _, _)| block)
            .collect();
        let mut jobs: HashMap<String, Vec<BlockId>> = HashMap::new();
        for block in blocks {
            let Some(dest) = self.doc_maps.block_image_dest.get(&block) else {
                continue;
            };
            if dest.is_empty() {
                continue;
            }
            let entry = jobs.entry(dest.clone()).or_default();
            if !entry.contains(&block) {
                entry.push(block);
            }
        }
        jobs
    }

    pub(super) fn schedule_images(
        &mut self,
        texts: &[md_render::snapshot::TextPiece],
        cells: &[md_render::snapshot::CellPiece],
        popover: Option<&ImagePopover>,
        viewfinder: Viewfinder,
        cx: &mut Context<'_, Self>,
    ) {
        let Viewfinder {
            top,
            height: viewport_h,
            dpr,
        } = viewfinder;
        let path = self.state.doc.source_path.clone();
        let mut protect = Vec::new();
        let mut source_jobs: HashMap<String, Vec<BlockId>> = HashMap::new();
        let mut display_jobs: HashMap<images::DisplayKey, DisplayJob> = HashMap::new();
        {
            let mut sched = ImageSchedule {
                dpr,
                cache: &self.images,
                protect: &mut protect,
                source_jobs: &mut source_jobs,
                display_jobs: &mut display_jobs,
            };
            for t in texts {
                collect_image_jobs(&t.art, t.block, &mut sched);
            }
            for c in cells {
                collect_image_jobs(&c.art, c.block, &mut sched);
            }

            if let Some(p) = popover {
                collect_popover_job(p, &mut sched);
            }
        }
        let hot_sources: Vec<String> = source_jobs.keys().cloned().collect();

        let mut warm_jobs = self.warm_image_sources(top, viewport_h);

        warm_jobs.retain(|dest, _| !source_jobs.contains_key(dest));
        let warm_sources: Vec<String> = warm_jobs.keys().cloned().collect();
        self.images
            .set_working_set(protect, hot_sources, warm_sources, cx);

        let source_jobs: Vec<(String, Vec<BlockId>)> =
            source_jobs.into_iter().chain(warm_jobs).collect();
        for (dest, blocks) in source_jobs {
            if self.images.source_contains(&dest) {
                continue;
            }
            if !self.images.begin_source(dest.clone()) {
                continue;
            }
            let source = self
                .doc_maps
                .data_source_links
                .get(&dest)
                .and_then(|id| self.doc_maps.link_raw.get(id))
                .map(|(raw, _)| raw.as_str())
                .unwrap_or(&dest);
            let resolved = images::resolve(source, path.as_deref()).or_else(|| {
                let local = std::path::PathBuf::from(source);
                local
                    .is_absolute()
                    .then_some(images::Resolved::Local(local))
            });
            cx.spawn(async move |this, cx| {
                let result = match this.update(cx, |view, cx| {
                    resolved.map(|resolved| images::load_source(resolved, view.remote_images, cx))
                }) {
                    Ok(Some(fut)) => cx.background_executor().spawn(fut).await,
                    Ok(None) => Err(md_content::Error::Image(md_i18n::Key::ImageBadPath.into())),
                    Err(_) => return,
                };
                let _ = this.update(cx, |v, cx| {
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, dest, "image source");
                    }
                    let changed = v.images.finish_source(dest, result, cx);
                    if changed && let Some(eng) = v.state.incremental.as_mut() {
                        for b in blocks {
                            eng.invalidate_island_for_block(b);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        for (key, job) in display_jobs {
            if self.images.display_contains(&key) {
                continue;
            }
            if !self.images.begin_display(key.clone()) {
                continue;
            }
            let DisplayJob { source, blocks } = job;
            cx.spawn(async move |this, cx| {
                let raster_key = key.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { images::raster_display(&source, &raster_key) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "image display");
                    }
                    let changed = v.images.finish_display(key, result, cx);
                    if changed && let Some(eng) = v.state.incremental.as_mut() {
                        for b in blocks {
                            eng.invalidate_island_for_block(b);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }
}
