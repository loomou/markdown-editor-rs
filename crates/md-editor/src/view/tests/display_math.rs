use super::support::{editor_with_doc, focus_editor};
use crate::view::EditorElement;
use gpui::TestAppContext;
use gpui::{point, px, size};
use md_content::shaper::ShapePart;
use md_core::block::BlockKind;

#[gpui::test]
fn display_math_is_centered_with_equal_air_above_and_below(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("one\n\ntwo\n\n$$\n\\frac{a+b}{c}\n$$\n\nthree\n", cx);

    cx.run_until_parked();
    let (_, prepaint) = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let texts = &prepaint.frame.snapshot.texts;
    assert_eq!(
        texts.len(),
        5,
        "the four body blocks plus the trailing empty paragraph must all be in the published window"
    );
    let span = |i: usize| {
        let p = &texts[i];
        (
            p.content_origin_device.1,
            p.content_origin_device.1 + p.art.height,
        )
    };
    let math = &texts[2];
    assert_eq!(
        math.kind,
        BlockKind::Math,
        "the third block must be the math block"
    );

    let body_gap = span(1).0 - span(0).1;
    let above = span(2).0 - span(1).1;
    let below = span(3).0 - span(2).1;
    assert!(
        (above - below).abs() < 0.5,
        "the margins above and below differ: above {above}, below {below}"
    );
    assert!(
        above > body_gap,
        "the margin {above} is not wider than the paragraph gap {body_gap}"
    );

    let part = math
        .art
        .bands
        .iter()
        .flat_map(|b| &b.parts)
        .find_map(|p| match p {
            ShapePart::Math { x, width, .. } => Some((*x, *width)),
            _ => None,
        })
        .expect("the math block must lay out a math slot");
    let (x, w) = part;
    assert!(
        w > 0.0 && w < math.content_width,
        "the math width {w} must not fill the whole column"
    );
    let slack = math.content_width - w;
    assert!(
        (x - slack / 2.0).abs() < 0.5,
        "the math is not centered: x={x} width={w} column width={}",
        math.content_width
    );
}

fn lone_math_slot(art: &md_content::shaper::ShapeArtifact) -> (f64, f64, f64, f64, f64) {
    let band = art
        .bands
        .iter()
        .find(|b| b.parts.iter().any(|p| matches!(p, ShapePart::Math { .. })))
        .expect("the band that holds the math");
    assert_eq!(band.parts.len(), 1, "the math must own its band");
    let ShapePart::Math {
        x,
        width,
        paint_y,
        slot_h,
        ..
    } = &band.parts[0]
    else {
        unreachable!()
    };
    (band.height, *x, *width, *paint_y, *slot_h)
}

#[gpui::test]
fn display_math_glued_to_a_text_line_still_centers(cx: &mut TestAppContext) {
    let f = "\\sum_{n=1}^{N} n = \\frac{N(N+1)}{2}";
    let viewport = size(px(1000.0), px(700.0));

    let (col, block_x, block_above) = {
        let (editor, vcx) = editor_with_doc(&format!("text\n\n$$\n{f}\n$$\n\ntail\n"), cx);
        vcx.run_until_parked();
        let (_, prepaint) = vcx.draw(point(px(0.0), px(0.0)), viewport, |_, _| EditorElement {
            state: editor.clone(),
        });
        let t = prepaint
            .frame
            .snapshot
            .texts
            .iter()
            .find(|t| t.kind == BlockKind::Math)
            .expect("$$ separated by blank lines must parse as a Math block");
        let slot = lone_math_slot(&t.art);
        (t.content_width, slot.1, slot.3)
    };

    let (editor, vcx) = editor_with_doc(&format!("text\n$$\n{f}\n$$\n\ntail\n"), cx);
    vcx.run_until_parked();
    let (_, prepaint) = vcx.draw(point(px(0.0), px(0.0)), viewport, |_, _| EditorElement {
        state: editor.clone(),
    });
    let t = prepaint
        .frame
        .snapshot
        .texts
        .iter()
        .find(|t| t.kind == BlockKind::Math)
        .expect("$$ glued to text must also be a math block");
    assert_eq!(
        t.content_width, col,
        "the two documents must have equal column widths for x to be comparable"
    );

    let (_band_h, x, _w, above, _slot_h) = lone_math_slot(&t.art);
    assert!(
        (x - block_x).abs() < 0.5,
        "not centered like the block math: glued x={x}, blank-line x={block_x}"
    );
    assert!(
        (above - block_above).abs() < 0.5,
        "the in-slot top margin must match the block math: glued {above}, blank-line {block_above}"
    );
}

#[gpui::test]
fn math_fence_enter_then_backspace_survives_the_redraw(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    focus_editor(&editor, cx);
    cx.simulate_keystrokes("$ $");
    cx.simulate_keystrokes("enter");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let b = view.state.cursor.block;
            assert_eq!(view.state.doc.kind(b), Some(BlockKind::Math));
        })
    });

    cx.simulate_keystrokes("backspace");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let b = view.state.cursor.block;
            assert_eq!(view.state.doc.kind(b), Some(BlockKind::Paragraph));
            assert_eq!(view.state.doc.text(b), Some(""));
        })
    });

    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.undo();
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let b = view.state.cursor.block;
            assert_eq!(
                view.state.doc.kind(b),
                Some(BlockKind::Math),
                "undo must put the math block back"
            );
        })
    });

    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let b = view.state.cursor.block;
            assert_eq!(view.state.doc.kind(b), Some(BlockKind::Paragraph));
        })
    });
}

#[gpui::test]
fn math_block_emptied_then_backspace_demotes(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    focus_editor(&editor, cx);
    cx.simulate_keystrokes("$ $");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("a+b");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let b = view.state.cursor.block;
            assert_eq!(view.state.doc.kind(b), Some(BlockKind::Math));
            assert_eq!(view.state.doc.text(b), Some("a+b"));
        })
    });

    for _ in 0.."a+b".len() {
        cx.simulate_keystrokes("backspace");
    }
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let b = view.state.cursor.block;
            assert_eq!(view.state.doc.kind(b), Some(BlockKind::Paragraph));
            assert_eq!(view.state.doc.text(b), Some(""));
        })
    });
}
