use super::EditorView;
use super::draw::well_view_h;
use crate::ui::icons;
use crate::ui::theme::{RADIUS, ShellTheme};
use gpui::{
    App, Bounds, ClickEvent, Context, Corners, Div, Entity, InteractiveElement, MouseButton,
    ParentElement, Stateful, StatefulInteractiveElement, Styled, Window, div, hsla, point, px,
    size, svg,
};
use md_content::gpui_theme::ThemeColorExt;
use md_content::mermaid;
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_render::snapshot::TextPiece;
use md_theme::DocumentTheme;
use std::sync::Arc;

const BTN: f32 = 24.0;
const BTN_PAD: f32 = 6.0;
const FIT_PAD: f32 = 32.0;
const SCALE_MIN: f32 = 0.25;
const SCALE_MAX: f32 = 16.0;

const ZOOM_RASTER_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(120);

const DENSITY_EPS: f32 = 0.05;

pub(crate) struct ZoomRaster {
    pub key: mermaid::MermaidSvgKey,
    pub density: f32,
    pub image: md_content::mermaid::ReadyImage,
}

pub(crate) struct ZoomRasterJob {
    pub key: mermaid::MermaidSvgKey,
    pub density: f32,

    #[expect(dead_code)]
    pub task: gpui::Task<()>,
}

