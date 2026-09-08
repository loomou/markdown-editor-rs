use base64::Engine;
use md_content::images;
use std::io::Cursor;
use std::io::Write;

fn svg_data(svg: &str) -> images::Resolved {
    images::resolve(
        &format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg.as_bytes())
        ),
        None,
    )
    .unwrap()
}

#[gpui::test]
fn svg_source_uses_bgra_channels(cx: &mut gpui::TestAppContext) {
    let resolved = svg_data(
        "<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'>\
         <rect width='2' height='2' fill='red'/></svg>",
    );
    let future = cx.update(|app| images::load_source(resolved, false, app));
    let (source, width, height) = futures::executor::block_on(future).unwrap();
    let pixel = &source.as_bytes(0).unwrap()[..4];
    println!("red SVG source={width}x{height}, first pixel={pixel:?}");
    assert_eq!(
        pixel,
        &[0, 0, 255, 255],
        "RenderImage and the display-layer raster consume BGRA; leaving SVG RGBA unswapped flips red and blue"
    );
}

#[gpui::test]
fn png_source_stays_bgra(cx: &mut gpui::TestAppContext) {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    let resolved = images::resolve(
        &format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buf)
        ),
        None,
    )
    .unwrap();
    let future = cx.update(|app| images::load_source(resolved, false, app));
    let (source, _, _) = futures::executor::block_on(future).unwrap();
    let pixel = &source.as_bytes(0).unwrap()[..4];
    assert_eq!(pixel, &[0, 0, 255, 255], "the PNG BGRA convention holds");
}

#[gpui::test]
fn svg_embedded_png_is_painted(cx: &mut gpui::TestAppContext) {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
    let resolved = svg_data(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' \
         xmlns:xlink='http://www.w3.org/1999/xlink' width='2' height='2'>\
         <image xlink:href='data:image/png;base64,{encoded}' width='2' height='2'/></svg>"
    ));
    let future = cx.update(|app| images::load_source(resolved, false, app));
    let (image, _, _) = futures::executor::block_on(future).unwrap();
    let pixel = &image.as_bytes(0).unwrap()[..4];
    println!("embedded PNG SVG pixel={pixel:?}");
    assert_eq!(
        pixel,
        &[56, 34, 12, 255],
        "an opaque embedded PNG must paint: RGBA [12,34,56] goes through svg_bgra to BGRA \
         [56,34,12] with alpha 255; fully transparent means the raster was dropped by the backend"
    );
}

#[gpui::test]
fn plain_svg_still_paints_vector_content(cx: &mut gpui::TestAppContext) {
    let future = cx.update(|app| {
        images::load_source(
            svg_data(
                "<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'>\
                 <rect width='2' height='2' fill='rgb(12,34,56)'/></svg>",
            ),
            false,
            app,
        )
    });
    let (image, _, _) = futures::executor::block_on(future).unwrap();
    let pixel = &image.as_bytes(0).unwrap()[..4];
    assert_eq!(
        pixel,
        &[56, 34, 12, 255],
        "the vector rectangle still paints as before"
    );
}

#[test]
fn usvg_default_resolver_does_read_local_files() {
    let path = marker_path();
    std::fs::write(&path, tiny_png_bytes()).unwrap();
    let source = svg_referencing(&path.to_string_lossy());
    let tree = usvg::Tree::from_data(&source, &usvg::Options::default()).unwrap();
    fn png_bytes(group: &usvg::Group) -> usize {
        group
            .children()
            .iter()
            .map(|node| match node {
                usvg::Node::Group(group) => png_bytes(group),
                usvg::Node::Image(image) => match image.kind() {
                    usvg::ImageKind::PNG(bytes) => bytes.len(),
                    _ => 0,
                },
                _ => 0,
            })
            .sum()
    }
    let read = png_bytes(tree.root());
    println!("usvg default resolver loaded {read} bytes from local marker");
    assert_eq!(
        read as u64,
        std::fs::metadata(&path).unwrap().len(),
        "usvg default options must keep reading local files (else the threat is gone upstream)"
    );
}

