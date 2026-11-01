use super::RasterSpec;
use super::svg::{MAX_RASTER_PIXELS, first_viewbox, root_length, strip_init};
use super::theme::{pipeline, presentation};
use crate::pixels::{GPUI_MAX_EDGE, ReadyImage, from_render_image, target_for_css};
use gpui::{DevicePixels, Size, SvgRenderer, SvgSize};
use merman::svg::{HeadlessRenderer, ResvgCompatibleSvg};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static RENDERER: RefCell<Option<(u64, HeadlessRenderer)>> = const { RefCell::new(None) };
}

pub fn raster(
    source: &str,
    spec: &RasterSpec,
    renderer: &SvgRenderer,
) -> Result<ReadyImage, crate::Error> {
    raster_with_svg(source, spec, renderer).map(|(image, _)| image)
}

pub fn raster_with_svg(
    source: &str,
    spec: &RasterSpec,
    renderer: &SvgRenderer,
) -> Result<(ReadyImage, Arc<ResvgCompatibleSvg>), crate::Error> {
    let sealed = render_sealed(source, spec)?;
    let image = raster_sealed(
        &sealed,
        spec.fit_w as f32,
        spec.fit_h as f32,
        spec.dpr,
        renderer,
    )?;
    Ok((image, Arc::new(sealed)))
}

pub fn raster_svg(
    sealed: &ResvgCompatibleSvg,
    css: (f32, f32),
    density: f32,
    renderer: &SvgRenderer,
) -> Result<ReadyImage, crate::Error> {
    raster_sealed(sealed, css.0, css.1, density, renderer)
}

fn raster_sealed(
    sealed: &ResvgCompatibleSvg,
    slot_w: f32,
    slot_h: f32,
    density: f32,
    renderer: &SvgRenderer,
) -> Result<ReadyImage, crate::Error> {
    let svg = sealed.as_str();
    let target = target_for(svg, slot_w, slot_h, density);
    let parsed = renderer
        .parse_svg(svg.as_bytes())
        .map_err(|e| crate::Error::Mermaid(e.to_string().into()))?;
    let size = Size::new(
        DevicePixels(target.px_w as i32),
        DevicePixels(target.px_h as i32),
    );
    let image = renderer
        .render_parsed(&parsed, SvgSize::ExactSize(size))
        .map_err(|e| crate::Error::Mermaid(e.to_string().into()))?;
    from_render_image(image, target.density)
        .ok_or_else(|| crate::Error::Mermaid(md_i18n::Key::DiagramEmptyPng.into()))
}

pub fn target_for(
    svg: &str,
    slot_w: f32,
    slot_h: f32,
    density: f32,
) -> crate::pixels::RasterTarget {
    target_for_css(
        fit_css(svg, slot_w, slot_h),
        density,
        MAX_RASTER_PIXELS as u64,
        GPUI_MAX_EDGE,
    )
}

fn fit_css(svg: &str, slot_w: f32, slot_h: f32) -> (f32, f32) {
    let intrinsic = first_viewbox(svg).map(|(_, _, w, h)| (w, h)).or_else(|| {
        match (root_length(svg, "width"), root_length(svg, "height")) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Some((w, h)),
            _ => None,
        }
    });
    let Some((w, h)) = intrinsic else {
        return (slot_w.max(1.0), slot_h.max(1.0));
    };
    let contain = (slot_w / w.max(1.0))
        .min(slot_h / h.max(1.0))
        .clamp(f32::MIN_POSITIVE, 1.0);
    ((w * contain).max(1.0), (h * contain).max(1.0))
}

pub fn render_sealed(source: &str, spec: &RasterSpec) -> Result<ResvgCompatibleSvg, crate::Error> {
    let src = strip_init(source);
    let theme = presentation(spec);
    let pipeline = pipeline();
    RENDERER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let stale = slot.as_ref().is_none_or(|(fp, _)| *fp != spec.theme_fp);
        if stale {
            *slot = Some((
                spec.theme_fp,
                HeadlessRenderer::new()
                    .with_presentation(theme)
                    .with_vendored_text_measurer(),
            ));
        }
        let renderer = &slot.as_ref().expect("renderer").1;
        match renderer.render_resvg_compatible_svg_with_pipeline_sync(&src, &pipeline) {
            Ok(Some(sealed)) if !sealed.as_str().is_empty() => Ok(sealed),
            Ok(_) => Err(crate::Error::Mermaid(md_i18n::Key::DiagramEmpty.into())),
            Err(e) => Err(crate::Error::Mermaid(e.to_string().into())),
        }
    })
}

pub fn render_svg(source: &str, spec: &RasterSpec) -> Result<String, crate::Error> {
    Ok(render_sealed(source, spec)?.into_string())
}
