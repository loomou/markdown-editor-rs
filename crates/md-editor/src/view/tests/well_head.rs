use super::support::editor_with_doc;
use crate::view::EditorElement;
use crate::view::draw::well_head_geom;
use gpui::TestAppContext;
use gpui::{point, px, size};
use md_core::block::BlockKind;
use md_render::snapshot::TextPiece;

fn code_piece_and_card(
    editor: &gpui::Entity<crate::view::EditorView>,
    cx: &mut gpui::VisualTestContext,
) -> (TextPiece, (f32, f32, f32, f32)) {
    let (_, prepaint) = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let frame = &*prepaint.frame.snapshot;
    let piece = frame
        .texts
        .iter()
        .find(|t| t.kind == BlockKind::CodeBlock && !t.edit_source)
        .expect("the code block must be in the first frame")
        .clone();
    let card = frame
        .decorations
        .iter()
        .find(|d| d.hit_block == piece.block && d.role == piece.box_id.role)
        .map(|d| {
            (
                d.rect_device.0 as f32,
                d.rect_device.1 as f32,
                d.rect_device.2 as f32,
                d.rect_device.3 as f32,
            )
        })
        .expect("the code block's well-card decoration must match the piece");
    (piece, card)
}

#[gpui::test]
fn well_head_aligns_with_the_well_card_across_densities(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("```rust\nfn x() {}\n```\n", cx);
    cx.run_until_parked();
    for k in [1.0, 0.8, 1.25] {
        cx.update(|_, app| {
            editor.update(app, |v, cx| {
                let mut t = v.state.theme;
                t.edge_scale = k;
                v.set_theme(t, cx);
            })
        });
        cx.run_until_parked();
        let (_, card) = code_piece_and_card(&editor, cx);
        let head = {
            let mut theme = md_theme::DocumentTheme::one_dark();
            theme.edge_scale = k;
            well_head_geom(card, &theme).head
        };
        assert_eq!(
            (head.0, head.1, head.2),
            (card.0, card.1, card.2),
            "at density {k} the head bar ({head:?}) does not hug the well card ({card:?})"
        );
        let head_h = {
            let theme = md_theme::DocumentTheme::one_dark();
            theme.decoration.well_head_h as f32
        };
        assert_eq!(head.3, head_h);
    }
}
