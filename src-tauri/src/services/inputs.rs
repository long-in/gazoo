//! ControlNet の参照画像(入力画像)の取り込みと保管(docs/10-assist-features.md §6)。
//!
//! - 取り込み時に PNG へ再エンコードして `{images_dir}/inputs/<uuid>.png` に保存する
//!   (メタデータを除去し、元ファイルが後で消えても「この設定で生成」で再現できるようにする)。
//! - `params_json` には保存先からの相対パス(`inputs/<uuid>.png`)だけを持つ。
//! - ファイルを読むときは、相対パスが `is_valid_input_path` を満たすものだけを保存先に結合する。

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{ImageFormat, ImageReader};
use serde::Serialize;

use crate::AppPaths;
use crate::db::Db;
use crate::domain::params::is_valid_input_path;
use crate::error::{AppError, AppResult};
use crate::services::images::ImageService;
use crate::services::jobs::join_rel;
use crate::services::settings;

/// 取り込める元ファイルの最大サイズ。
pub const MAX_INPUT_BYTES: u64 = 20 * 1024 * 1024;
/// 取り込める画像の長辺の上限(px)。
pub const MAX_INPUT_SIDE: u32 = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputImage {
    /// 画像保存先からの相対パス(`GenerationParams.controlnet.units[].image` に入れる)。
    pub path: String,
    /// 絶対パス(フロントが `convertFileSrc` で表示する)。
    pub file_path: String,
    pub width: u32,
    pub height: u32,
}

fn invalid(msg: &str) -> AppError {
    AppError::validation("image", msg)
}

/// 画像を検証し、PNG に再エンコードして `(PNG, 幅, 高さ)` を返す。
fn normalize_to_png(bytes: &[u8]) -> AppResult<(Vec<u8>, u32, u32)> {
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(invalid("画像のサイズが大きすぎます(20MB まで)"));
    }
    // 先に寸法だけ読み、巨大な画像は展開せずに断る
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| invalid("画像を読み込めません"))?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|_| invalid("PNG または JPEG の画像を選択してください"))?;
    if w == 0 || h == 0 || w.max(h) > MAX_INPUT_SIDE {
        return Err(invalid("画像の長辺は 8192px までです"));
    }
    let img = image::load_from_memory(bytes)
        .map_err(|_| invalid("PNG または JPEG の画像を選択してください"))?;
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| AppError::Storage(e.to_string()))?;
    Ok((out, w, h))
}

/// 検証済みの相対パスを絶対パスにする。不正なパスは `VALIDATION`。
pub(crate) fn input_abs(root: &Path, rel: &str) -> AppResult<PathBuf> {
    if !is_valid_input_path(rel) {
        return Err(invalid("参照画像のパスが不正です"));
    }
    Ok(join_rel(root, rel))
}

/// 参照画像のバイト列を読む。無ければ `IMAGE_NOT_FOUND`。
pub(crate) async fn read_input(root: PathBuf, rel: String) -> AppResult<Vec<u8>> {
    let path = input_abs(&root, &rel)?;
    tokio::task::spawn_blocking(move || {
        std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::ImageNotFound(format!("参照画像がありません: {rel}"))
            } else {
                AppError::Storage(format!("{}: {e}", path.display()))
            }
        })
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
}

