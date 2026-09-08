use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};
use pulldown_cmark::{Event, Parser};

fn display_math_count(source: &str) -> usize {
    Parser::new_ext(source, editor_options())
        .filter(|e| matches!(e, Event::DisplayMath(_)))
        .count()
}

#[test]
fn cross_math_delete_keeps_unselected_suffix() {
    let mut d = Doc::new(load_markdown(
        "$$abc$$\n\ndrop $y$ tail\n",
        editor_options(),
    ));
    let ids = d.text_leaves();
    assert_eq!(ids.len(), 2);
    assert_eq!(d.kind(ids[0]), Some(md_core::block::BlockKind::Math));
    d.apply(
        Sel {
            anchor: Caret {
                block: ids[0],
                offset: 1,
            },
            head: Caret {
                block: ids[1],
                offset: 5,
            },
        },
        Command::DeleteForward,
    );
    let saved = d.document.to_markdown();
    println!("saved={saved:?}");
    assert!(
        saved.contains("tail"),
        "unselected suffix must survive even when Math rejects the join"
    );
    assert!(
        !saved.contains("bc"),
        "the selected math body must be deleted"
    );
    assert!(
        !saved.contains("drop"),
        "the selected paragraph prefix must be deleted"
    );
}

#[test]
fn merging_into_math_preserves_serializable_math() {
    let mut d = Doc::new(load_markdown(
        "$$abc$$\n\ndrop $y$ tail\n",
        editor_options(),
    ));
    let block = d.text_leaves()[1];
    d.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::DeleteBackward,
    );
    let saved = d.document.to_markdown();
    println!("saved={saved:?}");
    assert_eq!(display_math_count(&saved), 1);
}

#[test]
fn enter_at_math_start_preserves_serializable_math() {
    let mut d = Doc::new(load_markdown("$$abc$$\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    d.apply(Sel::collapsed(Caret { block, offset: 0 }), Command::Break);
    let saved = d.document.to_markdown();
    println!("saved={saved:?}");
    assert_eq!(display_math_count(&saved), 1);
}

#[test]
fn enter_mid_math_keeps_one_formula() {
    let mut d = Doc::new(load_markdown("$$a+b$$\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    let off = d.text(block).unwrap().find("b").unwrap();
    d.apply(Sel::collapsed(Caret { block, offset: off }), Command::Break);
    let saved = d.document.to_markdown();
    println!("mid={saved:?}");
    assert_eq!(
        display_math_count(&saved),
        1,
        "a mid-body newline is legal formula content"
    );
}

#[test]
fn enter_at_math_tail_preserves_serializable_math() {
    let mut d = Doc::new(load_markdown("$$abc$$\n", editor_options()));
    let block = d.first_text_leaf().unwrap();
    let len = d.text(block).unwrap().len();
    d.apply(Sel::collapsed(Caret { block, offset: len }), Command::Break);
    let saved = d.document.to_markdown();
    println!("tail={saved:?}");
    assert_eq!(display_math_count(&saved), 1);
}

#[test]
fn merging_next_into_math_is_rejected_too() {
    let mut d = Doc::new(load_markdown(
        "$$abc$$\n\ndrop $y$ tail\n",
        editor_options(),
    ));
    let math = d.first_text_leaf().unwrap();
    let len = d.text(math).unwrap().len();
    d.apply(
        Sel::collapsed(Caret {
            block: math,
            offset: len,
        }),
        Command::DeleteForward,
    );
    let saved = d.document.to_markdown();
    println!("forward={saved:?}");
    assert_eq!(
        display_math_count(&saved),
        1,
        "the paragraph with $ must not be merged into the math body"
    );
}
