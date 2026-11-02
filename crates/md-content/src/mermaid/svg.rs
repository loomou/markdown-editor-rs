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

pub(super) fn root_length(svg: &str, name: &str) -> Option<f32> {
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

pub const MAX_RASTER_PIXELS: f64 = 8.0 * 1024.0 * 1024.0;
