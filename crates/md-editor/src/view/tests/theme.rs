use super::support::{editor_with_doc, focus_editor, place_caret};
use crate::view::{EditorElement, EditorView};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_theme::{DocumentTheme, LineBreakMode};
use std::rc::Rc;

#[gpui::test]
fn recoloring_keeps_the_layout_and_the_scroll_position(cx: &mut TestAppContext) {
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
        (
            drawn.1.frame.snapshot.scroll,
            drawn.1.frame.total_height,
            drawn.1.frame.snapshot.texts.len(),
        )
    };

    place_caret(&editor, cx, 0, 0);
    let _ = draw(cx, &editor);
    for _ in 0..40 {
        cx.update(|_, app| {
            editor.update(app, |v, _| {
                v.state.scroll += 60.0;
            })
        });
        let _ = draw(cx, &editor);
    }
    let before = draw(cx, &editor);
    assert!(before.0 > 0.0, "did not scroll: {before:?}");
    let (cache_before, invalidations_before) = cx.update(|_, app| {
        let v = editor.read(app);
        (
            Rc::clone(&v.state.shape_cache),
            v.state.shape_cache.stats().env_invalidations,
        )
    });

    cx.update(|_, app| {
        editor.update(app, |v, cx| {
            v.set_theme(DocumentTheme::one_light(), cx);
            assert!(
                v.state.incremental.is_some(),
                "recoloring dropped the engine"
            );
        })
    });
    let after = draw(cx, &editor);
    assert_eq!(before, after, "recoloring moved the layout");
    cx.update(|_, app| {
        let v = editor.read(app);
        assert!(
            Rc::ptr_eq(&cache_before, &v.state.shape_cache),
            "recoloring replaced the whole ShapeCache"
        );
        assert!(
            v.state.shape_cache.stats().env_invalidations > invalidations_before,
            "the old-color artifacts were not invalidated"
        );
        assert!(!v.state.theme.is_dark(), "the color scheme did not switch");
    });

    cx.update(|_, app| {
        editor.update(app, |v, cx| {
            let mut bigger = v.state.theme;
            for role in bigger.type_scale.roles_mut() {
                role.size_px += 2.0;
            }
            v.set_theme(bigger, cx);
            assert!(
                v.state.incremental.is_none(),
                "the old engine survived a font size change"
            );
        })
    });
    let resized = draw(cx, &editor);
    assert_ne!(
        before.1, resized.1,
        "two font sizes up and the total height did not change"
    );

    cx.update(|_, app| {
        editor.update(app, |v, cx| {
            let mut taller = v.state.theme;
            taller.type_scale.body.line_height_em += 0.2;
            v.set_theme(taller, cx);
            assert!(
                v.state.incremental.is_none(),
                "the old engine survived a body line height change"
            );
        })
    });
    let taller = draw(cx, &editor);
    assert_ne!(
        resized.1, taller.1,
        "the body line height changed but the total height did not"
    );
}

