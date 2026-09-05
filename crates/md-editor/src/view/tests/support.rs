use crate::view::{CursorMotion, EditorElement, EditorView};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};
use md_theme::DocumentTheme;

pub(super) fn test_doc(markdown: &str) -> Doc {
    Doc::new(load_markdown(markdown, editor_options()))
}

pub(super) fn editor_with_doc<'a>(
    markdown: &str,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<EditorView>, &'a mut VisualTestContext) {
    cx.add_window_view(|_, cx| EditorView::new(test_doc(markdown), DocumentTheme::one_dark(), cx))
}

pub(super) fn chrome() -> md_theme::ChromeTokens {
    DocumentTheme::formal().chrome
}

pub(super) const TABLE_2X2: &str = "| a | b |\n| --- | --- |\n| c | d |\n\nafter\n";

pub(super) const TABLE_2X3: &str = "| a | b | c |\n| --- | --- | --- |\n| d | e | f |\n\nafter\n";

pub(super) const TABLE_3ROW: &str = "| a | b |\n| --- | --- |\n| c | d |\n| e | f |\n\nafter\n";

pub(super) fn focus_editor(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) {
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });
}

pub(super) fn place_caret(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    leaf: usize,
    off: usize,
) {
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[leaf];
            view.place_cursor(Cursor { block, offset: off }, CursorMotion::Move);
        })
    });
}

pub(super) fn place_table_caret(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    text: &str,
    off: usize,
) {
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = table_leaf(&view.state.doc, text);
            view.place_cursor(Cursor { block, offset: off }, CursorMotion::Move);
        })
    });
}

pub(super) fn draw_editor(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) {
    let _ = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
}

pub(super) fn table_leaf(doc: &Doc, text: &str) -> u32 {
    doc.text_leaves()
        .into_iter()
        .find(|&id| doc.text(id) == Some(text))
        .unwrap_or_else(|| panic!("leaf {text:?}"))
}

pub(super) fn table_row_count(doc: &Doc) -> usize {
    doc.document
        .preorder()
        .into_iter()
        .filter(|&id| {
            doc.document
                .arena
                .get(id)
                .is_some_and(|n| n.kind == BlockKind::TableRow)
        })
        .count()
}

pub(super) fn math_parts(art: &md_content::shaper::ShapeArtifact) -> Vec<(String, bool, Px)> {
    art.bands
        .iter()
        .flat_map(|b| &b.parts)
        .filter_map(|p| match p {
            md_content::shaper::ShapePart::Math {
                latex,
                width,
                fallback,
                ..
            } => Some((latex.clone(), fallback.is_some(), *width)),
            _ => None,
        })
        .collect()
}

pub(super) fn temp_md(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "md-test-editor-save-{tag}-{}-{n}.md",
        std::process::id()
    ));
    p
}
