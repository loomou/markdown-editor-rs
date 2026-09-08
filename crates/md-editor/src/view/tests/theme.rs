use super::support::{editor_with_doc, focus_editor, place_caret};
use crate::view::{EditorElement, EditorView};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_theme::DocumentTheme;
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
