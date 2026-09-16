use crate::view::tests::support::{draw_editor, editor_with_doc};
use crate::view::{CursorMotion, EditorElement, EditorView};
use gpui::{MouseButton, MouseUpEvent, TestAppContext, VisualTestContext, point, px, size};
use md_core::block::BlockKind;
use md_core::doc::Cursor;
use md_core::document::{Command, Sel};

fn heading_blocks(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<u32> {
    cx.update(|_, app| {
        let doc = &editor.read(app).state.doc.document;
        doc.preorder()
            .into_iter()
            .filter_map(|id| match doc.arena.get(id)?.kind {
                BlockKind::Heading(_) => Some(id.index),
                _ => None,
            })
            .collect()
    })
}

fn caret_block(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> u32 {
    cx.update(|_, app| editor.read(app).state.cursor.block)
}

fn place_caret(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext, block: u32) {
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.place_cursor(Cursor { block, offset: 0 }, CursorMotion::Move);
        });
    });
}

fn follow(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext, dest: &str) {
    let dest = dest.to_owned();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.pending_open = Some(dest.clone());
        });
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(px(400.0), px(300.0)),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

fn spaced_headings(count: usize) -> String {
    let mut md = String::new();
    for i in 0..count {
        md.push_str(&format!("## Heading {i}\n\n{}\n\n", "word ".repeat(40)));
    }
    md
}

#[gpui::test]
fn a_followed_anchor_link_jumps_to_its_heading(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&spaced_headings(40), cx);
    let headings = heading_blocks(&editor, cx);
    assert!(headings.len() >= 8);
    let target = headings[7];
    let draw = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>| {
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(480.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        )
        .1
        .frame
        .snapshot
    };

    place_caret(&editor, cx, *headings.last().expect("a heading"));
    let before = draw(cx, &editor);
    assert_ne!(caret_block(&editor, cx), target);

    follow(&editor, cx, "#heading-7");
    let after = draw(cx, &editor);
    assert_eq!(
        caret_block(&editor, cx),
        target,
        "the caret should land on the linked heading"
    );
    let caret_y = after.caret_device.expect("caret").1;
    assert!(
        (8.0..20.0).contains(&caret_y),
        "the linked heading should park at the top, but the caret sits at y={caret_y} (before the jump {})",
        before.caret_device.expect("caret").1
    );
}

#[gpui::test]
fn a_percent_encoded_anchor_reaches_an_ideographic_heading(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# 中文标题\n\nbody\n\n# Other\n\ntail\n", cx);
    let headings = heading_blocks(&editor, cx);
    draw_editor(&editor, cx);
    follow(&editor, cx, "#%E4%B8%AD%E6%96%87%E6%A0%87%E9%A2%98");
    assert_eq!(caret_block(&editor, cx), headings[0]);
}

#[gpui::test]
fn an_unknown_anchor_leaves_the_caret_alone(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# Alpha\n\nbody\n\n# Beta\n\ntail\n", cx);
    let headings = heading_blocks(&editor, cx);
    let parked = headings[1];
    place_caret(&editor, cx, parked);
    draw_editor(&editor, cx);
    follow(&editor, cx, "#does-not-exist");
    assert_eq!(caret_block(&editor, cx), parked);
    follow(&editor, cx, "#");
    assert_eq!(caret_block(&editor, cx), parked);
}

#[gpui::test]
fn the_anchor_table_follows_an_edit(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# Alpha\n\nbody\n\n# Gamma\n\ntail\n", cx);
    let headings = heading_blocks(&editor, cx);
    let renamed = headings[0];
    let elsewhere = headings[1];
    draw_editor(&editor, cx);

    follow(&editor, cx, "#alpha");
    assert_eq!(caret_block(&editor, cx), renamed);

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.state.doc.apply(
                Sel::collapsed(Cursor {
                    block: renamed,
                    offset: 0,
                }),
                Command::Insert {
                    text: "x".to_owned(),
                },
            );
        });
    });

    place_caret(&editor, cx, elsewhere);
    follow(&editor, cx, "#alpha");
    assert_eq!(
        caret_block(&editor, cx),
        elsewhere,
        "the stale slug should no longer resolve"
    );
    follow(&editor, cx, "#xalpha");
    assert_eq!(
        caret_block(&editor, cx),
        renamed,
        "the renamed heading should resolve under its new slug"
    );
}
