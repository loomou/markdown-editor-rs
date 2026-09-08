use super::hit_test;
use crate::frame::{FrameContext, FrameRequest, compose};
use crate::snap::SnapOperator;
use gpui::TestAppContext;
use md_content::images::SourceKey;
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia};
use md_core::Px;
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_theme::DocumentTheme;
use std::collections::HashMap;
use std::rc::Rc;

fn test_shaper_with_media(
    window: &gpui::Window,
    app: &gpui::App,
    theme: &DocumentTheme,
    image_sizes: HashMap<SourceKey, (u32, u32)>,
    link_dests: HashMap<u32, String>,
) -> GpuiShaper {
    GpuiShaper::new(
        window,
        app,
        theme,
        1.0,
        ShapeCache::new(),
        ShapeMedia {
            mermaid_fitted: Rc::new(Default::default()),
            math_metrics: Rc::new(Default::default()),
            math_gen: 0,
            image_sizes: Rc::new(image_sizes),
            image_failed: Rc::new(Default::default()),
            image_gen: 0,
            link_dests: Rc::new(link_dests),
            link_raw: Rc::new(Default::default()),
            block_image_dest: Rc::new(Default::default()),
            block_code_lang: Rc::new(Default::default()),
        },
    )
}

#[gpui::test]
fn band_rows_survive_a_click_roundtrip(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let md = "look ![pic](pic.png) here\nsecond line has plenty of text to click mid-row\nshort\n";
        let doc = Doc::new(load_markdown(md, editor_options()));
        let env = BoxLayoutEnvironment::default();
        let theme = DocumentTheme::one_dark();
        let mut image_sizes = HashMap::new();
        image_sizes.insert(SourceKey::new("pic.png"), (200, 120));
        let mut link_dests = HashMap::new();
        link_dests.insert(0u32, "pic.png".to_string());
        let shaper = test_shaper_with_media(window, app, &theme, image_sizes, link_dests);
        let snap = SnapOperator::new(1.0);
        let block = doc.text_leaves()[0];
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor: Cursor {
                    block,
                    offset: 0,
                },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let text = frame
            .texts
            .iter()
            .find(|t| t.block == block)
            .expect("text piece");
        let art = &text.art;
        assert_eq!(art.bands.len(), 3, "the fixture must lay out 3 bands");
        assert!(
            art.bands[0].height > art.row_advance * 2.0,
            "the first-line band must be stretched by the image ({}/{})",
            art.bands[0].height,
            art.row_advance
        );

        let display = doc.text(block).expect("text");
        let (cox, coy) = text.content_origin_device;
        let roundtrip = |needle: &str, dx: Px| {
            let off = display.find(needle).unwrap_or_else(|| panic!("{needle} is not in the body text"));
            let (px, row) = shaper.position_for_offset(art, off, text.align, text.content_width);
            let click_x = cox + px + dx;
            let click_y = coy + art.row_top(row) + art.row_height(row) / 2.0;
            let hit = hit_test(
                &frame,
                frame.geometry_revision,
                (click_x, click_y),
                &shaper,
                |_| (0.0, 0.0),
            )
            .expect("a point inside the text box must hit");
            let line_start = display[..off].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line_len = display[line_start..].split('\n').next().unwrap_or("").len();
            assert!(
                hit.offset >= line_start && hit.offset <= line_start + line_len,
                "clicking row {row} at {needle:?} (x={click_x:.1}) landed on {} — the row mapping and band heights have come apart",
                hit.offset
            );
        };
        roundtrip("look", 3.0);
        roundtrip("second", 3.0);
        roundtrip("short", 3.0);
        let plenty = display.find("plenty").expect("plenty");
        let (px, row) = shaper.position_for_offset(art, plenty, text.align, text.content_width);
        let click_x = cox + px + 40.0;
        let click_y = coy + art.row_top(row) + art.row_height(row) / 2.0;
        let hit = hit_test(
            &frame,
            frame.geometry_revision,
            (click_x, click_y),
            &shaper,
            |_| (0.0, 0.0),
        )
        .expect("a click inside the text box must hit something");
        assert_eq!(hit.block, block);
        assert_ne!(
            hit.offset,
            display.len(),
            "a click just right of the line middle swallowed the block-text end — the old shape of a row-count bug hitting the fallback"
        );
    });
}
