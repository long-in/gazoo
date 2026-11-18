use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::domain::job::{ImageItem, Page};
use crate::error::{AppError, AppResult};
use crate::services::images::{ImageDetail, ImageService};

#[tauri::command]
pub async fn image_list(
    images: State<'_, Arc<ImageService>>,
    query: Option<String>,
    favorites_only: Option<bool>,
    cursor: Option<String>,
    limit: Option<u32>,
) -> AppResult<Page<ImageItem>> {
    images
        .list(query, favorites_only.unwrap_or(false), cursor, limit)
        .await
}

#[tauri::command]
pub async fn image_get(images: State<'_, Arc<ImageService>>, id: String) -> AppResult<ImageDetail> {
    images.get(&id).await
}

#[tauri::command]
pub async fn image_set_favorite(
    images: State<'_, Arc<ImageService>>,
    id: String,
    value: bool,
) -> AppResult<()> {
    images.set_favorite(&id, value).await
}

#[tauri::command]
pub async fn image_delete(
    images: State<'_, Arc<ImageService>>,
    ids: Vec<String>,
    delete_files: bool,
) -> AppResult<()> {
    images.delete(ids, delete_files).await
}

#[tauri::command]
pub async fn image_export(
    images: State<'_, Arc<ImageService>>,
    id: String,
    dest_path: String,
) -> AppResult<()> {
    images.export(&id, &PathBuf::from(dest_path)).await
}

/// エクスプローラー / Finder で画像ファイルを選択した状態で開く。
#[tauri::command]
pub async fn image_reveal(images: State<'_, Arc<ImageService>>, id: String) -> AppResult<()> {
    let path = images.original_path(&id).await?;
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| AppError::Storage(e.to_string()))
}
