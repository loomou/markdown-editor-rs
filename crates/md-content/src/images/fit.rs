pub fn contain_fit(iw: f32, ih: f32, max_w: f32, max_h: f32) -> (f32, f32) {
    let max_w = max_w.max(1.0);
    let max_h = max_h.max(1.0);
    if iw <= 0.0 || ih <= 0.0 {
        return (max_w, max_h);
    }
    let scale = (max_w / iw).min(max_h / ih).min(1.0);
    ((iw * scale).max(1.0), (ih * scale).max(1.0))
}

pub(crate) fn width_fit(iw: f32, ih: f32, max_w: f32) -> (f32, f32) {
    let max_w = max_w.max(1.0);
    if iw <= 0.0 || ih <= 0.0 {
        return (max_w, max_w);
    }
    let scale = (max_w / iw).min(1.0);
    ((iw * scale).max(1.0), (ih * scale).max(1.0))
}
