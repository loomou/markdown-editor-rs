use super::RasterSpec;
use super::svg::{first_viewbox, raster_dpr, raster_scale, strip_init};
use super::theme::{pipeline, presentation};
use crate::pixels::{ReadyImage, decode_png};
use merman::svg::export::{RasterOptions, svg_to_png};
use merman::svg::{HeadlessRenderer, ResvgCompatibleSvg};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static RENDERER: RefCell<Option<(u64, HeadlessRenderer)>> = const { RefCell::new(None) };
}

pub fn raster(source: &str, spec: &RasterSpec) -> Result<ReadyImage, crate::Error> {
    raster_with_svg(source, spec).map(|(image, _)| image)
}

pub fn raster_with_svg(
    source: &str,
    spec: &RasterSpec,
) -> Result<(ReadyImage, Arc<ResvgCompatibleSvg>), crate::Error> {
    let sealed = render_sealed(source, spec)?;
    let png = encode_png(&sealed, spec)?;
    let dpr = raster_dpr(sealed.as_str(), spec);
    let image = decode_png(&png, dpr)
        .ok_or_else(|| crate::Error::Mermaid(md_i18n::Key::DiagramBadPng.into()))?;
    Ok((image, Arc::new(sealed)))
}

pub fn raster_svg(
    sealed: &ResvgCompatibleSvg,
    css: (f32, f32),
    density: f32,
) -> Result<ReadyImage, crate::Error> {
    let contain = first_viewbox(sealed.as_str())
        .map(|(_, _, vw, vh)| (css.0 / vw.max(1.0)).min(css.1 / vh.max(1.0)))
        .unwrap_or(1.0);
    let png = svg_to_png(
        sealed,
        &RasterOptions::default().with_scale(density * contain),
    )
    .map_err(|e| crate::Error::Mermaid(e.to_string().into()))?;
    match &png[..] {
        [] => Err(crate::Error::Mermaid(md_i18n::Key::DiagramEmptyPng.into())),
        _ => decode_png(&png, density)
            .ok_or_else(|| crate::Error::Mermaid(md_i18n::Key::DiagramBadPng.into())),
    }
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

#[cfg(test)]
pub(super) fn render_png(source: &str, spec: &RasterSpec) -> Result<Vec<u8>, crate::Error> {
    let sealed = render_sealed(source, spec)?;
    encode_png(&sealed, spec)
}

fn encode_png(sealed: &ResvgCompatibleSvg, spec: &RasterSpec) -> Result<Vec<u8>, crate::Error> {
    let opts = RasterOptions::default().with_scale(raster_scale(sealed.as_str(), spec));
    match svg_to_png(sealed, &opts) {
        Ok(bytes) if !bytes.is_empty() => Ok(bytes),
        Ok(_) => Err(crate::Error::Mermaid(md_i18n::Key::DiagramEmpty.into())),
        Err(e) => Err(crate::Error::Mermaid(e.to_string().into())),
    }
}