#[gpui::test]
fn switching_the_line_break_mode_relayouts_the_paragraph(cx: &mut TestAppContext) {
    let text = "这是一个很长的中文段落，它包含「引号」、括号（以及）标点：需要正确地避头尾排版，不能把标点丢到行首，也不能把开括号留在行尾！";
    let (editor, cx) = editor_with_doc(&format!("{text}\n"), cx);
    focus_editor(&editor, cx);
    let widths = [200.0, 260.0, 320.0, 360.0, 420.0, 480.0];
    let seams = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>, width: f32| {
        let (_, prepaint) = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(width), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
        let art = prepaint
            .frame
            .snapshot
            .texts
            .iter()
            .find(|piece| piece.kind == md_core::block::BlockKind::Paragraph)
            .expect("the paragraph is in the publish window")
            .art
            .clone();
        art.lines
            .iter()
            .flat_map(|line| {
                line.wrap_boundaries
                    .iter()
                    .map(|b| line.unwrapped_layout.runs[b.run_ix].glyphs[b.glyph_ix].index)
            })
            .collect::<Vec<usize>>()
    };

    let greedy: Vec<Vec<usize>> = widths
        .iter()
        .map(|width| seams(cx, &editor, *width))
        .collect();
    assert!(
        greedy.iter().any(|seams| !seams.is_empty()),
        "premise: the paragraph wrapped at some of the widths"
    );

    cx.update(|_, app| {
        editor.update(app, |v, cx| {
            let mut themed = v.state.theme;
            themed.line_break.mode = LineBreakMode::Optimal;
            v.set_theme(themed, cx);
            assert_eq!(v.state.theme.line_break.mode, LineBreakMode::Optimal);
            assert!(
                v.state.incremental.is_none(),
                "the old engine survived a line break change"
            );
        })
    });

    let optimal: Vec<Vec<usize>> = widths
        .iter()
        .map(|width| seams(cx, &editor, *width))
        .collect();
    assert!(
        greedy != optimal,
        "switching the mode left every boundary where greedy had put it"
    );
}

#[gpui::test]
fn a_selection_across_a_justified_row_reaches_the_right_margin(cx: &mut TestAppContext) {
    use md_core::block::BlockKind;
    use md_core::doc::Cursor;

    let line = "这是一个很长的中文段落，它包含「引号」、括号（以及）标点：需要正确地避头尾排版，不能把标点丢到行首，也不能把开括号留在行尾！";
    let text = format!("{line}{line}{line}");
    let (editor, cx) = editor_with_doc(&format!("{text}\n"), cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |v, cx| {
            let mut themed = v.state.theme;
            themed.line_break.mode = LineBreakMode::Justify;
            v.set_theme(themed, cx);
        });
    });
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            let block = v.state.doc.text_leaves()[0];
            let end = text.len();
            v.state.cursor = Cursor { block, offset: end };
            v.state.selection = Some((Cursor { block, offset: 0 }, Cursor { block, offset: end }));
        });
    });

    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snap = drawn.1.frame.snapshot;
    let piece = snap
        .texts
        .iter()
        .find(|piece| piece.kind == BlockKind::Paragraph)
        .expect("the paragraph is in the publish window")
        .clone();
    let inner = piece.content_width;
    let rects = &snap.selection_device;
    assert!(
        rects.len() >= 3,
        "premise: the paragraph wrapped into several rows, got {}",
        rects.len()
    );
    for (row, rect) in rects.iter().enumerate() {
        if row + 1 == rects.len() {
            continue;
        }
        assert!(
            (rect.2 - inner).abs() < 1.0,
            "row {row} of the selection is {}px wide in a {inner}px measure",
            rect.2
        );
    }
    assert!(
        rects.last().expect("a selection rect").2 < inner - 1.0,
        "the last row of the selection was stretched to the measure"
    );
}

#[gpui::test]
fn the_engine_never_holds_a_layout_theme_the_view_has_moved_on_from(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# t\n\npara with some words\n\n- a\n- b\n", cx);
    focus_editor(&editor, cx);
    let draw = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>| {
        let _ = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
    };
    let check = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>, at: &str| {
        cx.update(|_, app| {
            let v = editor.read(app);
            let engine = v
                .state
                .incremental
                .as_ref()
                .expect("painted once, so there is an engine");
            assert_eq!(
                engine.layout_theme(),
                &v.state.theme.layout_theme(),
                "{at}: a box tree laid out with the old geometry is still in use"
            );
        });
    };

    draw(cx, &editor);
    check(cx, &editor, "initial");

    for theme in [
        DocumentTheme::one_light(),
        {
            let mut t = DocumentTheme::one_light();
            t.edge_scale = 1.25;
            t
        },
        DocumentTheme::one_dark(),
    ] {
        cx.update(|_, app| editor.update(app, |v, cx| v.set_theme(theme, cx)));
        draw(cx, &editor);
        check(cx, &editor, "after a theme change");
    }
}
