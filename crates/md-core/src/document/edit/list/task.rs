use crate::block::{BlockKind, NodeExtra};
use crate::document::Document;
use crate::document::edit::Caret;
use crate::document::edit::path::Path;

pub(crate) fn toggle_task(doc: &mut Document, path: &Path) -> Caret {
    let caret = Caret {
        block: path.leaf.index,
        offset: path.offset,
    };
    let Some(item) = path.item else {
        return caret;
    };
    let extra = match doc.extra(item).task_checked() {
        Some(checked) => NodeExtra::TaskItem { checked: !checked },
        None => NodeExtra::TaskItem { checked: false },
    };
    let before = doc.revision;
    let old_extra = doc.extra(item);
    let old_kind = BlockKind::ListItem;
    doc.set_extra(item, extra);
    let _ = doc.commit(before, vec![doc.attrs_change(item, old_kind, old_extra)]);
    caret
}

pub(crate) fn try_commit_task(doc: &mut Document, caret: Caret) -> Option<Caret> {
    let path = Path::at(doc, caret)?;
    let item = path.item?;
    if doc.extra(item).task_checked().is_some() {
        return None;
    }
    if doc.arena.get(item).and_then(|n| n.first_child) != Some(path.leaf) {
        return None;
    }
    let source = doc.leaf_source(path.leaf);
    let checked = if source.starts_with("[ ] ") {
        false
    } else if source.starts_with("[x] ") || source.starts_with("[X] ") {
        true
    } else {
        return None;
    };
    let before = doc.revision;
    let old_extra = doc.extra(item);
    let (mut changes, _) = doc.rewrite_text(path.leaf, 0..4, "");
    doc.set_extra(item, NodeExtra::TaskItem { checked });
    changes.push(doc.attrs_change(item, BlockKind::ListItem, old_extra));
    let _ = doc.commit(before, changes);
    Some(Caret {
        block: path.leaf.index,
        offset: 0,
    })
}

pub(crate) fn marker_spec(s: &str) -> Option<(bool, Option<bool>)> {
    let (len, ordered, task) = marker_prefix_spec(s)?;
    (len == s.len()).then_some((ordered, task))
}

pub(crate) fn marker_prefix_spec(s: &str) -> Option<(usize, bool, Option<bool>)> {
    for (marker, ordered, task) in [
        ("- [ ] ", false, Some(false)),
        ("- [x] ", false, Some(true)),
        ("- [X] ", false, Some(true)),
        ("+ [ ] ", false, Some(false)),
        ("+ [x] ", false, Some(true)),
        ("+ [X] ", false, Some(true)),
        ("* [ ] ", false, Some(false)),
        ("* [x] ", false, Some(true)),
        ("* [X] ", false, Some(true)),
        ("- ", false, None),
        ("+ ", false, None),
        ("* ", false, None),
        ("1. ", true, None),
    ] {
        if s.starts_with(marker) {
            return Some((marker.len(), ordered, task));
        }
    }
    None
}
