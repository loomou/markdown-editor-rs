use ratex_layout::engine::layout;
use ratex_layout::layout_options::LayoutOptions;
use ratex_layout::to_display::to_display_list;
use ratex_parser::parse;
use ratex_types::display_item::DisplayList;
use std::sync::Arc;

fn display_list(latex: &str) -> DisplayList {
    let nodes = parse(latex).expect("latex parses");
    let root = layout(&nodes, &LayoutOptions::default());
    to_display_list(&root)
}

fn options(font_size: f64) -> ratex_svg::SvgOptions {
    ratex_svg::SvgOptions {
        font_size,
        padding: 0.0,
        embed_glyphs: true,
        ..ratex_svg::SvgOptions::default()
    }
}

fn renderer() -> gpui::SvgRenderer {
    gpui::SvgRenderer::new(Arc::new(()))
}

fn ink_bounds(bytes: &[u8], width: usize, height: usize) -> (usize, usize) {
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            if bytes[(y * width + x) * 4 + 3] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x || min_y > max_y {
        return (0, 0);
    }
    (max_x - min_x + 1, max_y - min_y + 1)
}

#[test]
fn a_formula_svg_carries_glyph_outlines_instead_of_text() {
    let list = display_list(r"\frac{a}{b}+\sqrt{x^{2}+1}");
    let svg = ratex_svg::render_to_svg(&list, &options(20.0));

    assert!(svg.starts_with("<svg "), "{svg}");
    assert!(
        !svg.contains("<text"),
        "glyphs must be outlines so the raster does not depend on a font db"
    );
    assert!(svg.contains("viewBox=\"0 0 "), "{svg}");
}

#[test]
fn the_pt_root_size_does_not_shrink_the_formula() {
    let list = display_list(r"\frac{a}{b}+c");
    let svg = ratex_svg::render_to_svg(&list, &options(20.0));
    let renderer = renderer();
    let parsed = renderer.parse_svg(svg.as_bytes()).expect("svg parses");

    let width = list.width.ceil() as i32;
    let height = (list.height + list.depth).ceil() as i32;
    let target = gpui::Size::new(gpui::DevicePixels(width), gpui::DevicePixels(height));
    let image = renderer
        .render_parsed(&parsed, gpui::SvgSize::ExactSize(target))
        .expect("raster");

    assert_eq!(image.size(0), target);
    let bytes = image.as_bytes(0).expect("one frame");
    let (ink_w, _) = ink_bounds(bytes, width as usize, height as usize);
    assert!(
        ink_w * 10 >= width as usize * 9,
        "ink {ink_w} of {width} wide, the root pt size leaked into the raster"
    );
}

#[test]
fn the_raster_leaves_the_background_transparent() {
    let list = display_list("x+y");
    let svg = ratex_svg::render_to_svg(&list, &options(20.0));
    let renderer = renderer();
    let parsed = renderer.parse_svg(svg.as_bytes()).expect("svg parses");
    let target = gpui::Size::new(gpui::DevicePixels(96), gpui::DevicePixels(48));
    let image = renderer
        .render_parsed(&parsed, gpui::SvgSize::ExactSize(target))
        .expect("raster");

    let bytes = image.as_bytes(0).expect("one frame");
    assert_eq!(bytes.len(), 96 * 48 * 4);
    assert_eq!(bytes[3], 0, "the corner must stay transparent");
    assert!(
        bytes.chunks_exact(4).any(|px| px[3] > 0),
        "the formula must paint something"
    );
}

#[test]
fn a_parsed_formula_can_be_rasterised_on_another_thread() {
    let list = display_list(r"\sum_{i=1}^{n} i");
    let svg = ratex_svg::render_to_svg(&list, &options(20.0));
    let renderer = renderer();
    let parsed = renderer.parse_svg(svg.as_bytes()).expect("svg parses");
    let target = gpui::Size::new(gpui::DevicePixels(80), gpui::DevicePixels(24));

    let joined = std::thread::spawn(move || {
        renderer
            .render_parsed(&parsed, gpui::SvgSize::ExactSize(target))
            .expect("raster")
            .size(0)
    })
    .join()
    .expect("worker thread");

    assert_eq!(joined, target);
}

#[test]
fn one_parse_serves_every_target_size() {
    let list = display_list(r"\int_{0}^{1} f(x)\,dx");
    let svg = ratex_svg::render_to_svg(&list, &options(20.0));
    let renderer = renderer();
    let parsed = renderer.parse_svg(svg.as_bytes()).expect("svg parses");

    let small = gpui::Size::new(gpui::DevicePixels(40), gpui::DevicePixels(20));
    let large = gpui::Size::new(gpui::DevicePixels(160), gpui::DevicePixels(80));
    for target in [small, large] {
        let image = renderer
            .render_parsed(&parsed, gpui::SvgSize::ExactSize(target))
            .expect("raster");
        assert_eq!(image.size(0), target);
    }
}
