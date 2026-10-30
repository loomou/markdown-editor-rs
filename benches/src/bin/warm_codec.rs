use image::imageops::FilterType;
use md_content::media::{WarmCodec, decode_raw, encode_raw};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus/media");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("corpus/media")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect();
    files.sort();

    for width in [900u32, 1600u32] {
        println!("\n== display raster, {} px wide ==", width);
        println!(
            "{:<12} {:>10} {:>8} {:>12} {:>8} {:>9} {:>12} {:>8} {:>9}",
            "file", "px", "raw", "deflate", "ratio", "enc ms", "png", "ratio", "enc ms"
        );
        for path in &files {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let Some(row) = measure(path, width) else {
                println!("{:<12} decode failed", name);
                continue;
            };
            println!(
                "{:<12} {:>10} {:>8} {:>12} {:>7.2}x {:>9.1} {:>12} {:>7.2}x {:>9.1}",
                name,
                format!("{}x{}", row.px_w, row.px_h),
                kib(row.raw),
                kib(row.deflate),
                row.raw as f64 / row.deflate.max(1) as f64,
                row.deflate_ms,
                kib(row.png),
                row.raw as f64 / row.png.max(1) as f64,
                row.png_ms,
            );
        }
    }
}

struct Row {
    px_w: u32,
    px_h: u32,
    raw: usize,
    deflate: usize,
    deflate_ms: f64,
    png: usize,
    png_ms: f64,
}

fn measure(path: &PathBuf, width: u32) -> Option<Row> {
    let img = image::open(path).ok()?;
    let img = img.into_rgba8();
    let (w, h) = img.dimensions();
    let scale = (width as f32 / w as f32).min(1.0);
    let (tw, th) = (
        ((w as f32 * scale) as u32).max(1),
        ((h as f32 * scale) as u32).max(1),
    );
    let img = if tw == w && th == h {
        img
    } else {
        image::imageops::resize(&img, tw, th, FilterType::Triangle)
    };
    let mut raw = img.into_raw();
    for px in raw.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let (px_w, px_h) = (tw, th);

    let start = Instant::now();
    let deflate = encode_raw(&raw, px_w, px_h, WarmCodec::Deflate)?;
    let deflate_ms = start.elapsed().as_secs_f64() * 1000.0;

    let start = Instant::now();
    let png = encode_raw(&raw, px_w, px_h, WarmCodec::Png)?;
    let png_ms = start.elapsed().as_secs_f64() * 1000.0;

    let start = Instant::now();
    let restored = decode_raw(&deflate, px_w, px_h, WarmCodec::Deflate)?;
    let thaw_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(restored, raw, "deflate round trip");
    let start = Instant::now();
    let restored = decode_raw(&png, px_w, px_h, WarmCodec::Png)?;
    let png_thaw_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(restored, raw, "png round trip");

    println!(
        "{:<12} thaw: deflate {:.1} ms  png {:.1} ms",
        "",
        thaw_ms,
        png_thaw_ms
    );

    Some(Row {
        px_w,
        px_h,
        raw: raw.len(),
        deflate: deflate.len(),
        deflate_ms,
        png: png.len(),
        png_ms,
    })
}

fn kib(bytes: usize) -> String {
    format!("{:.1}K", bytes as f64 / 1024.0)
}
