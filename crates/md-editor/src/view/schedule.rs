use super::jobs::{
    collect_image_jobs, collect_math_jobs, collect_math_popover_job, collect_popover_job,
};
use super::{DisplayJob, EditorView, ImagePopover, ImageSchedule, MathPopover, Viewfinder};
use gpui::Context;
use md_content::media::MediaWorkingSet;
use md_content::{images, math, mermaid};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use std::collections::HashMap;

struct MermaidPlan {
    protect: Vec<mermaid::MermaidKey>,
    warm: Vec<mermaid::MermaidKey>,
    jobs: Vec<(mermaid::MermaidKey, String, mermaid::RasterSpec)>,
}

struct MathPlan {
    protect: Vec<math::MathKey>,
    jobs: HashMap<math::MathKey, (String, math::RasterSpec, Vec<BlockId>)>,
}

struct ImagePlan {
    protect: Vec<images::DisplayKey>,
    hot_sources: Vec<String>,
    warm_sources: Vec<String>,
    source_jobs: Vec<(String, Vec<BlockId>)>,
    display_jobs: Vec<(images::DisplayKey, DisplayJob)>,
}

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

    fn plan_mermaid(
        &self,
        texts: &[md_render::snapshot::TextPiece],
        viewfinder: Viewfinder,
    ) -> MermaidPlan {
        let Viewfinder {
            top,
            height: viewport_h,
            dpr,
        } = viewfinder;
        let theme = self.state.theme;
        let theme_fp = mermaid::theme_fingerprint(&theme);
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
            if self.media.mermaid().contains(&key) {
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
            if self.media.mermaid().contains(&key) {
                continue;
            }
            if let Some(src) = self.state.doc.text(block) {
                warm_jobs.push((key, src.to_string(), mermaid::spec_from_theme(&theme, key)));
            }
        }
        MermaidPlan {
            protect,
            warm,
            jobs: jobs.into_iter().chain(warm_jobs).collect(),
        }
    }

    fn plan_math(
        &self,
        texts: &[md_render::snapshot::TextPiece],
        cells: &[md_render::snapshot::CellPiece],
        popover: Option<&MathPopover>,
        dpr: f64,
    ) -> MathPlan {
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
        MathPlan { protect, jobs }
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

    fn plan_images(
        &self,
        texts: &[md_render::snapshot::TextPiece],
        cells: &[md_render::snapshot::CellPiece],
        popover: Option<&ImagePopover>,
        viewfinder: Viewfinder,
    ) -> ImagePlan {
        let Viewfinder {
            top,
            height: viewport_h,
            dpr,
        } = viewfinder;
        let mut protect = Vec::new();
        let mut source_jobs: HashMap<String, Vec<BlockId>> = HashMap::new();
        let mut display_jobs: HashMap<images::DisplayKey, DisplayJob> = HashMap::new();
        {
            let mut sched = ImageSchedule {
                dpr,
                cache: self.media.images(),
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
        ImagePlan {
            protect,
            hot_sources,
            warm_sources,
            source_jobs: source_jobs.into_iter().chain(warm_jobs).collect(),
            display_jobs: display_jobs.into_iter().collect(),
        }
    }

    pub(super) fn schedule_media(
        &mut self,
        texts: &[md_render::snapshot::TextPiece],
        cells: &[md_render::snapshot::CellPiece],
        math_popover: Option<&MathPopover>,
        image_popover: Option<&ImagePopover>,
        viewfinder: Viewfinder,
        cx: &mut Context<'_, Self>,
    ) {
        let theme_fp = mermaid::theme_fingerprint(&self.state.theme);
        self.media.mermaid_mut().sync_theme(theme_fp, cx);

        let mermaid = self.plan_mermaid(texts, viewfinder);
        let math = self.plan_math(texts, cells, math_popover, viewfinder.dpr);
        let images = self.plan_images(texts, cells, image_popover, viewfinder);

        self.media.submit_working_set(
            MediaWorkingSet {
                mermaid_hot: mermaid.protect.clone(),
                mermaid_warm: mermaid.warm.clone(),
                math_hot: math.protect.clone(),
                image_hot: images.protect.clone(),
                image_hot_sources: images.hot_sources.clone(),
                image_warm_sources: images.warm_sources.clone(),
            },
            cx,
        );

        self.run_mermaid(mermaid.jobs, cx);
        self.run_math(math.jobs, cx);
        self.visible_animations = images
            .display_jobs
            .iter()
            .filter_map(|(_, job)| job.media.animation().cloned())
            .collect();
        self.run_images(images, cx);
        self.drive_animations(cx);
    }

    fn run_mermaid(
        &mut self,
        jobs: Vec<(mermaid::MermaidKey, String, mermaid::RasterSpec)>,
        cx: &mut Context<'_, Self>,
    ) {
        for (key, src, spec) in jobs {
            if !self.media.mermaid_mut().begin(key) {
                continue;
            }
            let renderer = cx.svg_renderer();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { mermaid::raster_with_svg(&src, &spec, &renderer) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    if let Ok((_, svg)) = &result {
                        v.media.mermaid_mut().store_svg(key.svg_key(), svg.clone());
                    }
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "mermaid");
                    }
                    let ready = result.is_ok();
                    v.media
                        .mermaid_mut()
                        .finish(key, result.map(|(image, _)| image), cx);
                    v.media.enforce(cx);
                    if ready && let Some(eng) = v.state.incremental.as_mut() {
                        eng.invalidate_island_for_block(key.block);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn run_math(
        &mut self,
        jobs: HashMap<math::MathKey, (String, math::RasterSpec, Vec<BlockId>)>,
        cx: &mut Context<'_, Self>,
    ) {
        for (key, (src, spec, blocks)) in jobs {
            if self.media.math().contains(&key) {
                continue;
            }
            if !self.media.math_mut().begin(key.clone()) {
                continue;
            }
            let prepared = self.media.math().prepared(&key);
            let renderer = cx.svg_renderer();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        match prepared {
                            Some(prepared) => math::raster_prepared(&prepared, &spec, &renderer),
                            None => math::raster(&src, &spec, &renderer),
                        }
                    })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "math");
                    }
                    let changed = v.media.math_mut().finish(key, result, cx);
                    v.media.enforce(cx);
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

    fn run_images(&mut self, plan: ImagePlan, cx: &mut Context<'_, Self>) {
        let path = self.state.doc.source_path.clone();
        for (dest, blocks) in plan.source_jobs {
            if self.media.images().source_contains(&dest)
                && !self.media.images().source_retry_due(&dest)
            {
                continue;
            }
            let Some(token) = self.media.images_mut().begin_source(dest.clone()) else {
                continue;
            };
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
                    Ok(None) => Err(md_content::images::SourceError::Fatal(
                        md_content::Error::Image(md_i18n::Key::ImageBadPath.into()),
                    )),
                    Err(_) => return,
                };
                let _ = this.update(cx, |v, cx| {
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, dest = %token.dest, "image source");
                    }
                    let changed = v.media.images_mut().finish_source(token, result, cx);
                    v.media.enforce(cx);
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
        for (key, job) in plan.display_jobs {
            if self.media.images().display_contains(&key) {
                continue;
            }
            let Some(display_token) = self.media.images_mut().begin_display(key.clone()) else {
                continue;
            };
            let DisplayJob { media, blocks } = job;
            let renderer = cx.svg_renderer();
            cx.spawn(async move |this, cx| {
                let raster_key = key.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { images::raster_display(&media, &raster_key, renderer) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "image display");
                    }
                    let changed = v
                        .media
                        .images_mut()
                        .finish_display(display_token, result, cx);
                    v.media.enforce(cx);
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
