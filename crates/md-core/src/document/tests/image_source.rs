use crate::block::{BlockId, BlockKind};
use crate::doc::Doc;
use crate::document::change::DocChange;
use crate::document::edit::{Caret, Command, Sel};
use crate::document::{Document, PasteIntent, editor_options, load_markdown};

fn image_block(doc: &Document) -> BlockId {
    let mut found = None;
    doc.for_each_text_leaf(|id, _| {
        if doc.kind(id) == Some(BlockKind::Image) {
            found = Some(id);
            false
        } else {
            true
        }
    });
    found.expect("image block")
}

fn dest_of(doc: &Document, block: BlockId) -> Option<&str> {
    let id = doc.live_id(block)?;
    doc.link_dest(doc.extra(id).image_dest()?)
}

fn source_of(doc: &Document, block: BlockId) -> &str {
    doc.block_source(doc.live_id(block).expect("live"))
}

fn alt_of(doc: &Document, block: BlockId) -> &str {
    doc.display(doc.live_id(block).expect("live"))
}

#[test]
fn lone_image_splits_into_alt_and_source() {
    let doc = load_markdown("![a](u)\n", editor_options());
    let img = image_block(&doc);
    assert_eq!(source_of(&doc, img), "![a](u)");
    assert_eq!(alt_of(&doc, img), "a");
    assert_eq!(dest_of(&doc, img), Some("u"));
}

#[test]
fn caret_in_image_block_enters_edit_state() {
    let mut doc = load_markdown("![a](u)\n", editor_options());
    let img = image_block(&doc);
    doc.retarget_inline_focus(Caret {
        block: img,
        offset: 0,
    });
    assert_eq!(doc.block_edit(), Some(img));
}

#[test]
fn editing_the_alt_keeps_the_url() {
    let mut doc = load_markdown("![a](u)\n", editor_options());
    let img = image_block(&doc);
    doc.replace_text(img, 3..3, "b");
    assert_eq!(source_of(&doc, img), "![ab](u)");
    assert_eq!(alt_of(&doc, img), "ab");
    assert_eq!(dest_of(&doc, img), Some("u"));
    assert_eq!(doc.to_markdown().trim_end(), "![ab](u)");
}

#[test]
fn undo_and_redo_use_the_same_block_source_strategy() {
    let mut doc = Doc::new(load_markdown("![a](u)\n", editor_options()));
    let img = image_block(&doc.document);
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: img,
            offset: 3,
        }),
        Command::Insert { text: "b".into() },
    );
    assert_eq!(source_of(&doc.document, img), "![ab](u)");

    let _ = doc.undo().expect("undo image edit");
    assert_eq!(source_of(&doc.document, img), "![a](u)");
    assert_eq!(dest_of(&doc.document, img), Some("u"));

    let _ = doc.redo().expect("redo image edit");
    assert_eq!(source_of(&doc.document, img), "![ab](u)");
    assert_eq!(dest_of(&doc.document, img), Some("u"));
}

#[test]
fn editing_the_url_repoints_the_image() {
    let mut doc = load_markdown("![a](u)\n", editor_options());
    let img = image_block(&doc);
    doc.replace_text(img, 5..6, "v/w.png");
    assert_eq!(source_of(&doc, img), "![a](v/w.png)");
    assert_eq!(alt_of(&doc, img), "a");
    assert_eq!(dest_of(&doc, img), Some("v/w.png"));
}

#[test]
fn half_typed_source_keeps_the_kind_and_drops_the_dest() {
    let mut doc = load_markdown("![a](u)\n", editor_options());
    let img = image_block(&doc);
    doc.replace_text(img, 4..7, "");
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
    assert_eq!(source_of(&doc, img), "![a]");
    assert_eq!(alt_of(&doc, img), "");
    assert_eq!(dest_of(&doc, img), None);
    assert_eq!(doc.to_markdown().trim_end(), "![a]");
}

#[test]
fn source_survives_a_full_round_trip_through_a_broken_state() {
    let mut doc = load_markdown("before\n\n![a](u)\n\nafter\n", editor_options());
    let img = image_block(&doc);
    doc.replace_text(img, 7..7, " hello");
    assert_eq!(source_of(&doc, img), "![a](u) hello");
    let md = doc.to_markdown();
    assert!(md.contains("![a](u) hello"), "md={md:?}");
    let reloaded = load_markdown(&md, editor_options());
    assert_eq!(super::support::kind_count(&reloaded, BlockKind::Image), 0);
}

#[test]
fn backspace_uses_source_coordinates() {
    let mut doc = load_markdown("![a](u)\n", editor_options());
    let img = image_block(&doc);
    let (_, caret) = doc.delete_back(img, 7);
    assert_eq!(caret, 6);
    assert_eq!(source_of(&doc, img), "![a](u");
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
}

#[test]
fn delete_forward_uses_source_coordinates() {
    let mut doc = load_markdown("![a](€path.png)\n", editor_options());
    let img = image_block(&doc);
    let path = source_of(&doc, img).find("€path").expect("path");
    let _ = crate::document::edit::apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: img,
            offset: path,
        }),
        Command::DeleteForward,
    );

    assert_eq!(source_of(&doc, img), "![a](path.png)");
    assert_eq!(dest_of(&doc, img), Some("path.png"));
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
}

