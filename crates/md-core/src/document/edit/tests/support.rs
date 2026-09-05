use crate::document::edit::{Caret, Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

pub(super) fn fresh() -> (crate::document::Document, u32) {
    let doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    (doc, leaf)
}

pub(super) fn caret(block: u32, offset: usize) -> Caret {
    Caret { block, offset }
}
pub(super) fn type_chars(doc: &mut crate::document::Document, at: Caret, s: &str) -> Caret {
    let mut c = at;
    for ch in s.chars() {
        c = apply(
            doc,
            Sel::collapsed(c),
            Command::Insert {
                text: ch.to_string(),
            },
        );
    }
    c
}

pub(super) fn has_mark(
    doc: &crate::document::Document,
    leaf: u32,
    mark: crate::inline::InlineMarks,
) -> bool {
    let id = doc.live_id(leaf).expect("live");
    doc.runs(id).iter().any(|r| r.marks.contains(mark))
}

pub(super) fn fence_open(doc: &mut crate::document::Document, leaf: u32, line: &str) -> Caret {
    apply(
        doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: line.into() },
    )
}