/// 正規化した PNG を `inputs/` に保存する(同期)。
fn store(root: &Path, bytes: &[u8]) -> AppResult<InputImage> {
    let (png, width, height) = normalize_to_png(bytes)?;
    let rel = format!("inputs/{}.png", uuid::Uuid::now_v7());
    let abs = join_rel(root, &rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Storage(format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(&abs, &png).map_err(|e| AppError::Storage(format!("{}: {e}", abs.display())))?;
    Ok(InputImage {
        path: rel,
        file_path: abs.to_string_lossy().into_owned(),
        width,
        height,
    })
}

pub struct InputService {
    db: Db,
    paths: AppPaths,
    images: Arc<ImageService>,
}

impl InputService {
    pub fn new(db: Db, paths: AppPaths, images: Arc<ImageService>) -> Arc<Self> {
        Arc::new(Self { db, paths, images })
    }

    async fn root(&self) -> AppResult<PathBuf> {
        let s = self.db.call(|c| settings::load(c)).await?;
        Ok(self.paths.resolve_images_dir(&s))
    }

    async fn import_bytes(&self, bytes: Vec<u8>) -> AppResult<InputImage> {
        let root = self.root().await?;
        tokio::task::spawn_blocking(move || store(&root, &bytes))
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
    }

    /// ファイル選択ダイアログで選ばれたファイルを取り込む(パスは Rust 側のダイアログ由来)。
    pub async fn import_file(&self, src: PathBuf) -> AppResult<InputImage> {
        let bytes = tokio::task::spawn_blocking(move || -> AppResult<Vec<u8>> {
            let meta = std::fs::metadata(&src)
                .map_err(|e| AppError::Storage(format!("{}: {e}", src.display())))?;
            if meta.len() > MAX_INPUT_BYTES {
                return Err(invalid("画像のサイズが大きすぎます(20MB まで)"));
            }
            std::fs::read(&src).map_err(|e| AppError::Storage(format!("{}: {e}", src.display())))
        })
        .await
        .map_err(|e| AppError::internal(e.to_string()))??;
        self.import_bytes(bytes).await
    }

    /// ギャラリーの画像を参照画像として取り込む。
    pub async fn import_gallery(&self, image_id: &str) -> AppResult<InputImage> {
        let path = self.images.original_path(image_id).await?;
        self.import_file(path).await
    }

    /// 既存の参照画像の情報(存在確認を兼ねる)。
    pub async fn get(&self, rel: &str) -> AppResult<InputImage> {
        let root = self.root().await?;
        let abs = input_abs(&root, rel)?;
        let rel_owned = rel.to_owned();
        tokio::task::spawn_blocking(move || {
            let (width, height) = ImageReader::open(&abs)
                .map_err(|_| AppError::ImageNotFound(format!("参照画像がありません: {rel_owned}")))?
                .with_guessed_format()
                .map_err(|e| AppError::Storage(e.to_string()))?
                .into_dimensions()
                .map_err(|e| AppError::Storage(e.to_string()))?;
            Ok(InputImage {
                path: rel_owned,
                file_path: abs.to_string_lossy().into_owned(),
                width,
                height,
            })
        })
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::thumbs::png_bytes;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("gazoo-inputs-{}", uuid::Uuid::now_v7()))
    }

    fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::DynamicImage::new_rgb8(w, h);
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
            .unwrap();
        out
    }

    #[test]
    fn stores_png_under_inputs_with_relative_path() {
        let root = temp_root();
        let img = store(&root, &png_bytes(300, 200)).unwrap();
        assert!(is_valid_input_path(&img.path), "{}", img.path);
        assert_eq!((img.width, img.height), (300, 200));
        let saved = std::fs::read(join_rel(&root, &img.path)).unwrap();
        assert_eq!(&saved[..4], b"\x89PNG");
        assert_eq!(img.file_path, join_rel(&root, &img.path).to_string_lossy());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn jpeg_is_reencoded_as_png() {
        let root = temp_root();
        let img = store(&root, &jpeg_bytes(64, 48)).unwrap();
        let saved = std::fs::read(join_rel(&root, &img.path)).unwrap();
        assert_eq!(&saved[..4], b"\x89PNG");
        assert_eq!((img.width, img.height), (64, 48));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_non_images_and_oversized_input() {
        for bad in [
            &b"not an image"[..],
            &[][..],
            &b"GIF89a\x01\x00\x01\x00"[..],
        ] {
            let e = normalize_to_png(bad).unwrap_err();
            assert_eq!(e.code(), "VALIDATION");
        }
        let huge = vec![0u8; (MAX_INPUT_BYTES + 1) as usize];
        assert_eq!(normalize_to_png(&huge).unwrap_err().code(), "VALIDATION");
    }

    #[test]
    fn rejects_images_with_too_long_side() {
        // 長辺が上限を超える正当な PNG(1 行だけなのでデータは小さい)
        let e = normalize_to_png(&png_bytes(MAX_INPUT_SIDE + 8, 1)).unwrap_err();
        assert_eq!(e.code(), "VALIDATION");
        assert!(e.to_string().contains("8192"), "{e}");
        // 上限ちょうどは通る
        assert!(normalize_to_png(&png_bytes(MAX_INPUT_SIDE, 1)).is_ok());
    }

    #[test]
    fn invalid_paths_never_reach_the_filesystem() {
        let root = temp_root();
        for bad in ["../x.png", "inputs/../x.png", "x.png", "inputs\\a.png"] {
            assert_eq!(
                input_abs(&root, bad).unwrap_err().code(),
                "VALIDATION",
                "{bad}"
            );
        }
        assert!(input_abs(&root, "inputs/a.png").unwrap().starts_with(&root));
    }

    #[tokio::test]
    async fn read_input_reports_missing_file() {
        let root = temp_root();
        let img = store(&root, &png_bytes(8, 8)).unwrap();
        assert!(
            !read_input(root.clone(), img.path.clone())
                .await
                .unwrap()
                .is_empty()
        );
        let e = read_input(root.clone(), "inputs/missing.png".into())
            .await
            .unwrap_err();
        assert_eq!(e.code(), "IMAGE_NOT_FOUND");
        let _ = std::fs::remove_dir_all(&root);
    }
}
