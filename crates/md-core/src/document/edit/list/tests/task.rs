use super::support::{caret, first_list, items, lead};
use crate::document::change::DocChange;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn toggle_task_flips_checked() {
    let mut doc = load_markdown("- [ ] a\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let leaf = lead(&doc, item);
    let at = caret(leaf.index, 1);
    let _ = doc.take_changes();
    let out = apply(&mut doc, Sel::collapsed(at), Command::ToggleTask);
    assert_eq!(out, at);
    assert_eq!(doc.extra(item).task_checked(), Some(true));
    let changes = doc.take_changes();
    assert!(!changes.is_structural());
    assert!(
        changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::AttrsChanged { node, .. } if *node == item))
    );
    assert!(
        !changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::TreeSpliced { .. }))
    );
    let out = apply(&mut doc, Sel::collapsed(at), Command::ToggleTask);
    assert_eq!(out, at);
    assert_eq!(doc.extra(item).task_checked(), Some(false));
}

#[test]
fn toggle_task_marks_plain_item_unchecked() {
    let mut doc = load_markdown("- a\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let leaf = lead(&doc, item);
    assert!(doc.extra(item).task_checked().is_none());
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::ToggleTask,
    );
    assert_eq!(doc.extra(item).task_checked(), Some(false));
    assert_eq!(items(&doc, list)[0], item);
}

#[test]
fn toggle_task_outside_list_is_noop() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = caret(leaf, 1);
    let rev = doc.revision;
    let _ = doc.take_changes();
    let out = apply(&mut doc, Sel::collapsed(at), Command::ToggleTask);
    assert_eq!(out, at);
    assert_eq!(doc.revision, rev);
    assert!(doc.take_changes().is_empty());
    assert_eq!(doc.text_of(leaf).unwrap(), "hello");
}
