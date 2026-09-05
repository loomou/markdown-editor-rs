use super::RasterSpec;

pub(super) fn strip_init(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find("%%{") {
        out.push_str(&rest[..i]);
        match rest[i..].find("}%%") {
            Some(end) => rest = &rest[i + end + 3..],
            None => {
                out.push_str(&rest[i..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

pub(super) fn first_viewbox(svg: &str) -> Option<(f32, f32, f32, f32)> {
    let (_, _, value) = root_viewbox(svg)?;
    let mut it = value.split_whitespace();
    let x: f32 = it.next()?.parse().ok()?;
    let y: f32 = it.next()?.parse().ok()?;
    let w: f32 = it.next()?.parse().ok()?;
    let h: f32 = it.next()?.parse().ok()?;
    if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
        Some((x, y, w, h))
    } else {
        None
    }
}

pub(super) fn pad_svg_viewbox(svg: &str, pad: f32) -> String {
    let Some((start, end, value)) = root_viewbox(svg) else {
        return svg.to_string();
    };
    let mut it = value.split_whitespace();
    let Some(x) = it.next().and_then(|v| v.parse::<f32>().ok()) else {
        return svg.to_string();
    };
    let Some(y) = it.next().and_then(|v| v.parse::<f32>().ok()) else {
        return svg.to_string();
    };
    let Some(w) = it.next().and_then(|v| v.parse::<f32>().ok()) else {
        return svg.to_string();
    };
    let Some(h) = it.next().and_then(|v| v.parse::<f32>().ok()) else {
        return svg.to_string();
    };
    let mut out = String::with_capacity(svg.len() + 32);
    out.push_str(&svg[..start]);
    out.push_str(&format!(
        "{} {} {} {}",
        x - pad,
        y - pad,
        w + pad * 2.0,
        h + pad * 2.0
    ));
    out.push_str(&svg[end..]);
    out
}

fn root_viewbox(svg: &str) -> Option<(usize, usize, &str)> {
    let (tag_start, _, tag) = root_svg_tag(svg)?;
    for quote in ['"', '\''] {
        let needle = format!("viewBox={quote}");
        if let Some(rel_start) = tag.find(&needle) {
            let start = tag_start + rel_start + needle.len();
            let end = start + svg[start..].find(quote)?;
            return Some((start, end, &svg[start..end]));
        }
    }
    None
}

fn root_svg_tag(svg: &str) -> Option<(usize, usize, &str)> {
    let mut search = 0usize;
    while let Some(rel) = svg[search..].find("<svg") {
        let tag_start = search + rel;
        let after_name = svg.as_bytes().get(tag_start + 4).copied()?;
        if !after_name.is_ascii_whitespace() && after_name != b'>' {
            search = tag_start + 4;
            continue;
        }
        let tag_end = tag_start + 4 + svg[tag_start + 4..].find('>')?;
        return Some((tag_start, tag_end, &svg[tag_start..tag_end]));
    }
    None
}

fn root_length(svg: &str, name: &str) -> Option<f32> {
    let (_, _, tag) = root_svg_tag(svg)?;
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        let Some(rel_start) = tag.find(&needle) else {
            continue;
        };
        let start = rel_start + needle.len();
        let end = start + tag[start..].find(quote)?;
        let value = tag[start..end].trim();
        let value = value.strip_suffix("px").unwrap_or(value);
        let value = value.parse::<f32>().ok()?;
        return value.is_finite().then_some(value.max(0.0));
    }
    None
}

const ZOOM_TARGET_CSS: f32 = 1600.0;
const ZOOM_EXTRA_MAX: f32 = 2.0;

pub const MAX_RASTER_PIXELS: f64 = 8.0 * 1024.0 * 1024.0;

struct RasterFit {
    contain: f32,
    raster_dpr: f32,
}

fn raster_fit(svg: &str, spec: &RasterSpec) -> RasterFit {
    let Some((_, _, w, h)) = first_viewbox(svg) else {
        let requested_dpr = spec.dpr.max(0.25);
        let raster_dpr = match (root_length(svg, "width"), root_length(svg, "height")) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                let max_dpr = (MAX_RASTER_PIXELS / (f64::from(w) * f64::from(h))).sqrt() as f32;
                requested_dpr.min(max_dpr).max(f32::MIN_POSITIVE)
            }

            _ => 0.25,
        };
        return RasterFit {
            contain: 1.0,
            raster_dpr,
        };
    };
    let contain = (spec.fit_w as f32 / w.max(1.0))
        .min(spec.fit_h as f32 / h.max(1.0))
        .clamp(f32::MIN_POSITIVE, 1.0);
    let css = (w * contain).max(h * contain).max(1.0);
    let extra = (ZOOM_TARGET_CSS / css).clamp(1.0, ZOOM_EXTRA_MAX);
    let requested_dpr = spec.dpr.max(0.25) * extra;
    let max_scale = (MAX_RASTER_PIXELS / (f64::from(w) * f64::from(h))).sqrt() as f32;
    let raster_dpr = requested_dpr
        .min(max_scale / contain)
        .max(f32::MIN_POSITIVE);
    RasterFit {
        contain,
        raster_dpr,
    }
}

pub(super) fn raster_scale(svg: &str, spec: &RasterSpec) -> f32 {
    let fit = raster_fit(svg, spec);
    fit.raster_dpr * fit.contain
}

pub(super) fn raster_dpr(svg: &str, spec: &RasterSpec) -> f32 {
    raster_fit(svg, spec).raster_dpr
}
