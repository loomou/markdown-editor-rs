use super::support::{editor_with_doc, focus_editor};
use crate::view::overlay_diag_labels;
use gpui::TestAppContext;

#[gpui::test]
fn f5_toggles_fps_display_off_by_default(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    focus_editor(&editor, cx);
    assert!(!cx.update(|_, app| editor.read(app).state.show_fps));
    cx.simulate_keystrokes("f5");
    assert!(cx.update(|_, app| editor.read(app).state.show_fps));
    cx.simulate_keystrokes("f5");
    assert!(!cx.update(|_, app| editor.read(app).state.show_fps));
}

#[gpui::test]
fn f6_toggles_stress_redraw_off_by_default(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    focus_editor(&editor, cx);
    assert!(!cx.update(|_, app| editor.read(app).state.stress_redraw));
    cx.simulate_keystrokes("f6");
    assert!(cx.update(|_, app| editor.read(app).state.stress_redraw));
    cx.simulate_keystrokes("f6");
    assert!(!cx.update(|_, app| editor.read(app).state.stress_redraw));
}

#[test]
fn overlay_diag_labels_empty_until_toggles_on() {
    assert!(overlay_diag_labels(false, false, 60, 4.2, 0, 0).is_empty());
    assert_eq!(
        overlay_diag_labels(true, false, 60, 4.2, 12, 40),
        vec![
            "60 fps".to_string(),
            "4.2 ms".to_string(),
            "store=12".to_string(),
            "last_exact=40".to_string()
        ]
    );
    assert_eq!(
        overlay_diag_labels(false, true, 0, 0.0, 0, 0),
        vec!["stress".to_string()]
    );
    assert_eq!(
        overlay_diag_labels(true, true, 120, 8.0, 3, 1),
        vec![
            "120 fps".to_string(),
            "8.0 ms".to_string(),
            "store=3".to_string(),
            "last_exact=1".to_string(),
            "stress".to_string()
        ]
    );
}
