use super::draw::{is_scroll_well, well_content_w, well_scroll_xy};
use super::scrollbar::{
    park_block_top_margin, park_block_top_scroll, search_reveal_scroll, select_autoscroll_can_move,
    select_autoscroll_delta,
};
use super::{
    CompatKey, CursorMotion, EditorElement, EditorView, ImagePopover, MathPopover, PaintFault,
    PendingClick, PendingVertical, PrepaintState, StableFrame, Viewfinder, popover,
};
use gpui::{App, Bounds, Context, Pixels, Window};
use md_content::shaper::{GpuiShaper, ShapeMedia};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::{Cursor, Doc};
use md_core::document::TableStep;
use md_layout::island::{FallbackSolver, IslandSolver};
use md_layout::style::BoxLayoutEnvironment;
use md_render::frame::{
    FrameContext, FrameRequest, compose, from_assembly, selection_vertical_span,
};
use md_render::query::hit_test;
use md_render::search::SearchMatch;
use md_render::snap::SnapOperator;
use md_render::snapshot::{Frame, SnapshotRevs};
use md_theme::DocumentTheme;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

struct FrameInputs {
    env: BoxLayoutEnvironment,
    scroll: Px,
    cursor: Cursor,
    selection: Option<(Cursor, Cursor)>,
    marked: Option<(BlockId, Range<usize>)>,
    shape_cache: Rc<md_content::shaper::ShapeCache>,
    theme: DocumentTheme,
    search_query: String,
    search_skip: Option<SearchMatch>,
}

struct FrameBuild<'a> {
    env: BoxLayoutEnvironment,
    theme: &'a DocumentTheme,
    shaper: &'a GpuiShaper,
    snap: &'a SnapOperator,
    viewport: (Px, Px),
    scroll: Px,
    cursor: Cursor,
    selection: Option<(Cursor, Cursor)>,
    marked: Option<(BlockId, Range<usize>)>,
    search_query: String,
    search_skip: Option<SearchMatch>,
}

#[derive(Clone, Copy)]
struct FrameGear<'a> {
    env: BoxLayoutEnvironment,
    theme: &'a DocumentTheme,
    shaper: &'a GpuiShaper,
    snap: &'a SnapOperator,
    viewport: (Px, Px),

    scale: f64,

    cursor: Cursor,
    selection: Option<(Cursor, Cursor)>,

    paint_rev: u64,

    caret_ly: Option<Px>,
}

#[derive(Clone, Copy, Default)]
struct BuildTimings {
    engine: Duration,
    assemble: Duration,
    geometry: Duration,
}

impl EditorElement {
    pub(super) fn prepaint_impl(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let t_prepaint_start = std::time::Instant::now();
        let scale = f64::from(window.scale_factor());
        let hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);

        let FrameInputs {
            env,
            scroll,
            cursor,
            selection,
            marked,
            shape_cache,
            theme,
            search_query,
            search_skip,
        } = self.read_frame_inputs(bounds, cx);

        let cold_once = md_render::cold_trace::enabled();

        let t_docmaps0 = std::time::Instant::now();
        let media = self.sync_shape_media(cx);
        let t_docmaps = t_docmaps0.elapsed();
        let t_font0 = std::time::Instant::now();
        let shaper = self.make_shaper(window, cx, &theme, scale, &shape_cache, media);
        let t_font = t_font0.elapsed();
        shape_cache.begin_frame(shaper.env_fingerprint());
        let snap = SnapOperator::new(scale);

        let viewport = (
            f32::from(bounds.size.width) as Px,
            f32::from(bounds.size.height) as Px,
        );

        let (mut frame, timings) = self.build_frame(
            FrameBuild {
                env,
                theme: &theme,
                shaper: &shaper,
                snap: &snap,
                viewport,
                scroll,
                cursor,
                selection,
                marked,
                search_query,
                search_skip,
            },
            cx,
        );
        shape_cache.end_frame();

        self.inject_geometry_fault(&mut frame, cx);
        self.stabilise_frame(&mut frame, bounds, scale, env, cx);

        let gear = FrameGear {
            env,
            theme: &theme,
            shaper: &shaper,
            snap: &snap,
            viewport,
            scale,
            cursor,
            selection,
            paint_rev: frame.geometry_revision,
            caret_ly: frame.caret_logical_y,
        };

