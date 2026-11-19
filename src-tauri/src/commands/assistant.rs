use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::db::assistant::{AssistantStatus, Conversation, ConversationSummary};
use crate::domain::params::GenerationParams;
use crate::error::AppResult;
use crate::services::assistant::{
    AskStarted, AssistantPatch, AssistantService, capabilities, context::ModelContext,
};
use crate::services::engine::EngineService;

type Svc<'a> = State<'a, Arc<AssistantService>>;

#[derive(Serialize)]
pub struct KeyResult {
    pub verified: bool,
}

#[derive(Serialize)]
pub struct DeleteResult {
    pub deleted: usize,
}

#[tauri::command]
pub async fn assistant_status(svc: Svc<'_>) -> AppResult<AssistantStatus> {
    svc.status().await
}

#[tauri::command]
pub async fn assistant_set_key(svc: Svc<'_>, key: String) -> AppResult<KeyResult> {
    Ok(KeyResult {
        verified: svc.set_key(key).await?,
    })
}

#[tauri::command]
pub async fn assistant_clear_key(svc: Svc<'_>) -> AppResult<()> {
    svc.clear_key().await
}

#[tauri::command]
pub async fn assistant_update(svc: Svc<'_>, patch: AssistantPatch) -> AppResult<AssistantStatus> {
    svc.update(patch).await
}

#[tauri::command]
pub async fn assistant_ask(
    svc: Svc<'_>,
    engine: State<'_, Arc<EngineService>>,
    conversation_id: Option<String>,
    text: String,
    params: GenerationParams,
    models: Option<ModelContext>,
) -> AppResult<AskStarted> {
    // 今の ComfyUI で使える機能も添える(通信はしない。docs/13 §4.2)
    let caps = capabilities::capabilities_json(engine.last_features().as_ref());
    svc.inner()
        .ask_with(conversation_id, text, params, caps, models)
        .await
}

#[tauri::command]
pub fn assistant_cancel(svc: Svc<'_>, request_id: String) {
    svc.cancel(&request_id);
}

#[tauri::command]
pub async fn assistant_conversation_list(svc: Svc<'_>) -> AppResult<Vec<ConversationSummary>> {
    svc.conversation_list().await
}

#[tauri::command]
pub async fn assistant_conversation_get(svc: Svc<'_>, id: String) -> AppResult<Conversation> {
    svc.conversation_get(id).await
}

/// `ids` が `null` のときはすべて削除する。
#[tauri::command]
pub async fn assistant_conversation_delete(
    svc: Svc<'_>,
    ids: Option<Vec<String>>,
) -> AppResult<DeleteResult> {
    Ok(DeleteResult {
        deleted: svc.conversation_delete(ids).await?,
    })
}

#[tauri::command]
pub async fn assistant_suggestion_applied(
    svc: Svc<'_>,
    message_id: String,
    index: usize,
) -> AppResult<()> {
    svc.suggestion_applied(message_id, index).await
}
