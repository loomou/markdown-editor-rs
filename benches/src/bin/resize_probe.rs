use std::sync::Arc;

use md_bench::mem;
use md_content::images;

const MIB: f64 = 1024.0 * 1024.0;

fn arg(index: usize, fallback: u32) -> u32 {
    std::env::args()
        .nth(index)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(fallback)
}

fn main() {
    let side = arg(1, 4096);
    let slot_w = f64::from(arg(2, 800));
    let slot_h = f64::from(arg(3, 600));

    let frame = image::Frame::new(image::RgbaImage::new(side, side));
    let source = Arc::new(gpui::RenderImage::new(vec![frame]));
    let source_bytes = source.as_bytes(0).map_or(0, |bytes| bytes.len());

    let key = images::display_key("giant.png", slot_w as f32, slot_h as f32, 1.0);
    let before = mem::heap().live;
    mem::reset_peak();
    let ready = images::raster_bitmap(&source, None, &key).expect("raster");
    let after = mem::heap();

    let predicted = f64::from(side) * f64::from(ready.px_h) * 16.0;
    println!(
        "source {side}x{side} = {:.1} MiB, slot {slot_w:.0}x{slot_h:.0}",
        source_bytes as f64 / MIB
    );
    println!(
        "raster {}x{} = {:.2} MiB",
        ready.px_w,
        ready.px_h,
        ready.bytes as f64 / MIB
    );
    println!("live before raster  = {:.1} MiB", before as f64 / MIB);
    println!(
        "transient peak      = {:.1} MiB",
        after.peak.saturating_sub(before) as f64 / MIB
    );
    println!("in_w * out_h * 16   = {:.1} MiB", predicted / MIB);
    println!("live after raster   = {:.1} MiB", after.live as f64 / MIB);
}
