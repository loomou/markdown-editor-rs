use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};

fn doc(source: &str) -> md_core::document::Document {
    load_markdown(source, editor_options())
}

fn visible(doc: &md_core::document::Document) -> Vec<String> {
    doc.text_leaves()
        .iter()
        .map(|&id| doc.text_of(id).unwrap().to_owned())
        .collect()
}

#[test]
fn reverse_outdent_caret_lands_on_head() {
    let mut doc = Doc::new(load_markdown(
        "```\n    aa\n    bb\n    cc\n```\n",
        editor_options(),
    ));
    let block = doc.first_text_leaf().unwrap();
    let len = doc.caret_text(block).unwrap().len();
    let caret = doc.apply(
        Sel {
            anchor: Caret { block, offset: len },
            head: Caret { block, offset: 5 },
        },
        Command::Outdent,
    );
    println!("text={:?}\ncaret={caret:?}", doc.text(block));
    assert_eq!(
        doc.text(block).unwrap(),
        "aa\nbb\ncc",
        "outdent itself must be correct"
    );
    assert_eq!(
        caret.offset, 1,
        "reverse-selection outdent must keep the caret at the head"
    );
}

#[test]
fn lift_does_not_reorder_parent_tail() {
    let source = "- parent\n  - child\n\n  tail\n- sibling\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let block = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.text(id) == Some("child"))
        .unwrap();
    let _caret = doc.apply(Sel::collapsed(Caret { block, offset: 0 }), Command::Outdent);
    println!(
        "after={:?}\nsaved={:?}",
        visible(&doc.document),
        doc.document.to_markdown()
    );
    assert_eq!(
        visible(&doc.document),
        ["parent", "child", "tail", "sibling"]
    );
    let saved = doc.document.to_markdown();
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(
        visible(&reloaded),
        ["parent", "child", "tail", "sibling"],
        "saved={saved:?}"
    );
    assert_eq!(reloaded.to_markdown(), saved);
    doc.undo();
    assert_eq!(
        visible(&doc.document),
        ["parent", "child", "tail", "sibling"]
    );
    assert_eq!(
        doc.document.to_markdown(),
        load_markdown(source, editor_options()).to_markdown(),
        "undo must restore the loaded state"
    );
    doc.redo();
    assert_eq!(
        visible(&doc.document),
        ["parent", "child", "tail", "sibling"]
    );
}

#[test]
fn partial_lift_does_not_reorder_parent_tail() {
    let source = "- parent\n  - a\n  - b\n\n  tail\n- sibling\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let block = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.text(id) == Some("b"))
        .unwrap();
    let _caret = doc.apply(Sel::collapsed(Caret { block, offset: 0 }), Command::Outdent);
    println!(
        "after={:?}\nsaved={:?}",
        visible(&doc.document),
        doc.document.to_markdown()
    );
    assert_eq!(
        visible(&doc.document),
        ["parent", "a", "b", "tail", "sibling"]
    );
    let saved = doc.document.to_markdown();
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(
        visible(&reloaded),
        ["parent", "a", "b", "tail", "sibling"],
        "saved={saved:?}"
    );
    doc.undo();
    assert_eq!(
        visible(&doc.document),
        ["parent", "a", "b", "tail", "sibling"]
    );
    assert_eq!(
        doc.document.to_markdown(),
        load_markdown(source, editor_options()).to_markdown(),
        "undo must restore the loaded state"
    );
    doc.redo();
    assert_eq!(
        visible(&doc.document),
        ["parent", "a", "b", "tail", "sibling"]
    );
}

#[test]
fn undo_full_nested_outdent_restores_every_item() {
    let mut d = Doc::new(load_markdown(
        "- parent\n  - a\n  - b\n- after\n",
        editor_options(),
    ));
    let before = d.document.to_markdown();
    let a = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("a"))
        .unwrap();
    let b = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("b"))
        .unwrap();
    d.apply(
        Sel {
            anchor: Caret {
                block: a,
                offset: 0,
            },
            head: Caret {
                block: b,
                offset: 1,
            },
        },
        Command::Outdent,
    );
    let after = d.document.to_markdown();
    let undo = d.undo();
    let restored = d.document.to_markdown();
    println!("before={before:?}; after={after:?}; undo={undo:?}; restored={restored:?}");
    assert!(undo.is_some());
    assert_eq!(restored, before, "item b must not vanish on undo");
}

