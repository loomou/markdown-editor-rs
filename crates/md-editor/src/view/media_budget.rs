use super::EditorView;
use md_content::media;

impl EditorView {
    pub fn media_account(&self) -> media::Account {
        let mut account = self.images.account();
        account.merge(self.mermaid.account());
        account.merge(self.math.account());
        if let Some(zoom) = &self.zoom_raster {
            account.zoom_bytes = account.zoom_bytes.saturating_add(zoom.image.bytes);
        }
        account
    }
}