        self.clamp_well_scrolls(&frame, cx);
        self.follow_caret(&frame, gear, cx);
        self.park_caret_at_top(&frame, gear, cx);
        self.reveal_search_match(&frame, gear, cx);
        self.tick_search_refresh(cx);
        self.apply_pending_input(&frame, gear, cx);
        self.record_frame_stats(&frame, t_prepaint_start, cx);

        let caret_on = self.state.update(cx, |v, _| {
            let live = v.focus.is_focused(&*window);
            v.blink.set_live(live);
            live && v.blink.visible()
        });
        if cold_once {
            log_cold_trace(
                t_prepaint_start,
                t_docmaps,
                t_font,
                timings,
                &frame,
                &shaper,
            );
        }
        let (popover, math_popover) = self.plan_popovers(&frame, gear, cx);

        self.state.update(cx, |v, cx| {
            v.sync_table_chrome(&frame.cells, cx);
            v.sync_media_chrome(&frame.texts, cx);

            let viewfinder = Viewfinder {
                top: frame.scroll,
                height: viewport.1,
                dpr: scale,
            };
            v.schedule_mermaid(&frame.texts, viewfinder, cx);
            v.schedule_math(&frame.texts, &frame.cells, math_popover.as_ref(), scale, cx);
            v.schedule_images(&frame.texts, &frame.cells, popover.as_ref(), viewfinder, cx);
            v.schedule_zoom_raster(frame.viewport, scale, cx);
        });
        let refresh = self
            .state
            .update(cx, |v, _| std::mem::take(&mut v.stale_paint));
        PrepaintState {
            frame,
            hitbox,
            caret_on,
            refresh,
            popover,
            math_popover,
        }
    }

    fn read_frame_inputs(&mut self, bounds: Bounds<Pixels>, cx: &mut App) -> FrameInputs {
        let mut inputs = self.state.update(cx, |v, _cx| {
            let mut env = v.state.env;
            env.viewport_width = f32::from(bounds.size.width) as Px;
            v.state.env = env;
            FrameInputs {
                env,
                scroll: v.state.scroll,
                cursor: v.state.cursor,
                selection: v.state.selection,
                marked: v.state.marked.clone(),
                shape_cache: v.state.shape_cache.clone(),
                theme: v.state.theme,
                search_query: if v.search.capped || v.search.matches.is_empty() {
                    String::new()
                } else {
                    v.search.query.clone()
                },
                search_skip: v
                    .search
                    .active
                    .and_then(|i| v.search.matches.get(i).copied())
                    .map(|m| {
                        let hit = v.state.doc.visual_range(m.block, m.start..m.end);
                        SearchMatch {
                            block: m.block,
                            start: hit.start,
                            end: hit.end,
                        }
                    }),
            }
        });
        inputs.env.viewport_width = f32::from(bounds.size.width) as Px;
        inputs
    }

    fn sync_shape_media(&mut self, cx: &mut App) -> ShapeMedia {
        self.state.update(cx, |v, _| {
            v.doc_maps.sync(&v.state.doc);
            ShapeMedia {
                mermaid_fitted: v.mermaid.fitted_snapshot(),
                math_metrics: v.math.metrics_snapshot(),
                math_gen: v.math.metrics_gen(),
                image_sizes: v.images.sizes_snapshot(),
                image_failed: v.images.failed_sources(),
                image_gen: v.images.sizes_gen(),
                link_dests: Rc::clone(&v.doc_maps.link_dests),
                link_raw: Rc::clone(&v.doc_maps.link_raw),
                block_image_dest: Rc::clone(&v.doc_maps.block_image_dest),
                block_code_lang: Rc::clone(&v.doc_maps.block_code_lang),
            }
        })
    }

    fn make_shaper(
        &self,
        window: &Window,
        cx: &mut App,
        theme: &DocumentTheme,
        scale: f64,
        shape_cache: &Rc<md_content::shaper::ShapeCache>,
        media: ShapeMedia,
    ) -> GpuiShaper {
        GpuiShaper::new(window, cx, theme, scale, shape_cache.clone(), media)
    }

    fn build_frame(&mut self, b: FrameBuild<'_>, cx: &mut App) -> (Frame, BuildTimings) {
        let FrameBuild {
            env,
            theme,
            shaper,
            snap,
            viewport,
            mut scroll,
            cursor,
            selection,
            marked,
            search_query,
            search_skip,
        } = b;
        let solver: &dyn IslandSolver = &FallbackSolver;
        let mut t = BuildTimings::default();
        let frame = self.state.update(cx, |v, _cx| {
            v.prune_table_col_widths();
            if !v.state.incremental_enabled {
                return compose(
                    FrameContext {
                        doc: &v.state.doc,
                        env,
                        shaper,
                        snap,
                        theme,
                    },
                    &FrameRequest {
                        viewport,
                        scroll,
                        cursor,
                        selection,
                        marked,
                        search_query: &search_query,
                        search_skip,
                    },
                    solver,
                    Some(&v.table_ui.col_widths),
                );
            }

            let t0 = std::time::Instant::now();
            if v.state.incremental.is_none() {
                let keep = 2.0;
                let pad = keep * viewport.1;
                let top = (scroll - pad).max(0.0);
                let bottom = scroll + viewport.1 + pad;
                v.state.incremental = Some(md_render::incremental::IncrementalEngine::with_window(
                    &v.state.doc.document,
                    env,
                    md_render::incremental::Estimator::from_theme(theme),
                    theme.layout_theme(),
                    top,
                    bottom,
                ));
            }
            let Some(engine) = v.state.incremental.as_mut() else {
                unreachable!("engine was just inserted above");
            };
            t.engine = t0.elapsed();

            if (engine.viewport_width() - env.viewport_width).abs() > f64::EPSILON {
                engine.set_viewport_width(env.viewport_width);
            }
            engine.set_table_col_tracks(&v.table_ui.col_widths);

            let changes = v.state.doc.take_changes();
            if !changes.is_empty() {
                engine.apply_changes(&v.state.doc.document, &changes);
            }

            scroll = engine.sync_block_edit_retain_y(
                &v.state.doc.document,
                cursor.block,
                scroll,
                shaper,
                solver,
            );

            scroll = scroll.clamp(0.0, (engine.total_height() - viewport.1).max(0.0));
            v.state.scroll = scroll;

            let t_asm0 = std::time::Instant::now();
            if v.search.reveal.is_some() || v.park_caret_top.is_some() {
                let _ = engine.ensure_composed_block(
                    &v.state.doc.document,
                    cursor.block,
                    shaper,
                    solver,
                );
            } else {
                engine.materialize_pin_block(cursor.block, shaper, solver);
            }
            if v.park_caret_top.is_some()
                && let Some(ly) = engine.spine_caret_y(cursor, shaper, env)
            {
                let margin =
                    park_block_top_margin(theme.box_style(BlockKind::Paragraph).margin.top);
                v.park_caret_top = None;
                if let Some(s) =
                    park_block_top_scroll(ly, scroll, viewport.1, engine.total_height(), margin)
                {
                    scroll = s;
                    v.state.scroll = scroll;
                }
            }
            if let Some((blk, _)) = marked.as_ref() {
                engine.materialize_pin_block(*blk, shaper, solver);
            }

            let scroll_anchor = v
                .state
                .incremental_anchor_override
                .unwrap_or_else(|| engine.anchor_at_y(scroll, shaper, solver));

            let (assembly, published) = engine.assemble_with_doc(
                &v.state.doc.document,
                scroll_anchor,
                viewport.1,
                shaper,
                solver,
            );

            let scroll_used = published.resolved_top;
            t.assemble = t_asm0.elapsed();
            v.state.incremental_last_anchor = Some(scroll_anchor);

            v.state.resolved_top = scroll_used;

            let t_geom0 = std::time::Instant::now();
            let revs = SnapshotRevs {
                document: v.state.doc.document.revision(),
                layout: engine.publish_gen(),
                viewport: engine.viewport_params_gen(),
            };
            let built = from_assembly(
                FrameContext {
                    doc: &v.state.doc,
                    env,
                    shaper,
                    snap,
                    theme,
                },
                &FrameRequest {
                    viewport,

                    scroll: scroll_used,
                    cursor,
                    selection,
                    marked,
                    search_query: &search_query,
                    search_skip,
                },
                assembly,
                revs,
            );
            t.geometry = t_geom0.elapsed();
            built
        });
        (frame, t)
    }

    fn inject_geometry_fault(&mut self, frame: &mut Frame, cx: &mut App) {
        if self.state.read(cx).state.paint_fault != PaintFault::DropVisibleGeometry {
            return;
        }
        let victims: Vec<_> = frame.cells.iter().map(|c| c.cell_box).take(1).collect();
        if let Some(v) = victims.first() {
            let row = frame.assembly.tree.get(*v).parent();
            if let Some(row) = row {
                let tree = Rc::clone(&frame.assembly.tree);
                let snap = Rc::make_mut(&mut frame.snapshot);
                snap.cells
                    .retain(|c| tree.get(c.cell_box).parent() != Some(row));
                snap.absent_visible.push(row);
            }
        }
    }

    fn stabilise_frame(
        &mut self,
        frame: &mut Frame,
        bounds: Bounds<Pixels>,
        scale: f64,
        env: BoxLayoutEnvironment,
        cx: &mut App,
    ) {
        let compat = CompatKey {
            surface_q: (
                (f64::from(bounds.size.width) * 64.0).round() as i64,
                (f64::from(bounds.size.height) * 64.0).round() as i64,
            ),
            scale_q: (scale * 4096.0).round() as i64,
            doc_epoch: self
                .state
                .read(cx)
                .state
                .incremental
                .as_ref()
                .map_or(0, |e| e.doc_rebuilds()),
            params_gen: self
                .state
                .read(cx)
                .state
                .incremental
                .as_ref()
                .map_or(0, |e| e.viewport_params_gen()),
        };

        let breached = !frame.absent_visible.is_empty();

        if breached {
            let reusable = self
                .state
                .read(cx)
                .state
                .last_stable
                .as_ref()
                .filter(|s| s.compat == compat)
                .map(|s| (s.snapshot.scroll, Rc::clone(&s.snapshot)));
            if let Some(stable) = reusable {
                frame.snapshot = stable.1;
            } else {
                let absent = frame.absent_visible.clone();
                let mut extras = Vec::new();
                for box_id in absent {
                    let est = frame
                        .assembly
                        .heights
                        .get(&box_id)
                        .map(|h| match h {
                            md_layout::flow::HeightState::Exact(v) => *v,
                            md_layout::flow::HeightState::Estimated(v) => *v,
                        })
                        .unwrap_or(md_layout::style::DEFAULT_LINE_HEIGHT);
                    let sp = frame.spans.get(&box_id).map(|s| s.top);
                    if let Some(top) = sp {
                        let x = md_render::boxtree::inline_offset(&frame.assembly.tree, box_id);
                        let w = frame.assembly.tree.avail_width(box_id, env.viewport_width);
                        let kind = frame.assembly.tree.get(box_id).kind();
                        let Some(hit_block) = md_render::boxtree::block_id_of(box_id) else {
                            continue;
                        };
                        extras.push(md_render::snapshot::DecorationPiece {
                            rect_device: (x, top - frame.scroll, w, est),
                            clip_device: None,
                            kind,
                            role: md_layout::box_tree::BoxRole::Frame,
                            hit_block,
                            gutter_dot: None,
                            gutter_label: None,
                            gutter_label_at: None,
                            gutter_label_size: 0.0,
                            list_nest: 0,
                            task: None,
                            alert: None,
                        });
                    }
                }
                let snap = Rc::make_mut(&mut frame.snapshot);
                snap.decorations.extend(extras);
                snap.caret_device = None;
                snap.selection_device.clear();
            }
        } else {
            self.state.update(cx, |st, _| {
                st.state.last_stable = Some(StableFrame {
                    snapshot: Rc::clone(&frame.snapshot),
                    compat,
                });
            });
        }
    }

    fn clamp_well_scrolls(&mut self, frame: &Frame, cx: &mut App) {
        self.state.update(cx, |v, _| {
            for t in &frame.texts {
                if !is_scroll_well(t.kind, t.edit_source) {
                    continue;
                }
                let max_x = (well_content_w(t) - t.content_width).max(0.0);
                let max_y = (t.art.height - t.view_height).max(0.0);
                if let Some(s) = v.well_scroll.get_mut(&t.block) {
                    s.x = s.x.clamp(0.0, max_x);
                    s.y = s.y.clamp(0.0, max_y);
                }
            }
        });
    }

    fn follow_caret(&mut self, frame: &Frame, gear: FrameGear<'_>, cx: &mut App) {
        if self.state.read(cx).search.reveal.is_some()
            || self.state.read(cx).park_caret_top.is_some()
        {
            return;
        }
        let Some(ly) = gear.caret_ly else {
            return;
        };
        let cursor = gear.cursor;
        let kind = self
            .state
            .read(cx)
            .state
            .doc
            .kind(cursor.block)
            .unwrap_or(BlockKind::Paragraph);
        let lh = gear.shaper.row_advance_for(kind);
        let vh = gear.viewport.1;
        let total = frame.total_height;
        self.state.update(cx, |v, cx| {
            if !v.follow_caret {
                return;
            }
            v.follow_caret = false;
            let top = v.state.scroll;
            let want = if ly < top {
                Some(ly - lh)
            } else if ly + lh > top + vh {
                Some(ly + lh * 2.0 - vh)
            } else {
                None
            };
            let mut changed = false;
            if let Some(s) = want {
                v.state.scroll = s.clamp(0.0, (total - vh).max(0.0));
                changed = true;
            }
            if let Some((cx0, cy0, cw, ch)) = frame.caret_device
                && let Some(t) = frame
                    .texts
                    .iter()
                    .find(|t| t.block == cursor.block && is_scroll_well(t.kind, t.edit_source))
            {
                let (ox, oy) = t.content_origin_device;
                let mut s = v.well_scroll.get(&t.block).copied().unwrap_or_default();
                let local_x = cx0 - ox;
                let local_y = cy0 - oy;
                if local_x < s.x {
                    s.x = local_x;
                } else if local_x + cw > s.x + t.content_width {
                    s.x = local_x + cw - t.content_width;
                }
                if local_y < s.y {
                    s.y = local_y;
                } else if local_y + ch > s.y + t.view_height {
                    s.y = local_y + ch - t.view_height;
                }
                s.x =
                    s.x.clamp(0.0, (well_content_w(t) - t.content_width).max(0.0));
                s.y = s.y.clamp(0.0, (t.art.height - t.view_height).max(0.0));
                if v.well_scroll.get(&t.block).copied() != Some(s) {
                    v.well_scroll.insert(t.block, s);
                    changed = true;
                }
            }
            if changed {
                mark_stale(v, cx);
            }
        });
    }

    fn park_caret_at_top(&mut self, frame: &Frame, gear: FrameGear<'_>, cx: &mut App) {
        let Some(tries) = self.state.read(cx).park_caret_top else {
            return;
        };
        let caret_ly = gear.caret_ly;
        let vh = gear.viewport.1;
        let total = frame.total_height;
        let margin = park_block_top_margin(gear.theme.box_style(BlockKind::Paragraph).margin.top);
        self.state.update(cx, |v, cx| {
            let Some(ly) = caret_ly else {
                if tries >= 1 {
                    v.park_caret_top = None;
                } else {
                    v.park_caret_top = Some(tries + 1);
                    mark_stale(v, cx);
                }
                return;
            };
            v.park_caret_top = None;
            if let Some(s) = park_block_top_scroll(ly, v.state.scroll, vh, total, margin) {
                v.state.scroll = s;
                mark_stale(v, cx);
            }
        });
    }

    fn reveal_search_match(&mut self, frame: &Frame, gear: FrameGear<'_>, cx: &mut App) {
        let Some((dir, tries)) = self.state.read(cx).search.reveal else {
            return;
        };
        let (from, to) = gear.selection.unwrap_or((gear.cursor, gear.cursor));
        let span = selection_vertical_span(
            &frame.assembly,
            &frame.spans,
            gear.shaper,
            gear.env,
            from,
            to,
        );
        let vh = gear.viewport.1;
        let total = frame.total_height;
        self.state.update(cx, |v, cx| {
            let Some((y0, y1, lh)) = span else {
                if tries >= 1 {
                    v.search.reveal = None;
                } else {
                    v.search.reveal = Some((dir, tries + 1));
                    mark_stale(v, cx);
                }
                return;
            };
            v.search.reveal = None;
            if let Some(s) = search_reveal_scroll(
                y0,
                y1,
                v.state.scroll,
                vh,
                total,
                lh,
                dir,
                &v.state.theme.chrome,
            ) {
                v.state.scroll = s;
                mark_stale(v, cx);
            }
        });
    }

    fn tick_search_refresh(&mut self, cx: &mut App) {
        self.state.update(cx, |v, _| {
            if v.search.reveal.is_none() && v.search.refresh > 0 {
                v.search.refresh -= 1;
            }
        });
    }

    fn apply_pending_input(&mut self, frame: &Frame, gear: FrameGear<'_>, cx: &mut App) {
        let kind = self
            .state
            .read(cx)
            .state
            .doc
            .kind(gear.cursor.block)
            .unwrap_or(BlockKind::Paragraph);
        let lh = gear.shaper.row_advance_for(kind);
        self.state.update(cx, |v, cx| {
            let click = v.pending_click.take();
            let vert = v.pending_vertical.take();
            let mut moved = false;

            if let Some(click) = click {
                moved |= v.apply_click(frame, gear, click);
            }
            if v.dragging && v.scrollbar_drag.is_none() && v.well_bar_drag.is_none() {
                moved |= v.drag_selection(frame, gear);
            }
            if let Some(vert) = vert {
                moved = v.step_caret_vertically(frame, gear, vert, lh, moved);
            }

            if moved {
                mark_stale(v, cx);
            }
        });
    }

    fn record_frame_stats(&mut self, frame: &Frame, started: std::time::Instant, cx: &mut App) {
        let diag = self.state.read(cx).state.diag.clone();

        let (fps, frame_ms) = {
            let now = std::time::Instant::now();
            let ms = now.duration_since(started).as_secs_f64() * 1000.0;
            let f = self.state.update(cx, |v, _cx| {
                v.state.frame_times.push_back(now);

                while let Some(&front) = v.state.frame_times.front() {
                    if now.duration_since(front).as_secs_f64() > 1.0 {
                        v.state.frame_times.pop_front();
                    } else {
                        break;
                    }
                }
                v.state.frame_times.len() as u32
            });
            (f, ms)
        };

        let (store_n, last_n) = self
            .state
            .read(cx)
            .state
            .incremental
            .as_ref()
            .map_or((0, 0), |eng| {
                (eng.materialized_count(), eng.last_exact_count())
            });

        let mut d = diag.borrow_mut();
        d.fps = fps;
        d.frame_ms = frame_ms;
        d.caret = frame.caret_device;
        d.materialized = store_n;
        d.last_exact = last_n;
    }

    fn plan_popovers(
        &mut self,
        frame: &Frame,
        gear: FrameGear<'_>,
        cx: &mut App,
    ) -> (Option<ImagePopover>, Option<MathPopover>) {
        let v = self.state.read(cx);
        let sizes = v.images.sizes_snapshot();
        let metrics = v.math.metrics_snapshot();
        let input = popover::PopoverInput {
            theme: gear.theme,
            shaper: gear.shaper,
            env: gear.env,
            snap: gear.snap,
            scroll: frame.scroll,
            viewport: gear.viewport,
            dpr: gear.scale,
            sizes: &sizes,
            link_dests: &v.doc_maps.link_dests,
            math_metrics: &metrics,
        };
        (
            popover::plan(&v.state.doc, frame, &input),
            popover::plan_math(&v.state.doc, frame, &input),
        )
    }
}

