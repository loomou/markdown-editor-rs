use super::MarkdownExport;
use crate::document::Document;
use crate::document::arena::NodeId;

pub(crate) fn trim_end_newlines(s: &str) -> &str {
    s.trim_end_matches(['\n', '\r'])
}

pub(crate) fn phrasing_export<D: MarkdownExport>(doc: &D, id: NodeId) -> &str {
    let source = trim_end_newlines(doc.leaf_source(id));
    if source.is_empty() {
        doc.display(id)
    } else {
        source
    }
}

pub(crate) fn phrasing_source(doc: &Document, id: NodeId) -> String {
    phrasing_export(doc, id).to_string()
}
