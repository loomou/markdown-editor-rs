use crate::block::{BlockId, BlockKind};
use crate::doc::Doc;
use crate::document::edit::{Caret, Command, Sel};
use crate::document::focus::FocusBias;
use crate::document::{editor_options, load_markdown};

fn image_block(doc: &Doc) -> BlockId {
    doc.text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::Image))
        .expect("image block")
}

fn other_leaf(doc: &Doc, from: BlockId) -> BlockId {
    doc.text_leaves()
        .into_iter()
        .find(|&b| b != from)
        .expect("another leaf")
}

#[test]
fn apply_is_a_no_op_in_read_only() {
    let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let caret = Caret {
        block: leaf,
        offset: 0,
    };
    doc.set_read_only(true);
    let before = doc.document.to_markdown();

    let out = doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });

    assert_eq!(out, caret);
    assert_eq!(doc.document.to_markdown(), before);
    assert!(!doc.is_dirty());
}

#[test]
fn marked_and_ime_commits_are_no_ops_in_read_only() {
    let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let caret = Caret {
        block: leaf,
        offset: 0,
    };
    doc.set_read_only(true);
    let before = doc.document.to_markdown();

    let marked = doc.apply_marked(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    let committed =
        doc.apply_ime_commit(Sel::collapsed(caret), Command::Insert { text: "Y".into() });

    assert_eq!(marked, caret);
    assert_eq!(committed, caret);
    assert_eq!(doc.document.to_markdown(), before);
    assert!(!doc.is_composing());
}

#[test]
fn undo_and_redo_return_none_in_read_only() {
    let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let caret = Caret {
        block: leaf,
        offset: 0,
    };
    let _ = doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    let typed = doc.document.to_markdown();
    assert!(doc.can_undo());

    doc.set_read_only(true);

    assert_eq!(doc.undo(), None);
    assert_eq!(doc.redo(), None);
    assert_eq!(doc.document.to_markdown(), typed);
}

#[test]
fn retarget_focus_returns_its_input_in_read_only() {
    let mut doc = Doc::new(load_markdown("a **b** c\n\nplain\n", editor_options()));
    let leaves = doc.text_leaves();
    let caret = Caret {
        block: leaves[0],
        offset: 3,
    };
    let anchor = Caret {
        block: leaves[1],
        offset: 0,
    };
    let before_text = doc.caret_text(leaves[0]).expect("caret text").to_string();

    let mut editable = Doc::new(load_markdown("a **b** c\n\nplain\n", editor_options()));
    let moved = editable.retarget_focus(caret);
    assert_ne!(moved.offset, caret.offset);

    doc.set_read_only(true);
    let revision = doc.document.revision();

    assert_eq!(doc.retarget_focus(caret), caret);
    assert_eq!(doc.retarget_focus_biased(caret, FocusBias::Right), caret);
    assert_eq!(
        doc.retarget_focus_without_block_edit(caret, FocusBias::Neutral),
        caret
    );
    assert_eq!(
        doc.retarget_focus_range(anchor, caret, FocusBias::Neutral),
        (anchor, caret)
    );
    assert_eq!(doc.document.revision(), revision);
    assert_eq!(doc.caret_text(leaves[0]), Some(before_text.as_str()));
    assert_eq!(doc.block_edit(), None);
}

#[test]
fn leaving_read_only_restores_editing() {
    let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let caret = Caret {
        block: leaf,
        offset: 0,
    };

    doc.set_read_only(true);
    let _ = doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(doc.document.to_markdown(), "hello\n");

    doc.set_read_only(false);
    let _ = doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(doc.document.to_markdown(), "Xhello\n");
}

#[test]
fn entering_read_only_collapses_the_focused_block() {
    let mut doc = Doc::new(load_markdown("a **b** c\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.retarget_focus(Caret {
        block: leaf,
        offset: 3,
    });
    let revealed = doc.caret_text(leaf).expect("caret text").to_string();
    let collapsed = doc
        .collapsed_text(leaf)
        .expect("collapsed text")
        .to_string();
    assert_ne!(revealed, collapsed);
    assert!(revealed.contains("**"));

    doc.set_read_only(true);

    assert_eq!(doc.caret_text(leaf), Some(collapsed.as_str()));
    assert_eq!(doc.block_edit(), None);
}

#[test]
fn collapse_sel_folds_the_revealed_offsets() {
    let mut doc = Doc::new(load_markdown("a **b** c\n\nplain\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let plain = other_leaf(&doc, leaf);
    let _ = doc.retarget_focus(Caret {
        block: leaf,
        offset: 3,
    });
    let revealed_len = doc.caret_text(leaf).expect("caret text").len();
    let collapsed_len = doc.collapsed_text(leaf).expect("collapsed text").len();
    assert_ne!(revealed_len, collapsed_len);

    let at_start = doc.collapse_sel(Sel::collapsed(Caret {
        block: leaf,
        offset: 0,
    }));
    let at_end = doc.collapse_sel(Sel::collapsed(Caret {
        block: leaf,
        offset: revealed_len,
    }));

    assert_eq!(at_start.head.offset, 0);
    assert_eq!(at_end.head.offset, collapsed_len);

    let elsewhere = Caret {
        block: plain,
        offset: 3,
    };
    assert_eq!(doc.collapse_sel(Sel::collapsed(elsewhere)).head, elsewhere);
}

#[test]
fn read_only_never_demotes_a_broken_image() {
    let mut doc = Doc::new(load_markdown("![a](u)\n\nafter\n", editor_options()));
    let img = image_block(&doc);
    let _ = doc.retarget_focus(Caret {
        block: img,
        offset: 0,
    });
    doc.document.replace_text(img, 0..7, "hello");
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
    let other = other_leaf(&doc, img);

    doc.set_read_only(true);
    let caret = Caret {
        block: other,
        offset: 0,
    };
    let out = doc.retarget_focus(caret);

    assert_eq!(out, caret);
    assert_eq!(doc.block_edit(), None);
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
    assert_eq!(doc.document.to_markdown().trim_end(), "hello\n\nafter");
}

#[test]
fn editing_mode_still_demotes_a_broken_image() {
    let mut doc = Doc::new(load_markdown("![a](u)\n\nafter\n", editor_options()));
    let img = image_block(&doc);
    let _ = doc.retarget_focus(Caret {
        block: img,
        offset: 0,
    });
    doc.document.replace_text(img, 0..7, "hello");
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
    let other = other_leaf(&doc, img);

    let _ = doc.retarget_focus(Caret {
        block: other,
        offset: 0,
    });

    assert_eq!(doc.kind(img), Some(BlockKind::Paragraph));
}
