use super::support::{editor_with_doc, focus_editor};
use crate::view::EditorView;
use gpui::TestAppContext;
use md_core::block::BlockKind;
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};
use md_theme::DocumentTheme;

const PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50,
    0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[gpui::test]
fn dropping_a_png_beside_the_markdown_inserts_a_relative_image(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!(
        "md-test-editor-img-drop-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("images")).expect("dir");
    let png = dir.join("images").join("cat.png");
    std::fs::write(&png, PIXEL_PNG).expect("png");
    let md_path = dir.join("notes.md");
    std::fs::write(&md_path, "hello\n").expect("md");

    let (editor, cx) = cx.add_window_view(|_, cx| {
        EditorView::new(
            Doc::with_path(load_markdown("hello\n", editor_options()), Some(md_path)),
            DocumentTheme::one_dark(),
            cx,
        )
    });
    focus_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.drop_images(&[png], window, cx);
        });
    });

    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(
            md.contains("![cat](images/cat.png)"),
            "expected relative image markdown, got {md:?}"
        );
        assert!(
            view.state
                .doc
                .text_leaves()
                .into_iter()
                .any(|id| view.state.doc.kind(id) == Some(BlockKind::Image)),
            "dropped image should become an image block"
        );
    });

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let img = view
                .state
                .doc
                .text_leaves()
                .into_iter()
                .find(|&id| view.state.doc.kind(id) == Some(BlockKind::Image))
                .expect("image");
            let other = view
                .state
                .doc
                .text_leaves()
                .into_iter()
                .find(|&id| id != img)
                .expect("another leaf");
            view.state.cursor = view.state.doc.retarget_focus(md_core::doc::Cursor {
                block: other,
                offset: 0,
            });
        });
    });
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(
            md.contains("![cat](images/cat.png)"),
            "image must survive leaving the block, got {md:?}"
        );
        assert!(
            view.state
                .doc
                .text_leaves()
                .into_iter()
                .any(|id| view.state.doc.kind(id) == Some(BlockKind::Image)),
            "leaving the dropped image should not demote it"
        );
    });

    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn dropping_a_non_image_does_nothing(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    focus_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.drop_images(&[std::path::PathBuf::from("notes.md")], window, cx);
        });
    });
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some("hello"));
    });
}
