use super::support::{draw_editor, editor_with_doc, focus_editor};
use crate::view::media_zoom::collect_hits;
use crate::view::{EditorElement, EditorView};
use gpui::Modifiers;
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_core::Px;
use md_core::block::BlockKind;
use md_render::snapshot::TextPiece;

const MERMAID: &str = "before\n\n```mermaid\nflowchart TD\nA-->B\n```\n\nafter\n";
const MATH: &str = "before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n";
const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn draw_texts(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<TextPiece> {
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    )
    .1
    .frame
    .snapshot
    .texts
    .clone()
}

fn preview_center(texts: &[TextPiece], kind: BlockKind) -> (Px, Px) {
    let t = texts
        .iter()
        .find(|t| t.kind == kind && !t.edit_source)
        .unwrap_or_else(|| panic!("no {kind:?} preview"));
    (
        t.content_origin_device.0 + t.content_width * 0.5,
        t.content_origin_device.1 + t.view_height * 0.5,
    )
}

fn hover_kind(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    kind: BlockKind,
) -> (Px, Px) {
    let texts = draw_texts(editor, cx);
    let editing = cx.update(|_, app| editor.read(app).state.doc.block_edit());
    let hits = collect_hits(&texts, editing);
    let pos = preview_center(&texts, kind);
    cx.update(|_, app| {
        editor.update(app, |v, cx| v.hover_media(&hits, pos, cx));
    });
    pos
}

#[gpui::test]
fn hovering_a_mermaid_preview_shows_the_zoom_chrome(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(MERMAID, cx);
    focus_editor(&editor, cx);
    hover_kind(&editor, cx, BlockKind::Mermaid);
    cx.update(|_, app| {
        let chrome = editor.read(app).media_chrome.expect("chrome");
        assert_eq!(chrome.kind, BlockKind::Mermaid);
        assert!(editor.read(app).media_zoom.is_none());
        assert_eq!(editor.read(app).state.doc.block_edit(), None);
    });
}

#[gpui::test]
fn hovering_an_image_preview_shows_the_zoom_chrome(cx: &mut TestAppContext) {
    let md = format!("before\n\n![a]({PIXEL})\n\nafter\n");
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    hover_kind(&editor, cx, BlockKind::Image);
    cx.update(|_, app| {
        let chrome = editor.read(app).media_chrome.expect("chrome");
        assert_eq!(chrome.kind, BlockKind::Image);
        assert_eq!(editor.read(app).state.doc.block_edit(), None);
    });
}

#[gpui::test]
fn hovering_a_math_block_does_not_show_zoom_chrome(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(MATH, cx);
    focus_editor(&editor, cx);
    hover_kind(&editor, cx, BlockKind::Math);
    cx.update(|_, app| {
        assert!(editor.read(app).media_chrome.is_none());
        assert!(editor.read(app).media_zoom.is_none());
    });
}

#[gpui::test]
fn opening_zoom_does_not_enter_block_edit(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(MERMAID, cx);
    focus_editor(&editor, cx);
    hover_kind(&editor, cx, BlockKind::Mermaid);
    cx.update(|_, app| {
        editor.update(app, |v, cx| {
            let chrome = v.media_chrome.expect("chrome");
            v.open_media_zoom(chrome, cx);
        });
    });
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.media_zoom.is_some());
        assert!(view.media_chrome.is_none());
        assert_eq!(view.state.doc.block_edit(), None);
    });
}

#[gpui::test]
fn escape_closes_media_zoom_without_entering_source(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(MERMAID, cx);
    focus_editor(&editor, cx);
    hover_kind(&editor, cx, BlockKind::Mermaid);
    cx.update(|_, app| {
        editor.update(app, |v, cx| {
            let chrome = v.media_chrome.expect("chrome");
            v.open_media_zoom(chrome, cx);
        });
    });
    cx.simulate_keystrokes("escape");
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.media_zoom.is_none());
        assert_eq!(view.state.doc.block_edit(), None);
    });
}

#[gpui::test]
fn clicking_the_mermaid_preview_still_enters_source(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(MERMAID, cx);
    focus_editor(&editor, cx);
    let pos = {
        let texts = draw_texts(&editor, cx);
        preview_center(&texts, BlockKind::Mermaid)
    };
    cx.simulate_click(point(px(pos.0 as f32), px(pos.1 as f32)), Modifiers::none());
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.media_zoom.is_none());
        assert!(
            view.state.doc.block_edit().is_some(),
            "clicking the figure should open the source well"
        );
    });
}

#[gpui::test]
fn media_chrome_layout_y_holds_across_scroll(cx: &mut TestAppContext) {
    let mut md = MERMAID.to_string();
    for i in 0..40 {
        md.push_str(&format!("para {i}\n\n"));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    hover_kind(&editor, cx, BlockKind::Mermaid);
    let y0 = cx.update(|_, app| editor.read(app).media_chrome.expect("chrome").y);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.follow_caret = false;
            view.state.scroll += 40.0;
            cx.notify();
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.media_chrome.expect("chrome");
        assert!(
            (view.state.scroll - 40.0).abs() < 1.0,
            "scroll clamped away: {}",
            view.state.scroll
        );
        assert!(
            (chrome.y - y0).abs() < 2.0,
            "layout y jumped on scroll: {y0} -> {}",
            chrome.y
        );
    });
}