#[gpui::test]
fn svg_with_external_file_href_is_refused(cx: &mut gpui::TestAppContext) {
    let path = marker_path();
    std::fs::write(&path, tiny_png_bytes()).unwrap();
    let source = svg_referencing(&path.to_string_lossy());
    let future = cx.update(|app| images::load_source(data_url(&source), false, app));
    let outcome = futures::executor::block_on(future);
    assert!(
        outcome.is_err(),
        "an SVG referencing an external local file must be refused before any read"
    );
}

#[gpui::test]
fn plain_svg_still_loads(cx: &mut gpui::TestAppContext) {
    let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'>\
                   <rect width='4' height='4' fill='red'/></svg>";
    let future = cx.update(|app| images::load_source(data_url(source), false, app));
    let (image, _, _) = futures::executor::block_on(future)
        .expect("a plain SVG without external references must still load");
    assert!(image.size(0).width > gpui::DevicePixels(0));
}

#[gpui::test]
fn svg_with_embedded_data_image_still_loads(cx: &mut gpui::TestAppContext) {
    let png = base64::engine::general_purpose::STANDARD.encode(tiny_png_bytes());
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' \
         xmlns:xlink='http://www.w3.org/1999/xlink' width='2' height='2'>\
         <image xlink:href='data:image/png;base64,{png}' width='2' height='2'/></svg>"
    );
    let future = cx.update(|app| images::load_source(data_url(source.as_bytes()), false, app));
    let outcome = futures::executor::block_on(future);
    assert!(
        outcome.is_ok(),
        "an SVG with an embedded data-URL image must not be refused"
    );
}

#[gpui::test]
fn svgz_has_a_decompressed_input_budget(cx: &mut gpui::TestAppContext) {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'><!--{}-->\
         <rect width='2' height='2'/></svg>",
        " ".repeat(17 * 1024 * 1024)
    );
    let mut compressor = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    compressor.write_all(svg.as_bytes()).unwrap();
    let zipped = compressor.finish().unwrap();
    println!(
        "SVGZ bounded sample: encoded={} decompressed={} bytes",
        zipped.len(),
        svg.len()
    );
    let input = format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&zipped)
    );
    let future =
        cx.update(|app| images::load_source(images::resolve(&input, None).unwrap(), false, app));
    let result = futures::executor::block_on(future);
    println!(
        "SVGZ expanded beyond the decompressed budget accepted={}",
        result.is_ok()
    );
    assert!(
        result.is_err(),
        "an SVGZ whose decompressed size exceeds its own budget must be refused before usvg allocates, not swallowed"
    );
}

#[gpui::test]
fn small_svgz_still_loads(cx: &mut gpui::TestAppContext) {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'>\
               <rect width='2' height='2' fill='rgb(12,34,56)'/></svg>";
    let mut compressor = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    compressor.write_all(svg.as_bytes()).unwrap();
    let zipped = compressor.finish().unwrap();
    let input = format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&zipped)
    );
    let future =
        cx.update(|app| images::load_source(images::resolve(&input, None).unwrap(), false, app));
    let (image, _, _) = futures::executor::block_on(future)
        .expect("a small SVGZ must still load through the bounded decompress path");
    let pixel = &image.as_bytes(0).unwrap()[..4];
    assert_eq!(
        pixel,
        &[56, 34, 12, 255],
        "a small SVGZ still renders its content (BGRA)"
    );
}

fn marker_path() -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("external-svg-marker.png")
}

fn tiny_png_bytes() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 255]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("png");
    buf
}

fn svg_referencing(path: &str) -> Vec<u8> {
    let href = path.replace('\\', "/");
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' \
         xmlns:xlink='http://www.w3.org/1999/xlink' width='2' height='2'>\
         <image xlink:href='{href}' width='2' height='2'/></svg>"
    )
    .into_bytes()
}

fn data_url(bytes: &[u8]) -> images::Resolved {
    let svg = format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    images::resolve(&svg, None).unwrap()
}