impl EditorView {
    fn apply_click(&mut self, frame: &Frame, gear: FrameGear<'_>, click: PendingClick) -> bool {
        let Some(c) = hit_test(
            &frame.snapshot,
            gear.paint_rev,
            (click.x, click.y),
            gear.shaper,
            |id| well_scroll_xy(&self.well_scroll, id),
        ) else {
            return false;
        };
        self.apply_click_hit(c, click);
        if self.enter_block_edit_on_click
            && click.motion == CursorMotion::Move
            && self.drag_pointer.is_none()
        {
            self.commit_block_edit_on_click();
        }
        self.enter_block_edit_on_click = false;
        true
    }

    fn drag_selection(&mut self, frame: &Frame, gear: FrameGear<'_>) -> bool {
        let Some(p) = self.drag_pointer else {
            return false;
        };
        let vh = gear.viewport.1;
        let total = frame.total_height;
        let mut moved = false;
        if let Some(c) = hit_test(
            &frame.snapshot,
            gear.paint_rev,
            (p.0, p.1),
            gear.shaper,
            |id| well_scroll_xy(&self.well_scroll, id),
        ) {
            self.place_cursor(c, CursorMotion::Extend);
            self.follow_caret = false;
            moved = true;
        }
        let dy = select_autoscroll_delta(p.1, vh, &gear.theme.chrome);
        if select_autoscroll_can_move(dy, self.state.scroll, vh, total) {
            self.state.scroll = (self.state.scroll + dy).clamp(0.0, (total - vh).max(0.0));
            moved = true;
        }
        moved
    }

