use super::support::stop_blink;
use crate::shell::Shell;
use crate::ui::theme::TITLE_BAR_H;
use crate::view::{CursorMotion, EditorView};
use gpui::{Modifiers, TestAppContext, point, px};
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};
use md_render::snapshot::TextPiece;

fn pieces_of_block(editor: &EditorView, block: u32) -> Vec<TextPiece> {
    editor
        .state
        .last_stable
        .as_ref()
        .expect("the editor has settled a frame")
        .snapshot
        .texts
        .iter()
        .filter(|t| t.block == block)
        .cloned()
        .collect()
}

fn well_card_at(editor: &EditorView, block: u32, edit_source: bool) -> (f32, f32, f32, f32) {
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
        .find(|t| t.block == block && t.edit_source == edit_source)
        .expect("the block is in the frame");
    let card = snap
        .decorations
        .iter()
        .find(|d| d.hit_block == piece.block && d.role == piece.box_id.role)
        .map(|d| d.rect_device)
        .expect("the block has a well card");
    (card.0 as f32, card.1 as f32, card.2 as f32, card.3 as f32)
}

fn well_card(editor: &EditorView, kind: BlockKind) -> (f32, f32, f32, f32) {
    let block = editor
        .state
        .last_stable
        .as_ref()
        .expect("the editor has settled a frame")
        .snapshot
        .texts
        .iter()
        .find(|t| t.kind == kind && !t.edit_source)
        .expect("the block is in the frame")
        .block;
    well_card_at(editor, block, false)
}

fn label_click_at(editor: &EditorView, block: u32, edit_source: bool) -> gpui::Point<gpui::Pixels> {
    let (x, y, _, _) = well_card_at(editor, block, edit_source);
    let d = &editor.state.theme.decoration;
    point(
        px(x + d.well_lang_left as f32 + 4.0),
        px(y + d.well_lang_top as f32 + 8.0 + TITLE_BAR_H),
    )
}

fn lang_label_click(editor: &EditorView, kind: BlockKind) -> gpui::Point<gpui::Pixels> {
    let block = editor
        .state
        .last_stable
        .as_ref()
        .expect("the editor has settled a frame")
        .snapshot
        .texts
        .iter()
        .find(|t| t.kind == kind && !t.edit_source)
        .expect("the block is in the frame")
        .block;
    label_click_at(editor, block, false)
}

fn redraw(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, app| {
        let editor = shell.read(app).editor.clone();
        editor.update(app, |_, cx| cx.notify());
    });
    cx.run_until_parked();
}

fn caret_in(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext, leaf: usize) {
    cx.update(|_, app| {
        let editor = shell.read(app).editor.clone();
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[leaf];
            view.place_cursor(Cursor { block, offset: 0 }, CursorMotion::Move);
        });
    });
    redraw(shell, cx);
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
fn a_fence_without_an_info_string_can_gain_one(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("```\nfn x() {}\n```\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    open_code_lang(&shell, cx);
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang_rect(app).is_some(),
            "a fence with no info string must still open the input"
        );
    });

    cx.simulate_input("rust");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```rust\nfn x() {}\n```\n"
        );
    });
}

#[gpui::test]
fn an_indented_code_block_has_no_language_input(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("    fn x() {}\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    cx.run_until_parked();
    let at = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        lang_label_click(editor, BlockKind::CodeBlock)
    });
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang.is_none(),
            "an indented code block has no fence to rewrite"
        );
        assert_eq!(editor.state.doc.document.to_markdown(), "    fn x() {}\n");
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

#[gpui::test]
fn changing_the_language_to_mermaid_keeps_the_source_above_the_diagram(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("```rust\nfn x() {}\n```\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    caret_in(&shell, cx, 0);
    open_code_lang(&shell, cx);
    cx.simulate_input("mermaid");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        let block = editor.state.doc.text_leaves()[0];
        assert_eq!(editor.state.doc.kind(block), Some(BlockKind::Mermaid));
        assert_eq!(
            editor.state.doc.block_edit(),
            Some(block),
            "the caret is in the block, so it must be in block edit"
        );
        let pieces = pieces_of_block(editor, block);
        assert!(
            pieces.iter().any(|t| t.edit_source),
            "the source must stay editable above the diagram"
        );
        assert!(
            pieces.iter().any(|t| !t.edit_source),
            "the diagram needs a preview block"
        );
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```mermaid\nfn x() {}\n```\n"
        );
    });
}

#[gpui::test]
fn changing_a_mermaid_back_to_a_code_block_drops_the_preview(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown(
                "```mermaid\ngraph TD;\n```\n",
                editor_options(),
            )),
            cx,
        )
    });
    stop_blink(&shell, cx);
    caret_in(&shell, cx, 0);
    let block = cx.update(|_, app| shell.read(app).editor.read(app).state.doc.text_leaves()[0]);
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(
            pieces_of_block(editor, block).len(),
            2,
            "a mermaid under the caret is a source block plus a preview"
        );
    });

    let at = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        label_click_at(editor, block, true)
    });
    cx.simulate_click(at, Modifiers::none());
    cx.simulate_input("rust");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(editor.state.doc.kind(block), Some(BlockKind::CodeBlock));
        assert_eq!(editor.state.doc.block_edit(), None);
        let pieces = pieces_of_block(editor, block);
        assert_eq!(pieces.len(), 1, "the preview block must be gone");
        assert!(!pieces[0].edit_source);
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```rust\ngraph TD;\n```\n"
        );
    });
}

#[gpui::test]
fn the_mermaid_preview_has_no_language_input(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown(
                "```mermaid\ngraph TD;\n```\n",
                editor_options(),
            )),
            cx,
        )
    });
    stop_blink(&shell, cx);
    caret_in(&shell, cx, 0);
    let block = cx.update(|_, app| shell.read(app).editor.read(app).state.doc.text_leaves()[0]);
    let (preview, source) = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        (
            label_click_at(editor, block, false),
            label_click_at(editor, block, true),
        )
    });

    cx.simulate_click(preview, Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang.is_none(),
            "the diagram is a preview, not a fence to rewrite"
        );
    });

    cx.simulate_click(source, Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang_rect(app).is_some(),
            "the source above the diagram keeps its language input"
        );
    });
}

#[gpui::test]
fn a_mermaid_away_from_the_caret_has_no_language_input(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown(
                "```mermaid\ngraph TD;\n```\n\ntail\n",
                editor_options(),
            )),
            cx,
        )
    });
    stop_blink(&shell, cx);
    cx.run_until_parked();
    let at = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(
            editor.state.doc.block_edit(),
            None,
            "the caret is in another block, so the diagram is not being edited"
        );
        lang_label_click(editor, BlockKind::Mermaid)
    });
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang.is_none(),
            "a diagram that is not being edited is a preview, not a fence to rewrite"
        );
        assert_eq!(
            editor.state.doc.document.to_markdown(),
            "```mermaid\ngraph TD;\n```\n\ntail"
        );
    });
}

#[gpui::test]
fn a_code_block_away_from_the_caret_keeps_its_language_input(cx: &mut TestAppContext) {
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
    cx.run_until_parked();
    let at = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(
            editor.state.doc.block_edit(),
            None,
            "the caret is in another block, so the fence is not being edited"
        );
        lang_label_click(editor, BlockKind::CodeBlock)
    });
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();

    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(
            editor.code_lang_rect(app).is_some(),
            "a code block is a fence wherever the caret is, so its label stays editable"
        );
    });
}
