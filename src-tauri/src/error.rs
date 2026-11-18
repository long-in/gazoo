//! アプリ共通エラー。IPC には `{ code, message, details? }` としてシリアライズされる
//! (docs/02-architecture.md §3.7, docs/04-ipc-api.md)。

use serde::{Serialize, Serializer};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Validation { field: String, message: String },
    #[error("画像生成エンジンに接続できません: {0}")]
    EngineUnreachable(String),
    #[error("画像生成エンジンの応答が想定と異なります: {0}")]
    EngineBadResponse(String),
    #[error("指定したモデルが見つかりません: {0}")]
    EngineModelMissing(String),
    #[error("必要なカスタムノードが見つかりません: {0}")]
    EngineNodeMissing(String),
    #[error("画像生成に失敗しました: {0}")]
    EngineExecutionFailed(String),
    #[error("ジョブが見つかりません: {0}")]
    JobNotFound(String),
    #[error("このジョブには実行できない操作です: {0}")]
    JobInvalidState(String),
    #[error("画像が見つかりません: {0}")]
    ImageNotFound(String),
    #[error("プリセットが見つかりません: {0}")]
    PresetNotFound(String),
    #[error("database error: {0}")]
    Db(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// フロントエンドが i18n キーに使う安定したコード。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "VALIDATION",
            Self::EngineUnreachable(_) => "ENGINE_UNREACHABLE",
            Self::EngineBadResponse(_) => "ENGINE_BAD_RESPONSE",
            Self::EngineModelMissing(_) => "ENGINE_MODEL_MISSING",
            Self::EngineNodeMissing(_) => "ENGINE_NODE_MISSING",
            Self::EngineExecutionFailed(_) => "ENGINE_EXECUTION_FAILED",
            Self::JobNotFound(_) => "JOB_NOT_FOUND",
            Self::JobInvalidState(_) => "JOB_INVALID_STATE",
            Self::ImageNotFound(_) => "IMAGE_NOT_FOUND",
            Self::PresetNotFound(_) => "PRESET_NOT_FOUND",
            Self::Db(_) => "DB_ERROR",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let details = match self {
            Self::Validation { field, .. } => Some(json!({ "field": field })),
            _ => None,
        };
        json!({
            "code": self.code(),
            "message": self.to_string(),
            "details": details,
        })
        .serialize(serializer)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<rusqlite_migration::Error> for AppError {
    fn from(e: rusqlite_migration::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_message_details() {
        let v = serde_json::to_value(AppError::validation("comfyuiUrl", "invalid")).unwrap();
        assert_eq!(v["code"], "VALIDATION");
        assert_eq!(v["message"], "invalid");
        assert_eq!(v["details"]["field"], "comfyuiUrl");
    }

    #[test]
    fn non_validation_has_null_details() {
        let v = serde_json::to_value(AppError::Db("x".into())).unwrap();
        assert_eq!(v["code"], "DB_ERROR");
        assert!(v["details"].is_null());
    }
}
