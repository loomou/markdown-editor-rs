use super::support::stop_blink;
use crate::shell::Shell;
use crate::ui::theme::TITLE_BAR_H;
use crate::view::EditorView;
use gpui::{Modifiers, TestAppContext, point, px};
use md_core::block::BlockKind;
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};

fn well_card(editor: &EditorView, kind: BlockKind) -> (f32, f32, f32, f32) {
    let snap = editor
        .state
        .last_stable
        .as_ref()
        .expect("the editor has settled a frame")
        .snapshot
        .clone();
    let piece = snap
        .texts
        .iter()
        .find(|t| t.kind == kind && !t.edit_source)
        .expect("the block is in the frame");
    let card = snap
        .decorations
        .iter()
        .find(|d| d.hit_block == piece.block && d.role == piece.box_id.role)
        .map(|d| d.rect_device)
        .expect("the block has a well card");
    (card.0 as f32, card.1 as f32, card.2 as f32, card.3 as f32)
}

fn lang_label_click(editor: &EditorView, kind: BlockKind) -> gpui::Point<gpui::Pixels> {
    let (x, y, _, _) = well_card(editor, kind);
    let d = &editor.state.theme.decoration;
    point(
        px(x + d.well_lang_left as f32 + 4.0),
        px(y + d.well_lang_top as f32 + 8.0 + TITLE_BAR_H),
    )
}

fn open_code_lang(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    let at = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        lang_label_click(editor, BlockKind::CodeBlock)
    });
    cx.simulate_click(at, Modifiers::none());
}

#[gpui::test]
fn clicking_the_code_language_opens_an_input_that_enter_saves(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("```rust\nfn x() {}\n```\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    open_code_lang(&shell, cx);
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang_rect(app).is_some(),
            "clicking the language label must open the input"
        );
    });

    cx.simulate_input("python");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.code_lang.is_none(), "enter must close the input");
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```python\nfn x() {}\n```\n"
        );
    });
}

#[gpui::test]
fn clicking_away_from_the_code_language_saves_it(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown(
                "```rust\nfn x() {}\n```\n\ntail\n",
                editor_options(),
            )),
            cx,
        )
    });
    stop_blink(&shell, cx);
    let before = cx.update(|_, app| {
        shell
            .read(app)
            .editor
            .read(app)
            .state
            .doc
            .document
            .to_markdown()
    });
    open_code_lang(&shell, cx);
    cx.simulate_input("python");

    let away = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        let (_, y, _, _) = well_card(editor, BlockKind::CodeBlock);
        point(px(120.0), px(y + 220.0 + TITLE_BAR_H))
    });
    cx.simulate_click(away, Modifiers::none());
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang.is_none(),
            "losing focus must close the input"
        );
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            before.replace("rust", "python")
        );
    });
}

#[gpui::test]
fn escape_leaves_the_code_language_alone(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("```rust\nfn x() {}\n```\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    open_code_lang(&shell, cx);
    cx.simulate_input("python");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.code_lang.is_none());
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```rust\nfn x() {}\n```\n"
        );
    });
}

#[gpui::test]
fn a_math_block_has_no_language_input(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("$$\na+b\n$$\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    cx.run_until_parked();
    let at = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        lang_label_click(editor, BlockKind::Math)
    });
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang.is_none(),
            "a math fence has no info string to edit"
        );
        assert_eq!(editor.state.doc.document.to_markdown(), "$$\na+b\n$$\n");
    });
}

#[gpui::test]
fn clearing_the_language_falls_back_to_the_plain_label(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("```rust\nfn x() {}\n```\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    open_code_lang(&shell, cx);
    cx.simulate_keystrokes("backspace");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```\nfn x() {}\n```\n"
        );
    });
}

#[gpui::test]
fn typing_mermaid_turns_the_block_into_a_diagram(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("```rust\ngraph TD;\n```\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    open_code_lang(&shell, cx);
    cx.simulate_input("mermaid");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        let block = editor.state.doc.text_leaves()[0];
        assert_eq!(editor.state.doc.kind(block), Some(BlockKind::Mermaid));
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```mermaid\ngraph TD;\n```\n"
        );
    });
}

#[gpui::test]
fn the_input_starts_from_the_info_string_that_is_on_disk(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown(
                "```rust ignore\nfn x() {}\n```\n",
                editor_options(),
            )),
            cx,
        )
    });
    stop_blink(&shell, cx);
    open_code_lang(&shell, cx);

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        let field = editor.code_lang.as_ref().expect("open").read(app);
        assert_eq!(field.text(), "rust ignore");
    });

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```rust ignore\nfn x() {}\n```\n"
        );
    });
}