pub(crate) fn zoom_target_density(
    css: (f32, f32),
    viewport: (Px, Px),
    user_scale: f32,
    window_dpr: f32,
    doc_density: f32,
) -> Option<f32> {
    let aw = (viewport.0 as f32 - 2.0 * FIT_PAD).max(1.0);
    let ah = (viewport.1 as f32 - 2.0 * FIT_PAD).max(1.0);
    let fit = (aw / css.0.max(1.0)).min(ah / css.1.max(1.0));
    let need = (fit * user_scale * window_dpr).max(0.25);
    if need <= doc_density * (1.0 + DENSITY_EPS) {
        return None;
    }
    let budget = (mermaid::MAX_RASTER_PIXELS as f32 / (css.0 * css.1).max(1.0)).sqrt();
    Some(need.min(budget).max(0.25))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MediaChrome {
    pub block: BlockId,
    pub kind: BlockKind,
    pub x: Px,

    pub y: Px,
    pub w: Px,
    pub h: Px,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaHit {
    pub id: BlockId,
    pub kind: BlockKind,
    pub x: Px,
    pub y: Px,
    pub w: Px,
    pub h: Px,
}

pub(crate) struct MediaZoom {
    pub block: BlockId,
    pub kind: BlockKind,
    pub slot_w: Px,

    pub user_scale: f32,
    pub pan: (f32, f32),
    drag: Option<(f32, f32)>,
}

impl MediaHit {
    fn contains(self, pos: (Px, Px)) -> bool {
        pos.0 >= self.x && pos.0 < self.x + self.w && pos.1 >= self.y && pos.1 < self.y + self.h
    }
}

pub(crate) fn collect_hits(texts: &[TextPiece], editing: Option<BlockId>) -> Vec<MediaHit> {
    texts
        .iter()
        .filter(|t| {
            !t.edit_source
                && matches!(t.kind, BlockKind::Image | BlockKind::Mermaid)
                && editing != Some(t.block)
        })
        .map(|t| {
            let (x, y) = t.content_origin_device;
            MediaHit {
                id: t.block,
                kind: t.kind,
                x,
                y,
                w: t.content_width,
                h: well_view_h(t),
            }
        })
        .collect()
}

fn chrome_from_hit(h: MediaHit, scroll: Px) -> MediaChrome {
    MediaChrome {
        block: h.id,
        kind: h.kind,
        x: h.x,
        y: h.y + scroll,
        w: h.w,
        h: h.h,
    }
}

fn button_rect(chrome: MediaChrome, scroll: Px) -> (Px, Px, Px, Px) {
    let w = BTN as Px;
    let h = BTN as Px;
    let x = chrome.x + chrome.w - BTN_PAD as Px - w;
    let y = chrome.y - scroll + BTN_PAD as Px;
    (x, y, w, h)
}

pub(crate) fn chrome_contains(chrome: MediaChrome, scroll: Px, pos: (Px, Px)) -> bool {
    let (x, y, w, h) = button_rect(chrome, scroll);
    pos.0 >= x && pos.0 < x + w && pos.1 >= y && pos.1 < y + h
}

fn image_rect(
    viewport: (Px, Px),
    css: (f32, f32),
    user_scale: f32,
    pan: (f32, f32),
) -> (f32, f32, f32, f32) {
    let aw = (viewport.0 as f32 - 2.0 * FIT_PAD).max(1.0);
    let ah = (viewport.1 as f32 - 2.0 * FIT_PAD).max(1.0);
    let cw = css.0.max(1.0);
    let ch = css.1.max(1.0);
    let fit = (aw / cw).min(ah / ch);
    let s = (fit * user_scale).max(0.01);
    let w = cw * s;
    let h = ch * s;
    let x = (viewport.0 as f32 - w) * 0.5 + pan.0;
    let y = (viewport.1 as f32 - h) * 0.5 + pan.1;
    (x, y, w, h)
}

fn hit_image(
    viewport: (Px, Px),
    css: (f32, f32),
    user_scale: f32,
    pan: (f32, f32),
    pos: (Px, Px),
) -> bool {
    let (x, y, w, h) = image_rect(viewport, css, user_scale, pan);
    let px = pos.0 as f32;
    let py = pos.1 as f32;
    px >= x && px < x + w && py >= y && py < y + h
}

impl EditorView {
    pub(crate) fn sync_media_chrome(&mut self, texts: &[TextPiece], cx: &mut Context<'_, Self>) {
        if self.media_zoom.is_some() {
            if self.media_chrome.is_some() {
                self.media_chrome = None;
                self.media_hover = None;
                self.media_chrome_hover = false;
                cx.notify();
            }
            return;
        }
        let editing = self.state.doc.block_edit();
        let keep = self.media_hover.filter(|&id| editing != Some(id));
        let next = keep.and_then(|id| {
            collect_hits(texts, editing)
                .into_iter()
                .find(|h| h.id == id)
                .map(|h| chrome_from_hit(h, self.state.scroll))
        });
        if self.media_chrome != next {
            self.media_chrome = next;
            if next.is_none() {
                self.media_hover = None;
                self.media_chrome_hover = false;
            }
            cx.notify();
        }
    }

    pub(crate) fn hover_media(
        &mut self,
        hits: &[MediaHit],
        pos: (Px, Px),
        cx: &mut Context<'_, Self>,
    ) {
        if self.media_zoom.is_some() {
            return;
        }
        if let Some(chrome) = self.media_chrome
            && chrome_contains(chrome, self.state.scroll, pos)
        {
            return;
        }
        let hit = hits.iter().copied().find(|h| h.contains(pos));
        let next = hit.map(|h| h.id);
        if self.media_hover == next && (next.is_some() || self.media_chrome_hover) {
            return;
        }
        if next.is_none() && self.media_chrome_hover {
            return;
        }
        self.media_hover = next;
        if next.is_none() {
            self.media_chrome = None;
        } else if let Some(h) = hit {
            self.media_chrome = Some(chrome_from_hit(h, self.state.scroll));
        }
        cx.notify();
    }

    pub(crate) fn set_media_chrome_hover(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        if self.media_chrome_hover == on {
            return;
        }
        self.media_chrome_hover = on;
        if !on && self.media_hover.is_none() {
            self.media_chrome = None;
        }
        cx.notify();
    }

    pub(crate) fn open_media_zoom(&mut self, chrome: MediaChrome, cx: &mut Context<'_, Self>) {
        self.media_zoom = Some(MediaZoom {
            block: chrome.block,
            kind: chrome.kind,
            slot_w: chrome.w,
            user_scale: 1.0,
            pan: (0.0, 0.0),
            drag: None,
        });
        self.media_chrome = None;
        self.media_hover = None;
        self.media_chrome_hover = false;
        self.enter_block_edit_on_click = false;
        self.pending_click = None;
        self.dragging = false;
        self.drag_pointer = None;
        cx.notify();
    }

    pub(crate) fn close_media_zoom(&mut self, cx: &mut Context<'_, Self>) {
        if self.media_zoom.is_none() {
            return;
        }
        self.media_zoom = None;
        self.drop_zoom_raster(cx);
        cx.notify();
    }

    pub(crate) fn drop_zoom_raster(&mut self, cx: &mut Context<'_, Self>) {
        self.zoom_raster_job = None;
        if let Some(zr) = self.zoom_raster.take() {
            cx.drop_image(zr.image.image, None);
            cx.notify();
        }
    }

    pub(crate) fn schedule_zoom_raster(
        &mut self,
        viewport: (Px, Px),
        scale: f64,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(z) = self.media_zoom.as_ref() else {
            return;
        };
        if z.kind != BlockKind::Mermaid {
            return;
        }
        let theme = self.state.theme;
        let theme_fp = mermaid::theme_fingerprint(&theme);
        let Some(key) = mermaid::key_for(
            &self.state.doc.document,
            z.block,
            z.slot_w,
            theme.decoration.mermaid_max_width,
            theme.decoration.mermaid_max_height,
            scale,
            theme_fp,
        ) else {
            return;
        };
        let Some(doc) = self.mermaid.image(&key) else {
            return;
        };
        let css = doc.css_size();
        let Some(target) =
            zoom_target_density(css, viewport, z.user_scale, scale.max(0.25) as f32, doc.dpr)
        else {
            self.drop_zoom_raster(cx);
            return;
        };
        let svg_key = key.svg_key();
        if let Some(zr) = &self.zoom_raster
            && zr.key == svg_key
            && zr.density >= target - DENSITY_EPS
        {
            return;
        }
        if let Some(job) = &self.zoom_raster_job
            && job.key == svg_key
            && job.density >= target - DENSITY_EPS
        {
            return;
        }
        let svg = self.mermaid.svg(&svg_key);
        let Some(src) = self.state.doc.text(z.block).map(|s| s.to_string()) else {
            return;
        };
        let spec = mermaid::spec_from_theme(&theme, key);
        self.zoom_raster_job = Some(ZoomRasterJob {
            key: svg_key,
            density: target,
            task: cx.spawn(async move |this, cx| {
                cx.background_executor().timer(ZOOM_RASTER_DEBOUNCE).await;
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let svg = match svg {
                            Some(svg) => svg,
                            None => std::sync::Arc::new(mermaid::render_sealed(&src, &spec)?),
                        };
                        mermaid::raster_svg(&svg, css, target)
                    })
                    .await;
                let _ = this.update(cx, |v, cx| {
                    let same = v.media_zoom.as_ref().is_some_and(|z| {
                        mermaid::key_for(
                            &v.state.doc.document,
                            z.block,
                            z.slot_w,
                            v.state.theme.decoration.mermaid_max_width,
                            v.state.theme.decoration.mermaid_max_height,
                            scale,
                            mermaid::theme_fingerprint(&v.state.theme),
                        )
                        .is_some_and(|k| k.svg_key() == svg_key)
                    });
                    if !same {
                        return;
                    }
                    match result {
                        Ok(image) => {
                            if let Some(old) = v.zoom_raster.replace(ZoomRaster {
                                key: svg_key,
                                density: target,
                                image,
                            }) {
                                cx.drop_image(old.image.image, None);
                            }
                            cx.notify();
                        }
                        Err(err) => tracing::warn!(error = %err, "mermaid zoom raster"),
                    }
                });
            }),
        });
    }

    pub(crate) fn media_zoom_press(
        &mut self,
        local: (Px, Px),
        viewport: (Px, Px),
        scale: f64,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(z) = self.media_zoom.as_ref() else {
            return;
        };
        let Some(css) = self.zoom_css_size(z, scale) else {
            return;
        };
        if hit_image(viewport, css, z.user_scale, z.pan, local) {
            if let Some(z) = self.media_zoom.as_mut() {
                z.drag = Some((local.0 as f32, local.1 as f32));
            }
        } else {
            self.close_media_zoom(cx);
        }
    }

    pub(crate) fn media_zoom_move(&mut self, local: (Px, Px), cx: &mut Context<'_, Self>) {
        let Some(z) = self.media_zoom.as_mut() else {
            return;
        };
        let Some((px, py)) = z.drag else {
            return;
        };
        let nx = local.0 as f32;
        let ny = local.1 as f32;
        z.pan.0 += nx - px;
        z.pan.1 += ny - py;
        z.drag = Some((nx, ny));
        cx.notify();
    }

    pub(crate) fn media_zoom_release(&mut self) {
        if let Some(z) = self.media_zoom.as_mut() {
            z.drag = None;
        }
    }

    pub(crate) fn media_zoom_wheel(
        &mut self,
        local: (Px, Px),
        dy: Px,
        viewport: (Px, Px),
        scale: f64,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(z) = self.media_zoom.as_ref() else {
            return;
        };
        let Some(css) = self.zoom_css_size(z, scale) else {
            return;
        };
        let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
        let old = image_rect(viewport, css, z.user_scale, z.pan);
        let next = (z.user_scale * factor).clamp(SCALE_MIN, SCALE_MAX);
        if (next - z.user_scale).abs() < f32::EPSILON {
            return;
        }
        let new = image_rect(viewport, css, next, z.pan);
        let fx = if old.2.abs() > 1.0 {
            (local.0 as f32 - old.0) / old.2
        } else {
            0.5
        };
        let fy = if old.3.abs() > 1.0 {
            (local.1 as f32 - old.1) / old.3
        } else {
            0.5
        };
        if let Some(z) = self.media_zoom.as_mut() {
            z.pan.0 += local.0 as f32 - (new.0 + fx * new.2);
            z.pan.1 += local.1 as f32 - (new.1 + fy * new.3);
            z.user_scale = next;
        }
        cx.notify();
    }

    fn zoom_css_size(&self, z: &MediaZoom, scale: f64) -> Option<(f32, f32)> {
        match z.kind {
            BlockKind::Mermaid => {
                let theme = &self.state.theme;
                let key = mermaid::key_for(
                    &self.state.doc.document,
                    z.block,
                    z.slot_w,
                    theme.decoration.mermaid_max_width,
                    theme.decoration.mermaid_max_height,
                    scale,
                    mermaid::theme_fingerprint(theme),
                )?;
                self.mermaid.image(&key).map(|ready| ready.css_size())
            }
            BlockKind::Image => {
                let dest = self.doc_maps.block_image_dest.get(&z.block)?;
                if let Some(img) = self.images.source_image(dest) {
                    let sz = img.size(0);
                    let dpr = scale.max(0.25) as f32;
                    let w = (sz.width.0.max(0) as f32 / dpr).max(1.0);
                    let h = (sz.height.0.max(0) as f32 / dpr).max(1.0);
                    Some((w, h))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn zoom_paint_image(
        &self,
        z: &MediaZoom,
        scale: f64,
    ) -> Option<(Arc<gpui::RenderImage>, (f32, f32))> {
        match z.kind {
            BlockKind::Mermaid => {
                let theme = &self.state.theme;
                let key = mermaid::key_for(
                    &self.state.doc.document,
                    z.block,
                    z.slot_w,
                    theme.decoration.mermaid_max_width,
                    theme.decoration.mermaid_max_height,
                    scale,
                    mermaid::theme_fingerprint(theme),
                )?;

                if let Some(zr) = &self.zoom_raster
                    && zr.key == key.svg_key()
                {
                    return Some((zr.image.image.clone(), zr.image.css_size()));
                }
                let ready = self.mermaid.image(&key)?;
                Some((ready.image.clone(), ready.css_size()))
            }
            BlockKind::Image => {
                let dest = self.doc_maps.block_image_dest.get(&z.block)?;
                let img = self.images.source_image(dest)?;
                let sz = img.size(0);
                let dpr = scale.max(0.25) as f32;
                let w = (sz.width.0.max(0) as f32 / dpr).max(1.0);
                let h = (sz.height.0.max(0) as f32 / dpr).max(1.0);
                Some((img, (w, h)))
            }
            _ => None,
        }
    }
}

pub(crate) fn chrome_overlay(
    chrome: MediaChrome,
    scroll: Px,
    theme: DocumentTheme,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    let t = ShellTheme::from_app(&theme.app);
    let (x, y, w, h) = button_rect(chrome, scroll);
    div()
        .id("media-zoom-btn")
        .absolute()
        .left(px(x as f32))
        .top(px(y as f32))
        .w(px(w as f32))
        .h(px(h as f32))
        .rounded(px(RADIUS))
        .bg(t.panel_bg)
        .border_1()
        .border_color(t.border)
        .flex()
        .items_center()
        .justify_center()
        .text_color(t.text_muted)
        .hover(move |s| s.bg(t.hover).text_color(t.text))
        .on_hover({
            let editor = editor.clone();
            move |hovered, _, cx| {
                editor.update(cx, |v, cx| v.set_media_chrome_hover(*hovered, cx));
            }
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click({
            let editor = editor;
            move |_: &ClickEvent, _, cx| {
                cx.stop_propagation();
                editor.update(cx, |v, cx| v.open_media_zoom(chrome, cx));
            }
        })
        .child(
            svg()
                .size(px(12.))
                .path(icons::SEARCH)
                .text_color(t.text_muted),
        )
}

pub(crate) fn close_overlay(theme: DocumentTheme, editor: Entity<EditorView>) -> Stateful<Div> {
    let t = ShellTheme::from_app(&theme.app);
    div()
        .id("media-zoom-close")
        .absolute()
        .top(px(12.))
        .right(px(12.))
        .w(px(BTN))
        .h(px(BTN))
        .rounded(px(RADIUS))
        .bg(t.panel_bg)
        .border_1()
        .border_color(t.border)
        .flex()
        .items_center()
        .justify_center()
        .hover(move |s| s.bg(t.hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click({
            let editor = editor;
            move |_: &ClickEvent, _, cx| {
                cx.stop_propagation();
                editor.update(cx, |v, cx| v.close_media_zoom(cx));
            }
        })
        .child(
            svg()
                .size(px(10.))
                .path(icons::FIND_CLOSE)
                .text_color(t.text),
        )
}

pub(crate) fn paint_zoom(
    window: &mut Window,
    cx: &mut App,
    editor: &Entity<EditorView>,
    origin: (f32, f32),
    viewport: (Px, Px),
    scale: f64,
) {
    let v = editor.read(cx);
    let Some(z) = v.media_zoom.as_ref() else {
        return;
    };
    let dim = hsla(0.0, 0.0, 0.0, 0.55);
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(px(origin.0), px(origin.1)),
            size: size(px(viewport.0 as f32), px(viewport.1 as f32)),
        },
        dim,
    ));
    let Some((image, css)) = v.zoom_paint_image(z, scale) else {
        return;
    };
    let (x, y, w, h) = image_rect(viewport, css, z.user_scale, z.pan);
    let bounds = Bounds {
        origin: point(px(origin.0 + x), px(origin.1 + y)),
        size: size(px(w.max(1.0)), px(h.max(1.0))),
    };

    window.paint_quad(gpui::fill(bounds, v.state.theme.paint.canvas.hsla()));
    let _ = window.paint_image(bounds, Corners::all(px(0.0)), image, 0, false);
}

#[cfg(test)]
mod tests {
    use super::{
        BlockKind, DENSITY_EPS, FIT_PAD, MediaChrome, button_rect, image_rect, zoom_target_density,
    };

    #[test]
    fn contain_fit_keeps_the_picture_inside_the_viewport() {
        let (x, y, w, h) = image_rect((800.0, 600.0), (400.0, 200.0), 1.0, (0.0, 0.0));
        assert!(w <= 800.0 - 2.0 * FIT_PAD + 0.5);
        assert!(h <= 600.0 - 2.0 * FIT_PAD + 0.5);
        assert!((w / h - 2.0).abs() < 0.01, "aspect {w}x{h}");
        assert!((x - (800.0 - w) * 0.5).abs() < 0.5);
        assert!((y - (600.0 - h) * 0.5).abs() < 0.5);
    }

    #[test]
    fn pan_offsets_the_fitted_rect() {
        let (x0, y0, w, h) = image_rect((800.0, 600.0), (100.0, 100.0), 1.0, (0.0, 0.0));
        let (x, y, w2, h2) = image_rect((800.0, 600.0), (100.0, 100.0), 1.0, (40.0, -10.0));
        assert!((w2 - w).abs() < 0.01);
        assert!((h2 - h).abs() < 0.01);
        assert!((x - x0 - 40.0).abs() < 0.01);
        assert!((y - y0 + 10.0).abs() < 0.01);
    }

    #[test]
    fn button_screen_y_tracks_scroll_without_waiting_for_new_geometry() {
        let chrome = MediaChrome {
            block: 0,
            kind: BlockKind::Image,
            x: 100.0,
            y: 400.0,
            w: 200.0,
            h: 80.0,
        };
        let (_, y0, _, _) = button_rect(chrome, 0.0);
        let (_, y1, _, _) = button_rect(chrome, 40.0);
        assert!((y0 - y1 - 40.0).abs() < 0.01);
    }

    #[test]
    fn zoom_density_skips_when_the_flow_bitmap_is_dense_enough() {
        let d = zoom_target_density((800.0, 600.0), (1600.0, 1200.0), 1.0, 2.0, 4.0);
        assert!(d.is_none(), "a 4x document flow already covers it: {d:?}");
        let d = zoom_target_density((800.0, 600.0), (1600.0, 1200.0), 1.0, 2.0, 2.0)
            .expect("2x is not enough, so a raster must be scheduled");
        assert!(d > 3.5 && d < 4.1, "fit*dpr is about 3.79, got {d}");
    }

    #[test]
    fn zoom_density_caps_at_the_pixel_budget() {
        let d = zoom_target_density((4000.0, 3000.0), (16000.0, 12000.0), 2.0, 2.0, 1.0)
            .expect("over budget must still get a capped value");
        let budget = (md_content::mermaid::MAX_RASTER_PIXELS as f32 / (4000.0 * 3000.0)).sqrt();
        assert!((d - budget).abs() < DENSITY_EPS, "got {d}, budget {budget}");
    }

    #[gpui::test]
    fn zoom_overlay_rasterizes_a_dense_bitmap_for_mermaid(cx: &mut gpui::TestAppContext) {
        use super::{EditorView, ZOOM_RASTER_DEBOUNCE};
        let doc = md_core::doc::Doc::new(md_core::document::load_markdown(
            "# Heading\n\n```mermaid\nflowchart TD\n  A0-->B0\n  B0-->C0\n```\n",
            md_core::document::editor_options(),
        ));
        let (editor, cx) = cx
            .add_window_view(|_, cx| EditorView::new(doc, md_theme::DocumentTheme::one_dark(), cx));
        cx.run_until_parked();

        let block = editor.update(cx, |v, _| {
            v.state
                .doc
                .text_leaves()
                .into_iter()
                .find(|&id| v.state.doc.kind(id) == Some(BlockKind::Mermaid))
                .expect("mermaid block")
        });
        editor.update(cx, |v, cx| {
            v.open_media_zoom(
                MediaChrome {
                    block,
                    kind: BlockKind::Mermaid,
                    x: 0.0,
                    y: 0.0,
                    w: 400.0,
                    h: 300.0,
                },
                cx,
            );
        });
        editor.update(cx, |v, cx| {
            v.media_zoom.as_mut().expect("overlay").user_scale = 8.0;
            cx.notify();
        });
        cx.run_until_parked();
        editor.update(cx, |v, _| {
            assert!(
                v.zoom_raster_job.is_some(),
                "the 8x target density outruns the document flow, so a raster job must be scheduled"
            );
        });

        cx.executor().advance_clock(ZOOM_RASTER_DEBOUNCE);
        cx.run_until_parked();
        let density = editor.update(cx, |v, _| {
            let zr = v
                .zoom_raster
                .as_ref()
                .expect("after the debounce there must be an overlay bitmap");
            zr.density
        });

        assert!(
            density > 4.0,
            "the overlay density {density} must outrun the 4x document flow"
        );

        editor.update(cx, |v, cx| v.close_media_zoom(cx));
        cx.run_until_parked();
        editor.update(cx, |v, _| {
            assert!(v.zoom_raster.is_none() && v.zoom_raster_job.is_none());
        });
    }
}