    fn step_caret_vertically(
        &mut self,
        frame: &Frame,
        gear: FrameGear<'_>,
        vert: PendingVertical,
        lh: Px,
        moved: bool,
    ) -> bool {
        self.abort_composing();
        let mut moved = moved;
        let vh = gear.viewport.1;
        let dir = vert.dir.sign();
        if let Some((mut cx0, mut cy0, _cw, ch)) = frame.caret_device {
            if let Some(t) = frame.texts.iter().find(|t| {
                t.block == self.state.cursor.block && is_scroll_well(t.kind, t.edit_source)
            }) {
                let s = self.well_scroll.get(&t.block).copied().unwrap_or_default();
                cx0 -= s.x;
                cy0 -= s.y;
            }
            let step = lh * 0.25 * dir as Px;
            let mut y = if dir < 0 {
                cy0 - lh * 0.5
            } else {
                cy0 + ch + lh * 0.5
            };
            for _ in 0..32 {
                if y < 0.0 || y > vh {
                    break;
                }
                if let Some(c) = hit_test(
                    &frame.snapshot,
                    gear.paint_rev,
                    (cx0, y),
                    gear.shaper,
                    |id| well_scroll_xy(&self.well_scroll, id),
                ) && c != self.state.cursor
                {
                    self.place_cursor(c, vert.motion);
                    moved = true;
                    break;
                }
                y += step;
            }
        }

        if !moved {
            let cur = self.state.cursor;
            let next = if self.state.doc.in_table(cur.block) {
                let step = if dir < 0 {
                    TableStep::Above
                } else {
                    TableStep::Below
                };
                self.state.doc.table_step(cur, step).or_else(|| {
                    let edge = if dir < 0 {
                        TableStep::ExitBefore
                    } else {
                        TableStep::ExitAfter
                    };
                    self.state.doc.table_step(cur, edge)
                })
            } else {
                self.state.doc.sibling_leaf(cur.block, dir).map(|nb| {
                    let off = if dir < 0 {
                        self.state.doc.text(nb).map_or(0, |t| t.len())
                    } else {
                        0
                    };
                    Cursor {
                        block: nb,
                        offset: off,
                    }
                })
            };
            if let Some(c) = next {
                self.place_cursor(c, vert.motion);
                moved = true;
            }
        }
        moved
    }

