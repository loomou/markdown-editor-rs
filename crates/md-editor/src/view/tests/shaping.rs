use super::support::{draw_editor, editor_with_doc};
use crate::view::{EditorElement, EditorView};
use gpui::Entity;
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_content::shaper::ShapeArtifact;
use md_core::block::BlockKind;
use std::rc::Rc;

fn draw_arts(
    editor: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    width: f32,
) -> (Rc<ShapeArtifact>, Rc<ShapeArtifact>) {
    let (_, prepaint) = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(width), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let texts = &prepaint.frame.snapshot.texts;
    let find = |kind: BlockKind| {
        texts
            .iter()
            .find(|p| p.kind == kind)
            .unwrap_or_else(|| panic!("the {kind:?} text piece is not in the publish window"))
            .art
            .clone()
    };
    (find(BlockKind::MetadataBlock), find(BlockKind::Paragraph))
}

#[gpui::test]
fn metadata_blocks_do_not_wrap_at_any_window_width(cx: &mut TestAppContext) {
    let long_line = format!("tags: [{}]", "item-00000, ".repeat(60));
    let md = format!("---\ntitle: demo\n{long_line}\n---\n\n{long_line}\n");
    let (editor, cx) = editor_with_doc(&md, cx);
    cx.run_until_parked();

    let (meta_wide, _) = draw_arts(&editor, cx, 1000.0);
    let (meta_narrow, para_narrow) = draw_arts(&editor, cx, 360.0);

    for (name, art) in [("wide", &meta_wide), ("narrow", &meta_narrow)] {
        assert!(
            art.lines.len() >= 2,
            "front matter did not lay out over multiple lines in the {name} window; the fixture is wrong"
        );
        assert_eq!(
            art.rows,
            art.lines.len() as u32,
            "the metadata block wrapped in the {name} window"
        );
        assert!(
            art.lines.iter().all(|l| l.wrap_boundaries.is_empty()),
            "the metadata block has wrap boundaries in the {name} window"
        );
    }
    assert!(
        Rc::ptr_eq(&meta_wide, &meta_narrow),
        "the metadata block key is width-independent, so a different window width should reuse the same artifact"
    );
    assert!(
        para_narrow.rows > para_narrow.lines.len() as u32,
        "the body paragraph did not wrap in the narrow window; the control group failed: this width is not narrow enough"
    );
}

#[gpui::test]
fn scrolling_does_not_unbounded_grow_the_shape_cache(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..240 {
        md.push_str(&format!("paragraph {i} {}\n\n", "word ".repeat(8)));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    let mut peak = 0usize;
    for _ in 0..16 {
        draw_editor(&editor, cx);
        let n = cx.update(|_, app| editor.read(app).state.shape_cache.entry_count());
        peak = peak.max(n);
        cx.update(|_, app| {
            editor.update(app, |view, _| {
                view.state.scroll += 480.0;
            });
        });
    }
    assert!(peak > 0);
    assert!(peak < 240, "shape cache kept every paragraph: peak={peak}");
    for _ in 0..4 {
        draw_editor(&editor, cx);
    }
    let settled = cx.update(|_, app| editor.read(app).state.shape_cache.entry_count());
    assert!(
        settled <= peak,
        "idle frames grew the cache: peak={peak} settled={settled}"
    );
}
