use super::support::{editor_with_doc, focus_editor};
use crate::view::{EditorView, UnsavedChoice};
use gpui::TestAppContext;
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{Command, Sel, editor_options, load_markdown};
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
        "markdown-editor-rs-editor-img-drop-{}-{}",
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

fn drop_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "markdown-editor-rs-editor-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("dir");
    dir
}

#[gpui::test]
fn dropping_a_markdown_file_opens_it_instead_of_the_empty_untitled_doc(cx: &mut TestAppContext) {
    let dir = drop_dir("md-drop-open");
    let png = dir.join("cat.png");
    std::fs::write(&png, PIXEL_PNG).expect("png");
    let md_path = dir.join("dropped.md");
    std::fs::write(&md_path, "# dropped\n").expect("md");

    let (editor, cx) = cx.add_window_view(|_, cx| {
        EditorView::new(
            Doc::new(load_markdown("", editor_options())),
            DocumentTheme::one_dark(),
            cx,
        )
    });
    focus_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.drop_paths(&[png, md_path.clone()], window, cx);
        });
    });
    cx.run_until_parked();

    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(
            view.state.doc.source_path.as_deref(),
            Some(md_path.as_path()),
            "the dropped markdown should replace the empty untitled doc"
        );
        let md = view.state.doc.document.to_markdown();
        assert!(
            md.contains("# dropped"),
            "expected the dropped file's content, got {md:?}"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn dropping_a_markdown_file_on_a_dirty_doc_asks_before_replacing(cx: &mut TestAppContext) {
    let dir = drop_dir("md-drop-dirty");
    let md_path = dir.join("late.md");
    std::fs::write(&md_path, "late\n").expect("md");

    let (editor, cx) = cx.add_window_view(|_, cx| {
        EditorView::new(
            Doc::new(load_markdown("draft\n", editor_options())),
            DocumentTheme::one_dark(),
            cx,
        )
    });
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let leaf = view.state.doc.text_leaves()[0];
            view.state.doc.apply(
                Sel::collapsed(Cursor {
                    block: leaf,
                    offset: 0,
                }),
                Command::Insert { text: "x".into() },
            );
        });
    });

    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.drop_paths(std::slice::from_ref(&md_path), window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        cx.update(|_, app| editor.read(app).unsaved_nav.is_some()),
        "a dirty doc must ask before the dropped file replaces it"
    );
    assert_eq!(
        cx.update(|_, app| editor.read(app).state.doc.source_path.clone()),
        None
    );

    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_unsaved_choice(UnsavedChoice::Cancel, window, cx);
        });
    });
    cx.run_until_parked();
    assert!(cx.update(|_, app| editor.read(app).unsaved_nav.is_none()));
    assert!(cx.update(|_, app| editor.read(app).state.doc.is_dirty()));

    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.drop_paths(std::slice::from_ref(&md_path), window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_unsaved_choice(UnsavedChoice::Discard, window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(
            view.state.doc.source_path.as_deref(),
            Some(md_path.as_path()),
            "confirming the dialog should open the dropped file"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}
