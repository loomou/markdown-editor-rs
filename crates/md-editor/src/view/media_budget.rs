use super::EditorView;
use md_content::images::ReadyImage;
use md_content::media::{self, BytePermit};

/// Hand a delivered raster the resident lease its job permit converts into. The
/// estimate that admitted the job covers the decode and the scratch buffers as
/// well as the output, so only the pixels that survive become resident bytes;
/// the rest of the permit returns to the job budget when it drops here.
pub(super) fn deliver(
    result: Result<ReadyImage, md_content::Error>,
    permit: BytePermit,
) -> Result<ReadyImage, md_content::Error> {
    match result {
        Ok(image) => {
            let bytes = image.bytes;
            Ok(image.with_lease(permit.into_lease(bytes)))
        }
        Err(err) => {
            drop(permit);
            Err(err)
        }
    }
}

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

    pub fn media_ledger(&self, cx: &gpui::App) -> media::Account {
        let mut account = self.media_account();
        if let Some(coordinator) = cx.try_global::<media::MediaCoordinator>() {
            account.resident_bytes = coordinator.resident();
            account.resident_limit = coordinator.resident_limit();
            let refused = coordinator.refusals();
            account.refused_source = refused[media::JobKind::Source.index()];
            account.refused_display = refused[media::JobKind::Display.index()];
            account.refused_zoom = refused[media::JobKind::Zoom.index()];
            account.refused_mermaid = refused[media::JobKind::Mermaid.index()];
            account.refused_math = refused[media::JobKind::Math.index()];
            account.refused_cool = refused[media::JobKind::Cool.index()];
            account.refused_thaw = refused[media::JobKind::Thaw.index()];
            account.clamped = coordinator.clamped();
        }
        if let Some(usage) = cx.try_global::<media::MediaUsage>() {
            account.gpu_pending = usage.pending();
            account.gpu_released_after_wait = usage.released_after_wait();
            account.gpu_released_immediately = usage.released_immediately();
        }
        account
    }
}