#[test]
fn undo_batch_outdent_variants() {
    let mut d = Doc::new(load_markdown(
        "- parent\n  - a\n  - b\n  - c\n- after\n",
        editor_options(),
    ));
    let before = d.document.to_markdown();
    let a = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("a"))
        .unwrap();
    let c = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("c"))
        .unwrap();
    d.apply(
        Sel {
            anchor: Caret {
                block: c,
                offset: 0,
            },
            head: Caret {
                block: a,
                offset: 1,
            },
        },
        Command::Outdent,
    );
    assert!(d.undo().is_some());
    assert_eq!(d.document.to_markdown(), before, "reverse 3-item batch");

    let mut d = Doc::new(load_markdown(
        "- parent\n  - a\n  - b\n  - keep\n\n  tail\n- after\n",
        editor_options(),
    ));
    let before = d.document.to_markdown();
    let a = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("a"))
        .unwrap();
    let b = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("b"))
        .unwrap();
    d.apply(
        Sel {
            anchor: Caret {
                block: a,
                offset: 0,
            },
            head: Caret {
                block: b,
                offset: 1,
            },
        },
        Command::Outdent,
    );
    assert!(d.undo().is_some());
    assert_eq!(
        d.document.to_markdown(),
        load_markdown(&before, editor_options()).to_markdown(),
        "2-item batch with a parent tail"
    );
}

#[test]
fn undo_join_list_item_restores_all_child_paragraphs() {
    let mut d = Doc::new(load_markdown(
        "- a\n\n- b\n\n  c\n\n  d\n",
        editor_options(),
    ));
    let before = d.document.to_markdown();
    let b = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("b"))
        .unwrap();
    d.apply(
        Sel::collapsed(Caret {
            block: b,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    assert!(d.undo().is_some());
    let restored = d.document.to_markdown();
    println!("before={before:?}; restored={restored:?}");
    assert_eq!(restored, before, "paragraph d must not vanish on undo");
}

#[test]
fn redo_join_restores_merged_state() {
    let mut d = Doc::new(load_markdown(
        "- a\n\n- b\n\n  c\n\n  d\n",
        editor_options(),
    ));
    let b = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("b"))
        .unwrap();
    d.apply(
        Sel::collapsed(Caret {
            block: b,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    let merged = d.document.to_markdown();
    println!("merged={merged:?}");
    assert!(d.undo().is_some());
    assert!(d.redo().is_some());
    assert_eq!(
        d.document.to_markdown(),
        merged,
        "redo must restore the merged state"
    );
}

#[test]
fn redo_batch_outdent_restores_lifted_state() {
    let mut d = Doc::new(load_markdown(
        "- parent\n  - a\n  - b\n- after\n",
        editor_options(),
    ));
    let a = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("a"))
        .unwrap();
    let b = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("b"))
        .unwrap();
    d.apply(
        Sel {
            anchor: Caret {
                block: a,
                offset: 0,
            },
            head: Caret {
                block: b,
                offset: 1,
            },
        },
        Command::Outdent,
    );
    let lifted = d.document.to_markdown();
    println!("lifted={lifted:?}");
    assert!(d.undo().is_some());
    assert!(d.redo().is_some());
    assert_eq!(
        d.document.to_markdown(),
        lifted,
        "redo must restore the lifted state"
    );
}

#[test]
fn escaped_task_marker_survives_typing() {
    let mut d = Doc::new(doc("- \\[ ] hi\n"));
    let id = d.first_text_leaf().unwrap();
    let len = d.text(id).unwrap().len();
    let caret = d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: len,
        }),
        Command::Insert { text: "X".into() },
    );
    println!(
        "saved={:?}; display={:?}; caret={caret:?}",
        d.document.to_markdown(),
        d.text(id)
    );
    assert_eq!(d.document.to_markdown(), "- \\[ ] hiX\n");
    assert_eq!(d.text(id), Some("[ ] hiX"));
}

#[test]
fn escaped_continuation_list_marker_survives_typing() {
    let mut d = Doc::new(doc("first\n\\- hi\n"));
    let id = d.first_text_leaf().unwrap();
    let len = d.text(id).unwrap().len();
    d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: len,
        }),
        Command::Insert { text: "X".into() },
    );
    println!("saved={:?}", d.document.to_markdown());
    assert_eq!(d.document.to_markdown(), "first\n\\- hiX\n");
}

#[test]
fn escaped_continuation_quote_marker_survives_typing() {
    let mut d = Doc::new(doc("first\n\\> hi\n"));
    let id = d.first_text_leaf().unwrap();
    let len = d.text(id).unwrap().len();
    d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: len,
        }),
        Command::Insert { text: "X".into() },
    );
    println!("quote saved={:?}", d.document.to_markdown());
    assert_eq!(d.document.to_markdown(), "first\n\\> hiX\n");
}

#[test]
fn live_markers_still_promote_on_typing() {
    let mut d = Doc::new(doc("- hi\n"));
    let id = d.first_text_leaf().unwrap();
    d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: 0,
        }),
        Command::Insert {
            text: "[ ] ".into(),
        },
    );
    println!("live task saved={:?}", d.document.to_markdown());
    assert_eq!(d.document.to_markdown(), "- [ ] hi\n");

    let mut d = Doc::new(doc("hi\n"));
    let id = d.first_text_leaf().unwrap();
    d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: 0,
        }),
        Command::Insert { text: "- ".into() },
    );
    println!("live list saved={:?}", d.document.to_markdown());
    assert_eq!(d.document.to_markdown(), "- hi\n");

    let mut d = Doc::new(doc("hi\n"));
    let id = d.first_text_leaf().unwrap();
    d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: 0,
        }),
        Command::Insert { text: "> ".into() },
    );
    println!("live quote saved={:?}", d.document.to_markdown());
    assert_eq!(d.document.to_markdown(), "> hi\n");
}

