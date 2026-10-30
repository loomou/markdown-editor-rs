use super::jobs::{
    collect_image_jobs, collect_math_jobs, collect_math_popover_job, collect_popover_job,
};
use super::{DisplayJob, EditorView, ImagePopover, ImageSchedule, MathPopover, Viewfinder};
use gpui::Context;
use md_content::{images, math, media, mermaid};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use std::collections::HashMap;

impl EditorView {
    pub(super) fn set_media_budget(
        &mut self,
        viewport: (Px, Px),
        dpr: f64,
        cx: &mut Context<'_, Self>,
    ) {
        let (width, height) = viewport;
        let dpr = dpr.max(0.25);
        let screen = (width.max(1.0) * height.max(1.0) * dpr * dpr * 4.0) as usize;
        let total = media::resident_override().unwrap_or_else(|| media::resident_ceiling(screen));
        media::MediaCoordinator::set_resident_limit(cx, total);
        let split = media::split_resident(total);
        let entries = images::Budget::for_viewport(width, height, dpr);
        self.images.set_budget(
            images::Budget {
                display_bytes: split.display,
                display_entries: entries.display_entries,
                source_bytes: split.source,
            },
            cx,
        );
        self.mermaid.set_budget(
            mermaid::Budget {
                bytes: split.mermaid,
                entries: entries.display_entries,
            },
            cx,
        );
        self.math.set_budget(split.math, cx);
    }

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
            if let Some(stored) = self.mermaid.begin_thaw(&key) {
                let estimate = media::estimate_decode_job(stored.px_w, stored.px_h);
                let Some(permit) =
                    media::MediaCoordinator::admit(cx, media::JobKind::Thaw, estimate)
                else {
                    self.mermaid.cancel_thaw(&key);
                    continue;
                };
                cx.spawn(async move |this, cx| {
                    let (result, permit) = cx
                        .background_executor()
                        .spawn(async move { (media::decode(&stored), permit) })
                        .await;
                    let _ = this.update(cx, |v, cx| {
                        let result = match result {
                            Some(image) => {
                                let bytes = image.bytes;
                                Some(image.with_lease(permit.into_lease(bytes)))
                            }
                            None => {
                                drop(permit);
                                None
                            }
                        };
                        let changed = v.mermaid.finish_thaw(&key, result);
                        if changed && let Some(eng) = v.state.incremental.as_mut() {
                            eng.invalidate_island_for_block(key.block);
                        }
                        cx.notify();
                    });
                })
                .detach();
                continue;
            }
            let Some(permit) = media::MediaCoordinator::admit(
                cx,
                media::JobKind::Mermaid,
                mermaid::estimate_job(&spec),
            ) else {
                continue;
            };
            if !self.mermaid.begin(key) {
                continue;
            }
            cx.spawn(async move |this, cx| {
                let (result, permit) = cx
                    .background_executor()
                    .spawn(async move { (mermaid::raster_with_svg(&src, &spec), permit) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    let result = match result {
                        Ok((image, svg)) => {
                            v.mermaid.store_svg(key.svg_key(), svg);
                            let bytes = image.bytes;
                            Ok(image.with_lease(permit.into_lease(bytes)))
                        }
                        Err(err) => {
                            drop(permit);
                            Err(err)
                        }
                    };
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "mermaid");
                    }
                    let ready = result.is_ok();
                    v.mermaid.finish(key, result, cx);
                    if ready && let Some(eng) = v.state.incremental.as_mut() {
                        eng.invalidate_island_for_block(key.block);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        self.schedule_mermaid_cooling(cx);
    }

    fn schedule_mermaid_cooling(&mut self, cx: &mut Context<'_, Self>) {
        let pressured = media::over_resident(cx);
        for key in self
            .mermaid
            .coolable_at(std::time::Instant::now(), pressured)
        {
            let Some((key, image)) = self.mermaid.begin_cool(&key) else {
                continue;
            };
            let (px_w, px_h, dpr) = (image.px_w, image.px_h, image.dpr);
            let Some(permit) = media::MediaCoordinator::admit(
                cx,
                media::JobKind::Cool,
                media::estimate_encode_job(px_w, px_h),
            ) else {
                self.mermaid.cancel_cool(&key);
                continue;
            };
            cx.spawn(async move |this, cx| {
                let codec = media::DEFAULT_WARM_CODEC;
                let (result, permit) = cx
                    .background_executor()
                    .spawn(async move { (media::encode(&image, codec), permit) })
                    .await;
                let _ = this.update(cx, |v, cx| match result {
                    Some(bytes) => {
                        let len = bytes.len();
                        let warm = media::WarmImage::new(bytes, codec, px_w, px_h, dpr)
                            .with_lease(permit.into_lease(len));
                        v.mermaid.finish_cool(&key, warm, cx);
                    }
                    None => {
                        drop(permit);
                        v.mermaid.cancel_cool(&key);
                    }
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
        let metrics = self.math.metrics_snapshot();
        for (key, (src, spec, blocks)) in jobs {
            if self.math.contains(&key) {
                continue;
            }
            let em = math::metric(&metrics, &key.latex, key.display)
                .flatten()
                .unwrap_or_else(|| math::MathEm::estimate(&key.latex));
            let bytes = math::estimate_job(em, key.font_size(), key.dpr());
            let Some(permit) = media::MediaCoordinator::admit(cx, media::JobKind::Math, bytes)
            else {
                continue;
            };
            if !self.math.begin(key.clone()) {
                continue;
            }
            cx.spawn(async move |this, cx| {
                let (result, permit) = cx
                    .background_executor()
                    .spawn(async move { (math::raster(&src, &spec), permit) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    let result = match result {
                        Ok(mut out) => {
                            match out.image.take() {
                                Some(image) => {
                                    let bytes = image.bytes;
                                    out.image = Some(image.with_lease(permit.into_lease(bytes)));
                                }
                                None => drop(permit),
                            }
                            Ok(out)
                        }
                        Err(err) => {
                            drop(permit);
                            Err(err)
                        }
                    };
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

    /// Replace the pixels of entries that have been off-screen for a while with
    /// their compressed form. A job that cannot be admitted is not started: the
    /// entry stays hot and eviction, which needs no budget, is what frees the
    /// bytes instead.
    fn schedule_cooling(&mut self, cx: &mut Context<'_, Self>) {
        let pressured = media::over_resident(cx);
        for key in self
            .images
            .coolable_at(std::time::Instant::now(), pressured)
        {
            let Some((token, image)) = self.images.begin_cool(&key) else {
                continue;
            };
            let (px_w, px_h, dpr) = (image.px_w, image.px_h, image.dpr);
            let Some(permit) = media::MediaCoordinator::admit(
                cx,
                media::JobKind::Cool,
                media::estimate_encode_job(px_w, px_h),
            ) else {
                self.images.cancel_cool(token);
                continue;
            };
            cx.spawn(async move |this, cx| {
                let codec = media::DEFAULT_WARM_CODEC;
                let (result, permit) = cx
                    .background_executor()
                    .spawn(async move { (media::encode(&image, codec), permit) })
                    .await;
                let _ = this.update(cx, |v, cx| match result {
                    Some(bytes) => {
                        let len = bytes.len();
                        let warm = media::WarmImage::new(bytes, codec, px_w, px_h, dpr)
                            .with_lease(permit.into_lease(len));
                        v.images.finish_cool(token, warm, cx);
                    }
                    None => {
                        drop(permit);
                        v.images.cancel_cool(token);
                    }
                });
            })
            .detach();
        }
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
            if self.images.source_contains(&dest) && !self.images.source_retry_due(&dest) {
                continue;
            }
            let Some(permit) = media::MediaCoordinator::admit(
                cx,
                media::JobKind::Source,
                images::estimate_source_job(),
            ) else {
                continue;
            };
            let Some(token) = self.images.begin_source(dest.clone()) else {
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
                    Ok(Some(fut)) => {
                        let (loaded, permit) = cx
                            .background_executor()
                            .spawn(async move { (fut.await, permit) })
                            .await;
                        match loaded {
                            Ok(media) => {
                                let resident = media.resident_bytes();
                                Ok(media.with_lease(permit.into_lease(resident)))
                            }
                            Err(err) => {
                                drop(permit);
                                Err(err)
                            }
                        }
                    }
                    Ok(None) => Err(md_content::images::SourceError::Fatal(
                        md_content::Error::Image(md_i18n::Key::ImageBadPath.into()),
                    )),
                    Err(_) => return,
                };
                let _ = this.update(cx, |v, cx| {
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, dest = %token.dest, "image source");
                    }
                    let changed = v.images.finish_source(token, result, cx);
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
            let DisplayJob { source, blocks } = job;
            if let Some((token, warm)) = self.images.begin_thaw(&key) {
                let estimate = media::estimate_decode_job(warm.px_w, warm.px_h);
                let Some(permit) =
                    media::MediaCoordinator::admit(cx, media::JobKind::Thaw, estimate)
                else {
                    self.images.cancel_thaw(token);
                    continue;
                };
                cx.spawn(async move |this, cx| {
                    let (result, permit) = cx
                        .background_executor()
                        .spawn(async move { (media::decode(&warm), permit) })
                        .await;
                    let _ = this.update(cx, |v, cx| {
                        let result = match result {
                            Some(image) => {
                                let bytes = image.bytes;
                                Some(image.with_lease(permit.into_lease(bytes)))
                            }
                            None => {
                                drop(permit);
                                tracing::warn!(?key, "warm payload did not inflate");
                                None
                            }
                        };
                        let changed = v.images.finish_thaw(token, result, cx);
                        if changed && let Some(eng) = v.state.incremental.as_mut() {
                            for b in blocks {
                                eng.invalidate_island_for_block(b);
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
                continue;
            }
            let bytes = images::estimate_display_job(&source, &key);
            let Some(permit) = media::MediaCoordinator::admit(cx, media::JobKind::Display, bytes)
            else {
                continue;
            };
            let Some(display_token) = self.images.begin_display(key.clone()) else {
                continue;
            };
            cx.spawn(async move |this, cx| {
                let raster_key = key.clone();
                let (result, permit) = cx
                    .background_executor()
                    .spawn(async move { (images::raster_display(&source, &raster_key), permit) })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    let result = super::media_budget::deliver(result, permit);
                    if let Err(ref err) = result {
                        tracing::warn!(error = %err, "image display");
                    }
                    let changed = v.images.finish_display(display_token, result, cx);
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
        self.schedule_cooling(cx);
    }
}
