use super::support::{editor_with_doc, math_parts};
use crate::view::{EditorElement, EditorView};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_core::Px;
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};
use md_theme::DocumentTheme;

fn draw_first_art(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
) -> std::rc::Rc<md_content::shaper::ShapeArtifact> {
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    drawn.1.frame.snapshot.texts[0].art.clone()
}

#[gpui::test]
fn unparsable_inline_math_falls_back_to_the_full_source(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("$a^$ tail", cx);
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();

    let art = draw_first_art(&editor, cx);
    let parts = math_parts(&art);
    assert_eq!(
        parts.len(),
        1,
        "still one Math part after the fallback, caret landing unaffected"
    );
    assert_eq!(parts[0].0, "a^");
    assert!(
        parts[0].1,
        "after a parse failure it must keep the raw `$a^$` source line"
    );
    assert!(parts[0].2 > 0.0);
}

#[gpui::test]
fn parsable_inline_math_keeps_rasterizing(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("$a^2$ tail", cx);
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();

    let art = draw_first_art(&editor, cx);
    let parts = math_parts(&art);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].0, "a^2");
    assert!(
        !parts[0].1,
        "a successfully parsed formula should have no fallback line"
    );
}

#[gpui::test]
fn math_fallback_advances_x_for_the_following_text(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("$a^$ tail", cx);
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();
    let art = draw_first_art(&editor, cx);

    let mut math_end = None;
    let mut tail_x = None;
    for part in art.bands.iter().flat_map(|b| &b.parts) {
        match part {
            md_content::shaper::ShapePart::Math {
                x, width, fallback, ..
            } => {
                assert!(
                    fallback.is_some(),
                    "after a parse failure it should carry the fallback line"
                );
                math_end = Some(*x + *width);
            }
            md_content::shaper::ShapePart::Text { x, line, .. }
                if line.text.as_ref().contains("tail") =>
            {
                tail_x = Some(*x);
            }
            _ => {}
        }
    }
    let math_end = math_end.expect("math part");
    let tail_x = tail_x.expect("tail text part");
    assert!(
        tail_x >= math_end - 0.5,
        "the following text should sit after the fallback row:tail_x={tail_x} math_end={math_end}"
    );
}

fn image_parts(art: &md_content::shaper::ShapeArtifact) -> Vec<(String, bool, Px)> {
    art.bands
        .iter()
        .flat_map(|b| &b.parts)
        .filter_map(|p| match p {
            md_content::shaper::ShapePart::Image {
                dest,
                width,
                fallback,
                ..
            } => Some((dest.clone(), fallback.is_some(), *width)),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn unloadable_inline_image_falls_back_to_the_full_source(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("![cat](./nope.png) tail", cx);
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();

    let art = draw_first_art(&editor, cx);
    let parts = image_parts(&art);
    assert_eq!(
        parts.len(),
        1,
        "still one Image part after the fallback, caret landing unaffected"
    );
    assert!(
        parts[0].1,
        "after a load failure it must keep the raw `![cat](./nope.png)` source line"
    );
    assert!(parts[0].2 > 0.0);
}

#[gpui::test]
fn image_fallback_advances_x_for_the_following_text(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("![cat](./nope.png) tail", cx);
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();
    let art = draw_first_art(&editor, cx);

    let mut img_end = None;
    let mut tail_x = None;
    for part in art.bands.iter().flat_map(|b| &b.parts) {
        match part {
            md_content::shaper::ShapePart::Image {
                x, width, fallback, ..
            } => {
                assert!(
                    fallback.is_some(),
                    "after a load failure it should carry the fallback line"
                );
                img_end = Some(*x + *width);
            }
            md_content::shaper::ShapePart::Text { x, line, .. }
                if line.text.as_ref().contains("tail") =>
            {
                tail_x = Some(*x);
            }
            _ => {}
        }
    }
    let img_end = img_end.expect("image part");
    let tail_x = tail_x.expect("tail text part");
    assert!(
        tail_x >= img_end - 0.5,
        "the following text should sit after the fallback row:tail_x={tail_x} img_end={img_end}"
    );
}

#[gpui::test]
fn image_fallback_keeps_the_band_one_row_tall(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("![cat](./nope.png) tail", cx);
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();
    let art = draw_first_art(&editor, cx);

    assert_eq!(
        art.bands.len(),
        1,
        "it fits on one row, so there should be a single band"
    );
    assert!(
        (art.bands[0].height - art.row_advance).abs() < 0.5,
        "after the fallback the band should shrink to one row:height={} row_advance={}",
        art.bands[0].height,
        art.row_advance
    );
}

#[gpui::test]
fn long_image_fallback_wraps_inside_the_text_column(cx: &mut TestAppContext) {
    let md = "start ![screenshot](https://example.com/very/long/path/to/some/deeply/nested/image-name-v2-final-really-long.png) end";
    let (editor, cx) = editor_with_doc(md, cx);
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();

    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let t = &drawn.1.frame.snapshot.texts[0];
    let (band, part) = t
        .art
        .bands
        .iter()
        .find_map(|b| {
            b.parts.iter().find_map(|p| match p {
                md_content::shaper::ShapePart::Image { fallback, .. } if fallback.is_some() => {
                    Some((b, p))
                }
                _ => None,
            })
        })
        .expect("the fallback image part");
    let md_content::shaper::ShapePart::Image {
        x,
        width,
        slot_h,
        fallback: Some(line),
        ..
    } = part
    else {
        unreachable!()
    };

    assert!(
        *x + *width <= t.content_width + 0.5,
        "the fallback row must not overflow the text column:right={} content_width={}",
        *x + *width,
        t.content_width
    );
    assert!(
        !line.wrap_boundaries.is_empty(),
        "source this long should wrap"
    );

    let rows = (line.wrap_boundaries.len() + 1) as f64 * t.art.row_advance;
    assert!(
        (band.height - rows).abs() < 0.5,
        "the band should hold every row of the wrap:height={} rows={rows}",
        band.height
    );
    assert!(
        (*slot_h - rows).abs() < 0.5,
        "slot_h should equal the total height after the wrap"
    );
}

const PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50,
    0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[gpui::test]
fn relative_image_beside_the_markdown_file_loads(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!(
        "md-test-editor-img-rel-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("a.png"), PIXEL_PNG).expect("png");
    let md_path = dir.join("doc.md");
    let markdown = "![cat](./a.png) tail";
    std::fs::write(&md_path, markdown).expect("md");

    let (editor, cx) = cx.add_window_view(|_, cx| {
        EditorView::new(
            Doc::with_path(load_markdown(markdown, editor_options()), Some(md_path)),
            DocumentTheme::one_dark(),
            cx,
        )
    });
    let _ = draw_first_art(&editor, cx);
    cx.run_until_parked();
    let art = draw_first_art(&editor, cx);
    let parts = image_parts(&art);
    assert_eq!(parts.len(), 1, "still one image part");
    assert!(
        !parts[0].1,
        "relative image next to the markdown file should load, dest={}",
        parts[0].0
    );

    let _ = std::fs::remove_dir_all(&dir);
}
