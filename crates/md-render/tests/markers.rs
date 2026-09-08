use gpui::TestAppContext;
use md_content::images::SourceKey;
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia};
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_render::frame::{FrameContext, FrameRequest, compose};
use md_render::snap::SnapOperator;
use md_theme::DocumentTheme;
use std::collections::HashMap;
use std::rc::Rc;

fn shaper_with_pic(window: &gpui::Window, app: &gpui::App, theme: &DocumentTheme) -> GpuiShaper {
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
            image_sizes: Rc::new(HashMap::from([(SourceKey::new("pic.png"), (200, 120))])),
            image_failed: Rc::new(Default::default()),
            image_gen: 0,
            link_dests: Rc::new(Default::default()),
            link_raw: Rc::new(Default::default()),
            block_image_dest: Rc::new(Default::default()),
            block_code_lang: Rc::new(Default::default()),
        },
    )
}

fn first_line_ink_center(frame: &md_render::snapshot::Frame, shaper: &GpuiShaper) -> f64 {
    let tall = |bands: &[md_content::shaper::ShapeBand], adv: f64| {
        bands.first().is_some_and(|b| b.height > adv * 2.0)
    };
    let (box_id, kind, origin_y, art) = if let Some(t) = frame
        .texts
        .iter()
        .find(|t| tall(&t.art.bands, t.art.row_advance))
    {
        (t.box_id, t.kind, t.content_origin_device.1, t.art.clone())
    } else {
        let c = frame
            .cells
            .iter()
            .find(|c| tall(&c.art.bands, c.art.row_advance))
            .unwrap();
        let node = frame.assembly.tree.get(c.cell_box);
        (
            c.cell_box,
            node.kind(),
            c.content_origin_device.1,
            c.art.clone(),
        )
    };
    let node = frame.assembly.tree.get(box_id);
    let (dy, height) = shaper.caret_ink(kind, node.type_slot(), &art, 0);
    origin_y + dy + height / 2.0
}

#[gpui::test]
fn checkbox_aligns_to_tall_band_text(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let doc = Doc::new(load_markdown(
            "- [ ] look ![pic](pic.png) here\n",
            editor_options(),
        ));
        let theme = DocumentTheme::one_dark();
        let env = BoxLayoutEnvironment::default();
        let shaper = shaper_with_pic(window, app, &theme);
        let snap = SnapOperator::new(1.0);
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
                    block: doc.text_leaves()[0],
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
            .find(|t| t.kind == BlockKind::Paragraph)
            .unwrap();
        assert!(
            text.art.bands[0].height > text.art.row_advance * 2.0,
            "fixture must have a tall first band: {} vs {}",
            text.art.bands[0].height,
            text.art.row_advance
        );
        let ink_center = first_line_ink_center(&frame, &shaper);
        let marker = frame.decorations.iter().find(|d| d.task.is_some()).unwrap();
        let marker_center = marker.gutter_dot.unwrap().1 + theme.decoration.task_size / 2.0;
        println!(
            "checkbox center={marker_center} text_ink_center={ink_center} band_height={}",
            text.art.bands[0].height
        );
        assert!(
            (marker_center - ink_center).abs() < 3.0,
            "checkbox must follow the first line's actual text ink"
        );
    });
}

#[gpui::test]
fn bullet_aligns_to_tall_band_text(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let doc = Doc::new(load_markdown(
            "- look ![pic](pic.png) here\n",
            editor_options(),
        ));
        let theme = DocumentTheme::one_dark();
        let env = BoxLayoutEnvironment::default();
        let shaper = shaper_with_pic(window, app, &theme);
        let snap = SnapOperator::new(1.0);
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
                    block: doc.text_leaves()[0],
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
        let ink_center = first_line_ink_center(&frame, &shaper);
        let marker = frame
            .decorations
            .iter()
            .find(|d| d.task.is_none() && d.gutter_label.is_none())
            .unwrap();
        let marker_center = marker.gutter_dot.unwrap().1 + theme.decoration.list_marker_size / 2.0;
        assert!(
            (marker_center - ink_center).abs() < 3.0,
            "bullet must follow the first line's actual text ink: {marker_center} vs {ink_center}"
        );
    });
}

#[gpui::test]
fn bullet_on_leading_table_aligns_to_tall_band_text(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let doc = Doc::new(load_markdown(
            "- | ![pic](pic.png) |\n  | --- |\n  | look |\n",
            editor_options(),
        ));
        let theme = DocumentTheme::one_dark();
        let env = BoxLayoutEnvironment::default();
        let shaper = shaper_with_pic(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let frame = compose(
            FrameContext { doc: &doc, env, shaper: &shaper, snap: &snap, theme: &theme },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor: Cursor { block: doc.text_leaves()[0], offset: 0 },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let ink_center = first_line_ink_center(&frame, &shaper);
        let marker = frame
            .decorations
            .iter()
            .find(|d| d.task.is_none() && d.gutter_label.is_none())
            .unwrap();
        let marker_center =
            marker.gutter_dot.unwrap().1 + theme.decoration.list_marker_size / 2.0;
        assert!(
            (marker_center - ink_center).abs() < 3.0,
            "bullet on a leading table must follow the first row's actual text ink: {marker_center} vs {ink_center}"
        );
    });
}
