use md_core::block::BlockId;
use md_core::doc::Doc;
use md_core::document::HeadingAnchor;

#[derive(Default)]
pub(crate) struct AnchorCache {
    key: Option<(u64, u64)>,
    table: Vec<HeadingAnchor>,
}

impl AnchorCache {
    pub(crate) fn resolve(&mut self, doc: &Doc, anchor: &str) -> Option<BlockId> {
        let key = (doc.identity(), doc.document.revision());
        if self.key != Some(key) {
            self.key = Some(key);
            self.table = md_core::document::heading_anchors(&doc.document);
        }
        md_core::document::find_anchor(&self.table, anchor)
    }
}
