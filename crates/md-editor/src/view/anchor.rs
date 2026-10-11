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
        md_layout::hot_path::add_anchor_linear_scan();
        let key = (doc.identity(), doc.document.revision());
        if self.key != Some(key) {
            self.key = Some(key);
            self.table = md_core::document::heading_anchors(&doc.document);
        }
        md_core::document::find_anchor(&self.table, anchor)
    }
}

#[cfg(all(test, feature = "hot_path"))]
mod tests {
    use super::AnchorCache;
    use md_core::doc::Doc;
    use md_core::document::{editor_options, load_markdown};

    fn document() -> Doc {
        Doc::new(load_markdown("# Title\n\nbody\n", editor_options()))
    }

    #[test]
    fn resolving_an_anchor_counts_a_linear_scan() {
        let doc = document();
        md_layout::hot_path::reset();
        let mut cache = AnchorCache::default();
        assert!(
            cache.resolve(&doc, "title").is_some(),
            "the fixture heading must resolve"
        );
        assert_eq!(
            md_layout::hot_path::snapshot().anchor_linear_scan,
            1,
            "resolving one anchor must be counted as one linear scan"
        );
        cache.resolve(&doc, "title");
        assert_eq!(
            md_layout::hot_path::snapshot().anchor_linear_scan,
            2,
            "the counter must keep counting when the heading table is already cached"
        );
    }
}
