use super::EditorView;
use crate::store::recovery::Recovery;
use gpui::Context;
use std::time::Duration;

const RECOVERY_IDLE: Duration = Duration::from_secs(1);

impl EditorView {
    pub(crate) fn set_recovery(&mut self, recovery: Recovery) {
        self.recovery = Some(recovery);
    }

    pub(super) fn schedule_recovery(&mut self, cx: &mut Context<'_, Self>) {
        if self.recovery.is_none() || !self.state.doc.is_dirty() {
            return;
        }
        self.recovery_epoch = self.recovery_epoch.wrapping_add(1);
        let epoch = self.recovery_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECOVERY_IDLE).await;
            let _ = this.update(cx, |v, cx| v.fire_recovery(epoch, cx));
        })
        .detach();
    }

    pub(super) fn discard_recovery_files(&mut self, cx: &mut Context<'_, Self>) {
        self.recovery_epoch = self.recovery_epoch.wrapping_add(1);
        let Some(store) = self.recovery.clone() else {
            return;
        };
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { store.clear() })
                .await;
            if let Err(err) = result {
                tracing::warn!(error = %err, "clear recovery draft failed");
            }
        })
        .detach();
    }

    fn fire_recovery(&mut self, epoch: u64, cx: &mut Context<'_, Self>) {
        if self.recovery_epoch != epoch || !self.state.doc.is_dirty() {
            return;
        }
        let Some(store) = self.recovery.clone() else {
            return;
        };
        let snap = self.state.doc.document.write_snapshot();
        let path = self.state.doc.source_path.clone();
        cx.spawn(async move |this, cx| {
            let markdown = cx
                .background_executor()
                .spawn(async move { snap.to_markdown() })
                .await;
            let should_write = this.update(cx, |v, cx| {
                if v.recovery_epoch != epoch || !v.state.doc.is_dirty() {
                    if !v.state.doc.is_dirty() {
                        v.discard_recovery_files(cx);
                    }
                    return false;
                }
                true
            });
            if !matches!(should_write, Ok(true)) {
                return;
            }
            let result = cx
                .background_executor()
                .spawn(async move { store.write_markdown(&markdown, path.as_deref()) })
                .await;
            if let Err(err) = result {
                tracing::warn!(error = %err, "recovery draft write failed");
            }
        })
        .detach();
    }
}
