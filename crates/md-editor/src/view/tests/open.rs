use super::support::{editor_with_doc, temp_md};
use gpui::TestAppContext;

#[gpui::test]
fn opening_a_document_hands_the_keyboard_to_the_editor(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("placeholder\n", cx);
    let path = temp_md("open-focus");
    std::fs::write(&path, "opened\n").expect("seed");

    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.open_from_path(path.clone(), window, cx)
        });
    });
    cx.run_until_parked();

    let focus = cx.update(|_, app| editor.read(app).focus.clone());
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "opening a document should hand the keyboard to the editor"
    );

    cx.simulate_input("x");
    let text = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        view.state.doc.text(block).unwrap_or("").to_string()
    });
    assert_eq!(
        text, "xopened",
        "typing straight after opening should land in the document"
    );

    let _ = std::fs::remove_file(&path);
}