    fn sync_table_chrome(
        &mut self,
        cells: &[md_render::snapshot::CellPiece],
        cx: &mut Context<'_, Self>,
    ) {
        let loc = self.state.doc.table_loc(self.state.cursor.block);
        let scroll = self.state.scroll;
        let next = loc.and_then(|loc| {
            table_union_rect(&self.state.doc, cells, loc.table).map(|(x, y, w)| {
                super::table_toolbar::TableChrome {
                    x,
                    y: y + scroll,
                    w,
                    loc,
                }
            })
        });
        let left = next.is_none();
        let chrome_changed = self.table_ui.chrome != next;
        let close_more = left && self.table_ui.more_open;
        let clear_hover = left && self.table_ui.align_hover.is_some();
        let close_picker = left && (self.table_ui.picker_open || self.table_ui.picker_drag);
        let left_table = loc.is_none();
        let clear_grip =
            left_table && (self.table_ui.grip_hover.is_some() || self.table_ui.reorder.is_some());
        if chrome_changed {
            self.table_ui.chrome = next;
        }
        if close_more {
            self.table_ui.more_open = false;
        }
        if clear_hover {
            self.table_ui.align_hover = None;
        }
        if close_picker {
            self.close_table_picker(true, cx);
        }
        if left_table {
            self.table_ui.grip_hover = None;
            self.table_ui.reorder = None;
            self.table_ui.toolbar_hover = false;
            self.table_ui.menu_op = None;
        }
        if chrome_changed || close_more || clear_hover || close_picker || clear_grip {
            cx.notify();
        }
    }
}