#[test]
fn merging_into_an_image_returns_a_source_coordinate() {
    let mut doc = load_markdown("![alt](url)\n\nb\n", editor_options());
    let image = image_block(&doc);
    let paragraph = doc
        .text_leaves()
        .into_iter()
        .find(|&block| doc.text_of(block) == Some("b"))
        .expect("paragraph");
    let image_source_len = source_of(&doc, image).len();

    let joined = crate::document::edit::apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: paragraph,
            offset: 0,
        }),
        Command::DeleteBackward,
    );

    assert_eq!(joined.block, image);
    assert_eq!(joined.offset, image_source_len);
    assert_eq!(source_of(&doc, image), "![alt](url)b");

    let typed = crate::document::edit::apply(
        &mut doc,
        Sel::collapsed(joined),
        Command::Insert { text: "x".into() },
    );
    assert_eq!(typed.offset, image_source_len + 1);
    assert_eq!(source_of(&doc, image), "![alt](url)xb");
}

fn leave(doc: &mut Document, from: BlockId) {
    let other = doc
        .text_leaves()
        .into_iter()
        .find(|&b| b != from)
        .expect("another leaf");
    doc.retarget_inline_focus(Caret {
        block: other,
        offset: 0,
    });
}

#[test]
fn leaving_a_broken_image_falls_back_to_a_paragraph() {
    let mut doc = load_markdown("![a](u)\n\nafter\n", editor_options());
    let img = image_block(&doc);
    doc.retarget_inline_focus(Caret {
        block: img,
        offset: 0,
    });
    doc.replace_text(img, 0..7, "hello");
    assert_eq!(doc.kind(img), Some(BlockKind::Image));

    leave(&mut doc, img);
    assert_eq!(doc.kind(img), Some(BlockKind::Paragraph));
    assert_eq!(doc.block_edit(), None);
    assert_eq!(alt_of(&doc, img), "hello");
    assert_eq!(source_of(&doc, img), "hello");
    assert_eq!(dest_of(&doc, img), None);
    assert_eq!(doc.to_markdown().trim_end(), "hello\n\nafter");
}

#[test]
fn leaving_an_intact_image_changes_nothing() {
    let mut doc = load_markdown("![a](u)\n\nafter\n", editor_options());
    let img = image_block(&doc);
    doc.retarget_inline_focus(Caret {
        block: img,
        offset: 0,
    });
    doc.replace_text(img, 3..3, "b");
    let _ = doc.take_changes();

    leave(&mut doc, img);
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
    assert_eq!(dest_of(&doc, img), Some("u"));
    assert!(doc.take_changes().changes.is_empty());
}

#[test]
fn settling_emits_both_attrs_and_text_changes() {
    let mut doc = load_markdown("![a](u)\n\nafter\n", editor_options());
    let img = image_block(&doc);
    doc.retarget_inline_focus(Caret {
        block: img,
        offset: 0,
    });
    doc.replace_text(img, 0..7, "hello");
    let _ = doc.take_changes();

    let before = doc.revision();
    leave(&mut doc, img);
    let id = doc.live_id(img).expect("live");
    let changes = doc.take_changes();
    assert_eq!(changes.before_revision, before);
    assert_eq!(changes.after_revision, before + 1);
    assert_eq!(doc.revision(), before + 1);
    assert!(
        changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::AttrsChanged { node, .. } if *node == id)),
        "{changes:?}"
    );
    assert!(
        changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::TextChanged { node, .. } if *node == id)),
        "{changes:?}"
    );
}

#[test]
fn pasted_image_keeps_source_after_leaving() {
    let mut doc = load_markdown("hello\n", editor_options());
    let host = doc.text_leaves()[0];
    let off = doc.text_of(host).unwrap().len();
    let (_, img, _) = doc.paste(
        host,
        off..off,
        "![cat](images/cat.png)\n",
        PasteIntent::IndependentFragment,
    );
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
    assert_eq!(source_of(&doc, img), "![cat](images/cat.png)");
    assert_eq!(dest_of(&doc, img), Some("images/cat.png"));

    doc.retarget_inline_focus(Caret {
        block: img,
        offset: 0,
    });
    leave(&mut doc, img);
    assert_eq!(doc.kind(img), Some(BlockKind::Image));
    assert_eq!(source_of(&doc, img), "![cat](images/cat.png)");
    assert_eq!(dest_of(&doc, img), Some("images/cat.png"));
    assert!(
        doc.to_markdown().contains("![cat](images/cat.png)"),
        "md={:?}",
        doc.to_markdown()
    );
}

#[test]
fn settling_does_not_leave_a_focus_behind() {
    let mut doc = load_markdown("![a](u)\n\nafter\n", editor_options());
    let img = image_block(&doc);
    doc.retarget_inline_focus(Caret {
        block: img,
        offset: 0,
    });
    doc.replace_text(img, 0..7, "*em*");
    leave(&mut doc, img);
    assert_eq!(doc.kind(img), Some(BlockKind::Paragraph));
    assert_eq!(alt_of(&doc, img), "em");
    assert_eq!(source_of(&doc, img), "*em*");
}

#[test]
fn undo_after_leaving_broken_image_restores_image_kind() {
    let mut doc = Doc::new(load_markdown("![alt](url)\n\nafter\n", editor_options()));
    let leaves = doc.text_leaves();
    let caret = doc.retarget_focus(Caret {
        block: leaves[0],
        offset: 0,
    });
    let _ = doc.apply(Sel::collapsed(caret), Command::DeleteForward);
    doc.retarget_focus(Caret {
        block: leaves[1],
        offset: 0,
    });
    assert_eq!(doc.kind(leaves[0]), Some(BlockKind::Paragraph));
    doc.undo().unwrap();
    assert_eq!(doc.document.to_markdown(), "![alt](url)\n\nafter\n");
    assert_eq!(
        doc.kind(leaves[0]),
        Some(BlockKind::Image),
        "kind not restored: {:?}",
        doc.document.to_markdown()
    );
    assert_eq!(dest_of(&doc.document, leaves[0]), Some("url"));
}
