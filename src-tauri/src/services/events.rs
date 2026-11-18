//! フロントエンドへのイベント通知(docs/04-ipc-api.md「イベント」)。
//! テストで差し替えられるようトレイトにしている。

use tauri::{AppHandle, Emitter};

use crate::domain::job::{ImageItem, Job, ProgressEvent};

pub trait EventSink: Send + Sync {
    fn job_updated(&self, job: &Job);
    fn job_progress(&self, progress: &ProgressEvent);
    fn image_created(&self, image: &ImageItem);
    fn image_deleted(&self, ids: &[String]);
}

pub struct TauriSink(pub AppHandle);

impl EventSink for TauriSink {
    fn job_updated(&self, job: &Job) {
        let _ = self.0.emit("job://updated", job);
    }
    fn job_progress(&self, progress: &ProgressEvent) {
        let _ = self.0.emit("job://progress", progress);
    }
    fn image_created(&self, image: &ImageItem) {
        let _ = self.0.emit("image://created", image);
    }
    fn image_deleted(&self, ids: &[String]) {
        let _ = self
            .0
            .emit("image://deleted", serde_json::json!({ "ids": ids }));
    }
}
