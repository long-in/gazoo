mod commands;
mod db;
mod domain;
mod engine;
mod error;
mod services;
mod util;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

use crate::db::Db;
use crate::services::assistant::AssistantService;
use crate::services::assistant::anthropic::AnthropicClient;
use crate::services::engine::EngineService;
use crate::services::events::TauriSink;
use crate::services::images::{ImageService, OsTrash};
use crate::services::inputs::InputService;
use crate::services::jobs::JobService;
use crate::services::presets::PresetService;
use crate::services::settings::Settings;
use crate::services::storage::StorageService;

/// アプリが使うディレクトリ(docs/06-cross-platform.md §1)。
#[derive(Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
}

impl AppPaths {
    pub fn default_images_dir(&self) -> PathBuf {
        self.data_dir.join("images")
    }

    pub fn resolve_images_dir(&self, settings: &Settings) -> PathBuf {
        settings
            .images_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_images_dir())
    }
}

/// 非同期ログ書き込みのフラッシュを保証するため、アプリ終了まで保持する。
struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let log_dir = app.path().app_log_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            std::fs::create_dir_all(&log_dir)?;

            let appender = tracing_appender::rolling::daily(&log_dir, "gazoo.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            tracing_subscriber::fmt()
                .with_writer(writer)
                .with_ansi(false)
                .with_max_level(tracing::Level::INFO)
                .init();
            app.manage(LogGuard(guard));

            let db = Db::open(&data_dir.join("gazoo.db"))?;
            let paths = AppPaths { data_dir };

            // 画像保存先を asset プロトコルのスコープに許可する(それ以外は読めない)
            let settings =
                tauri::async_runtime::block_on(db.call(|c| services::settings::load(c)))?;
            let images_dir = paths.resolve_images_dir(&settings);
            std::fs::create_dir_all(&images_dir)?;
            app.asset_protocol_scope()
                .allow_directory(&images_dir, true)?;

            let engine = Arc::new(EngineService::new(&settings.comfyui_url)?);
            let jobs = JobService::new(
                db.clone(),
                Arc::clone(&engine),
                Arc::new(TauriSink(app.handle().clone())),
                paths.clone(),
            );
            let images = ImageService::new(
                db.clone(),
                Arc::new(TauriSink(app.handle().clone())),
                paths.clone(),
                Arc::new(OsTrash),
            );
            let storage = StorageService::new(db.clone(), paths.clone(), Arc::new(OsTrash));
            app.manage(Arc::new(PresetService::new(db.clone())));
            app.manage(AssistantService::new(
                db.clone(),
                AnthropicClient::new()?,
                Arc::new(TauriSink(app.handle().clone())),
            ));
            app.manage(storage);
            app.manage(engine);
            app.manage(Arc::clone(&jobs));
            app.manage(InputService::new(
                db.clone(),
                paths.clone(),
                Arc::clone(&images),
            ));
            app.manage(Arc::clone(&images));
            // サムネイル未生成の画像(以前に保存したもの)を裏で補完する
            tauri::async_runtime::spawn(async move {
                match images.backfill_thumbnails().await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(count = n, "thumbnails backfilled"),
                    Err(e) => tracing::warn!(error = %e, "thumbnail backfill failed"),
                }
            });
            tauri::async_runtime::spawn(services::engine::monitor(app.handle().clone()));
            // 前回の実行中ジョブを復旧(失敗扱い)してから、待機中ジョブの実行を再開する
            tauri::async_runtime::spawn(async move {
                if let Err(e) = jobs.recover().await {
                    tracing::error!(error = %e, "job recovery failed");
                }
                jobs.run_worker().await;
            });

            tracing::info!("gazoo started");
            app.manage(db);
            app.manage(paths);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::engine::engine_status,
            commands::engine::engine_list_models,
            commands::engine::engine_reconnect,
            commands::jobs::job_enqueue,
            commands::jobs::job_list,
            commands::jobs::job_cancel,
            commands::jobs::job_reorder,
            commands::jobs::job_retry,
            commands::jobs::job_clear_finished,
            commands::images::image_list,
            commands::images::image_get,
            commands::images::image_set_favorite,
            commands::images::image_delete,
            commands::images::image_export,
            commands::images::image_reveal,
            commands::inputs::input_image_pick,
            commands::inputs::input_image_from_gallery,
            commands::inputs::input_image_get,
            commands::presets::preset_list,
            commands::presets::preset_save,
            commands::presets::preset_delete,
            commands::storage::storage_change_images_dir,
            commands::assistant::assistant_status,
            commands::assistant::assistant_set_key,
            commands::assistant::assistant_clear_key,
            commands::assistant::assistant_update,
            commands::assistant::assistant_ask,
            commands::assistant::assistant_cancel,
            commands::assistant::assistant_conversation_list,
            commands::assistant::assistant_conversation_get,
            commands::assistant::assistant_conversation_delete,
            commands::assistant::assistant_suggestion_applied,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
