use super::support::{editor_with_doc, focus_editor, place_caret};
use crate::view::{EditorElement, EditorView};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_core::block::BlockKind;

#[gpui::test]
fn scrolling_estimated_blocks_paints_at_the_requested_offset(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..120 {
        let words = [3, 9, 20, 40, 70][i % 5];
        md.push_str(&format!(
            "para {i}: {}\n\n",
            format!("word{i} ").repeat(words)
        ));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let draw = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>| {
        let drawn = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
        drawn.1.frame.snapshot.scroll
    };
    place_caret(&editor, cx, 20, 0);
    let _ = draw(cx, &editor);
    cx.update(|_, app| {
        app.write_to_clipboard(gpui::ClipboardItem::new_string(
            "## t\n\n```rust\nfn x() {}\n```\n\n- a\n- b\n".into(),
        ))
    });
    cx.simulate_keystrokes("secondary-v");
    let mut drift = Vec::new();
    for _ in 0..120 {
        let want = cx.update(|_, app| {
            editor.update(app, |v, _| {
                v.state.scroll += 13.0;
                v.state.scroll
            })
        });
        let got = draw(cx, &editor);
        if (got - want).abs() > 1e-6 {
            drift.push((want, got));
        }
    }
    assert!(
        drift.is_empty(),
        "what was painted drifted from what was requested: {drift:?}"
    );
}

#[gpui::test]
fn select_all_delete_paints_the_empty_document_at_the_top(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..120 {
        let words = [3, 9, 20, 40, 70][i % 5];
        md.push_str(&format!(
            "para {i}: {}\n\n",
            format!("word{i} ").repeat(words)
        ));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let draw = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>| {
        let drawn = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
        let snap = &drawn.1.frame.snapshot;
        (
            snap.scroll,
            drawn.1.frame.total_height,
            snap.caret_device,
            snap.texts.len(),
        )
    };
    place_caret(&editor, cx, 0, 0);
    let _ = draw(cx, &editor);
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            v.state.scroll = 4_000.0;
        })
    });
    let (before, _, _, _) = draw(cx, &editor);
    assert!(before > 0.0);
    cx.simulate_keystrokes("secondary-a");
    let _ = draw(cx, &editor);
    cx.simulate_keystrokes("backspace");

    let (scroll, total, caret, texts) = draw(cx, &editor);
    let state_scroll = cx.update(|_, app| editor.read(app).state.scroll);
    assert_eq!(scroll, 0.0, "an empty document still painted at {scroll}");
    assert_eq!(
        state_scroll, 0.0,
        "the scroll amount is stuck outside the document: {state_scroll}"
    );
    assert!(
        total <= 600.0,
        "an empty document's total height is {total}"
    );
    assert!(caret.is_some(), "the empty document painted no caret");
    assert_eq!(texts, 1, "the empty document published {texts} text pieces");

    cx.simulate_keystrokes("z");
    let (scroll, _, typed_caret, texts) = draw(cx, &editor);
    assert_eq!(scroll, 0.0);
    assert_eq!(
        texts, 2,
        "the body plus the trailing empty paragraph published {texts} text pieces"
    );
    let (x0, _, _, _) = caret.expect("caret");
    let (x1, _, _, _) = typed_caret.expect("caret after typing");
    assert!(
        x1 > x0,
        "after typing the caret did not move forward: {x0} -> {x1}"
    );
    let md_after = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    assert_eq!(md_after, "z\n");
}

#[gpui::test]
fn shrinking_the_document_pulls_the_viewport_back_into_range(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..120 {
        md.push_str(&format!("para {i}: {}\n\n", "word ".repeat(20)));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let draw = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>| {
        let drawn = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
        let snap = &drawn.1.frame.snapshot;
        (snap.scroll, drawn.1.frame.total_height, snap.caret_device)
    };
    place_caret(&editor, cx, 0, 0);
    let _ = draw(cx, &editor);
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_keystrokes("backspace");
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            v.state.scroll = 4_000.0;
            v.follow_caret = false;
        })
    });
    let (scroll, total, caret) = draw(cx, &editor);
    assert!(
        total <= 600.0,
        "an empty document's total height is {total}"
    );
    assert_eq!(
        scroll, 0.0,
        "the viewport stayed outside the document at {scroll}"
    );
    let (_, y, _, _) = caret.expect("caret");
    assert!(y >= 0.0, "the caret is painted above the viewport at {y}");
}

#[gpui::test]
fn select_all_does_not_move_the_viewport(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..120 {
        md.push_str(&format!("para {i}: {}\n\n", "word ".repeat(20)));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let draw = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>| {
        let drawn = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
        drawn.1.frame.snapshot.scroll
    };
    place_caret(&editor, cx, 0, 0);
    let _ = draw(cx, &editor);
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            v.state.scroll = 2_000.0;
        })
    });
    let before = draw(cx, &editor);
    assert!(
        (before - 2_000.0).abs() < 1.0,
        "even the starting viewport is wrong: {before}"
    );

    cx.simulate_keystrokes("secondary-a");
    let after = draw(cx, &editor);
    let state_scroll = cx.update(|_, app| editor.read(app).state.scroll);
    assert!(
        (after - before).abs() < 1.0,
        "select-all flung the viewport to {after}"
    );
    assert_eq!(
        state_scroll, 2_000.0,
        "select-all changed the scroll amount: {state_scroll}"
    );

    let (sel, first, last, end) = cx.update(|_, app| {
        let v = editor.read(app);
        let mut leaves = v.state.doc.text_leaves();
        if leaves.len() > 1 {
            let last = *leaves.last().expect("leaf");
            if v.state.doc.kind(last) == Some(BlockKind::Paragraph)
                && v.state.doc.caret_text(last).is_some_and(|t| t.is_empty())
            {
                leaves.pop();
            }
        }
        let last = *leaves.last().expect("leaf");
        let end = v.state.doc.caret_text(last).map_or(0, |t| t.len());
        (v.state.selection, leaves[0], last, end)
    });
    let (anchor, head) = sel.expect("select-all must leave a selection");
    assert_eq!((anchor.block, anchor.offset), (first, 0));
    assert_eq!((head.block, head.offset), (last, end));
}