fn mark_stale(v: &mut EditorView, cx: &mut Context<'_, EditorView>) {
    v.stale_paint = true;
    cx.notify();
}

fn log_cold_trace(
    started: std::time::Instant,
    t_docmaps: Duration,
    t_font: Duration,
    t: BuildTimings,
    frame: &Frame,
    shaper: &GpuiShaper,
) -> ! {
    md_render::cold_trace::log(&format!(
        "cold first prepaint {:.1}ms docmaps={:.1}ms font={:.1}ms engine={:.1}ms assemble={:.1}ms geom={:.1}ms texts={} cells={} content_atoms={} shapes={} gpui={:.1}ms",
        started.elapsed().as_secs_f64() * 1000.0,
        t_docmaps.as_secs_f64() * 1000.0,
        t_font.as_secs_f64() * 1000.0,
        t.engine.as_secs_f64() * 1000.0,
        t.assemble.as_secs_f64() * 1000.0,
        t.geometry.as_secs_f64() * 1000.0,
        frame.texts.len(),
        frame.cells.len(),
        frame.content_atoms_painted,
        shaper.stats().total_shape_calls,
        md_render::cold_trace::gpui_ms(),
    ));
    std::process::exit(0);
}

fn table_union_rect(
    doc: &Doc,
    cells: &[md_render::snapshot::CellPiece],
    table: BlockId,
) -> Option<(Px, Px, Px)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut any = false;
    for c in cells {
        let Some(loc) = doc.table_loc(c.block) else {
            continue;
        };
        if loc.table != table {
            continue;
        }
        let (x, y, w, h) = c.rect_device;
        if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
            continue;
        }
        any = true;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
    }
    if any {
        Some((min_x, min_y, (max_x - min_x).max(0.0)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::Direction;
    use super::*;
    use crate::view::EditorView;
    use gpui::{Entity, TestAppContext, point, px, size};
    use md_core::document::{editor_options, load_markdown};

    fn editor_with_doc<'a>(
        markdown: &str,
        cx: &'a mut TestAppContext,
    ) -> (Entity<EditorView>, &'a mut gpui::VisualTestContext) {
        cx.add_window_view(|_, cx| {
            EditorView::new(
                Doc::new(load_markdown(markdown, editor_options())),
                DocumentTheme::one_dark(),
                cx,
            )
        })
    }

    #[gpui::test]
    fn a_missing_caret_device_still_walks_the_structure(cx: &mut TestAppContext) {
        let (editor, cx) = editor_with_doc("abc\n\nxyz\n", cx);

        let (a, b) = cx.update(|_, app| {
            let v = editor.read(app);
            let leaves = v.state.doc.text_leaves();
            (leaves[0], leaves[1])
        });
        cx.update(|_, app| {
            editor.update(app, |v, _| {
                v.place_cursor(
                    Cursor {
                        block: a,
                        offset: 3,
                    },
                    CursorMotion::Move,
                );
            });
        });

        let (_, prepaint) = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| crate::view::EditorElement {
                state: editor.clone(),
            },
        );
        let mut snap = (*prepaint.frame.snapshot).clone();
        snap.caret_device = None;
        let frame = Frame {
            snapshot: Rc::new(snap),
            assembly: prepaint.frame.assembly,
        };

        let moved = cx.update(|window, app| {
            let v = editor.read(app);
            let theme = v.state.theme;
            let media = ShapeMedia {
                mermaid_fitted: v.mermaid.fitted_snapshot(),
                math_metrics: v.math.metrics_snapshot(),
                math_gen: v.math.metrics_gen(),
                image_sizes: v.images.sizes_snapshot(),
                image_failed: v.images.failed_sources(),
                image_gen: v.images.sizes_gen(),
                link_dests: Rc::clone(&v.doc_maps.link_dests),
                link_raw: Rc::clone(&v.doc_maps.link_raw),
                block_image_dest: Rc::clone(&v.doc_maps.block_image_dest),
                block_code_lang: Rc::clone(&v.doc_maps.block_code_lang),
            };
            let scale = f64::from(window.scale_factor());
            let shape_cache = v.state.shape_cache.clone();
            let shaper = GpuiShaper::new(window, app, &theme, scale, shape_cache, media);
            let snap = SnapOperator::new(scale);
            let gear = FrameGear {
                env: BoxLayoutEnvironment::default(),
                theme: &theme,
                shaper: &shaper,
                snap: &snap,
                viewport: (800.0, 600.0),
                scale,
                cursor: Cursor {
                    block: a,
                    offset: 3,
                },
                selection: None,
                paint_rev: frame.snapshot.geometry_revision,
                caret_ly: None,
            };
            let kind = v.state.doc.kind(a).unwrap_or(BlockKind::Paragraph);
            let lh = shaper.row_advance_for(kind);
            editor.update(app, |v, _| {
                v.step_caret_vertically(
                    &frame,
                    gear,
                    PendingVertical {
                        dir: Direction::Next,
                        motion: CursorMotion::Move,
                    },
                    lh,
                    false,
                )
            })
        });
        assert!(
            moved,
            "the structural fallback must move the caret even when geometry is absent"
        );
        let (block, text) = cx.update(|_, app| {
            let v = editor.read(app);
            let block = v.state.cursor.block;
            (block, v.state.doc.text(block).unwrap_or("").to_string())
        });
        assert_eq!(block, b, "the fallback should land on the next leaf");
        assert_eq!(text, "xyz");
    }
}