#[test]
fn task_marker_promotion_is_consistent_for_all_unordered_markers() {
    fn task_state_after_insert(marker: &str) -> Option<bool> {
        let mut doc = Doc::new(load_markdown("", editor_options()));
        let leaf = doc.first_text_leaf().expect("one editable leaf");
        let _ = doc.apply(
            Sel::collapsed(Caret {
                block: leaf,
                offset: 0,
            }),
            Command::Insert {
                text: marker.into(),
            },
        );
        doc.document
            .preorder()
            .into_iter()
            .find_map(|id| doc.document.extra(id).task_checked())
    }
    let dash = task_state_after_insert("- [ ] ");
    let plus = task_state_after_insert("+ [ ] ");
    let star = task_state_after_insert("* [ ] ");
    eprintln!("dash={dash:?}; plus={plus:?}; star={star:?}");
    assert_eq!(dash, Some(false));
    assert_eq!(plus, dash, "a complete + task marker should promote like -");
    assert_eq!(star, dash, "a complete * task marker should promote like -");
}

#[test]
fn checked_task_marker_promotes_checked_for_all_unordered_markers() {
    fn task_state_after_insert(marker: &str) -> Option<bool> {
        let mut doc = Doc::new(load_markdown("", editor_options()));
        let leaf = doc.first_text_leaf().expect("one editable leaf");
        let _ = doc.apply(
            Sel::collapsed(Caret {
                block: leaf,
                offset: 0,
            }),
            Command::Insert {
                text: marker.into(),
            },
        );
        doc.document
            .preorder()
            .into_iter()
            .find_map(|id| doc.document.extra(id).task_checked())
    }
    for marker in ["- [x] ", "- [X] ", "+ [x] ", "+ [X] ", "* [x] ", "* [X] "] {
        assert_eq!(
            task_state_after_insert(marker),
            Some(true),
            "marker {marker:?} must promote to a checked task item"
        );
    }
}

#[test]
fn delete_list_text_keeps_unselected_warning_block() {
    let mut d = Doc::new(load_markdown("- a\n\n  > [!WARNING]\n", editor_options()));
    let before = d.document.to_markdown();
    let block = d.first_text_leaf().unwrap();
    d.apply(
        Sel {
            anchor: Caret { block, offset: 0 },
            head: Caret { block, offset: 1 },
        },
        Command::DeleteForward,
    );
    let after = d.document.to_markdown();
    let undo = d.undo();
    let restored = d.document.to_markdown();
    println!("before={before:?}; after={after:?}; undo={undo:?}; restored={restored:?}");
    assert!(
        after.contains("[!WARNING]"),
        "the warning block is outside the text selection"
    );
    assert_eq!(restored, before);
}

#[test]
fn undo_restores_all_paragraphs_after_list_unwrap() {
    let mut d = Doc::new(load_markdown("- a\n\n  b\n", editor_options()));
    let before = d.document.to_markdown();
    let leaves = d.text_leaves();
    println!("leaves={leaves:?}");
    d.apply(
        Sel {
            anchor: Caret {
                block: leaves[0],
                offset: 0,
            },
            head: Caret {
                block: leaves[1],
                offset: 1,
            },
        },
        Command::DeleteBackward,
    );
    let after = d.document.to_markdown();
    println!("after={after:?}");
    d.undo();
    let restored = d.document.to_markdown();
    println!("restored={restored:?}");
    assert_eq!(restored, before);
}

#[test]
fn undo_restores_span_external_empty_paragraph_after_unwrap() {
    let mut d = Doc::new(load_markdown("- a\n\n  b\n\n  c\n", editor_options()));
    let before = d.document.to_markdown();
    let leaves = d.text_leaves();
    println!("leaves={leaves:?}");
    d.apply(
        Sel {
            anchor: Caret {
                block: leaves[2],
                offset: 0,
            },
            head: Caret {
                block: leaves[2],
                offset: 1,
            },
        },
        Command::DeleteBackward,
    );
    println!("c emptied: {:?}", d.document.to_markdown());
    d.apply(
        Sel {
            anchor: Caret {
                block: leaves[0],
                offset: 0,
            },
            head: Caret {
                block: leaves[1],
                offset: 1,
            },
        },
        Command::DeleteBackward,
    );
    println!("ab emptied: {:?}", d.document.to_markdown());
    d.undo();
    d.undo();
    let restored = d.document.to_markdown();
    println!("restored={restored:?}");
    assert_eq!(restored, before);
}
