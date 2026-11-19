use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, AppResult};
use crate::services::inputs::{InputImage, InputService};

/// ファイル選択ダイアログを **Rust 側** で開き、選ばれた PNG / JPEG を参照画像として取り込む。
/// パスをフロントエンドから受け取らない(WebView から任意ファイルを読ませないため)。
/// キャンセルされたら `None`。
#[tauri::command]
pub async fn input_image_pick(
    app: AppHandle,
    inputs: State<'_, Arc<InputService>>,
) -> AppResult<Option<InputImage>> {
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;
    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file
        .into_path()
        .map_err(|e| AppError::Storage(e.to_string()))?;
    inputs.import_file(path).await.map(Some)
}

/// ギャラリーの画像を参照画像として取り込む。
#[tauri::command]
pub async fn input_image_from_gallery(
    inputs: State<'_, Arc<InputService>>,
    image_id: String,
) -> AppResult<InputImage> {
    inputs.import_gallery(&image_id).await
}

/// マスクエディタが描いた PNG(リクエスト本文のバイト列)を検証して保存する(docs/13 §3.5)。
#[tauri::command]
pub async fn mask_save(
    request: tauri::ipc::Request<'_>,
    inputs: State<'_, Arc<InputService>>,
) -> AppResult<InputImage> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::validation("mask", "マスクのデータが不正です"));
    };
    inputs.save_mask(bytes.clone()).await
}

/// 既存の参照画像の情報を取得する(存在確認を兼ねる。無ければ `IMAGE_NOT_FOUND`)。
#[tauri::command]
pub async fn input_image_get(
    inputs: State<'_, Arc<InputService>>,
    path: String,
) -> AppResult<InputImage> {
    inputs.get(&path).await
}
