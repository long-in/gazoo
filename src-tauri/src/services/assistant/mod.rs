//! AI サポート(docs/11-ai-assistant.md)。キーの管理、会話の保存、Claude への問い合わせ。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::db::Db;
use crate::db::assistant::{
    self as store, AssistantStatus, Conversation, ConversationSummary, StoredSuggestion,
};
use crate::domain::params::GenerationParams;
use crate::error::{AppError, AppResult};

pub mod anthropic;
pub mod capabilities;
pub mod context;

use anthropic::{AnthropicClient, MessageRequest, StreamEnd, StreamEvent};

const MAX_QUESTION_CHARS: usize = 8000;
const MAX_KEY_CHARS: usize = 512;

/// フロントエンドへの通知(docs/04「イベント」)。テストで差し替えられるようトレイトにしている。
pub trait AssistantSink: Send + Sync {
    fn delta(&self, request_id: &str, text: &str);
    fn suggestion(&self, request_id: &str, index: usize, suggestion: &StoredSuggestion);
    fn done(&self, request_id: &str, cancelled: bool);
    fn error(&self, request_id: &str, error: &AppError);
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantPatch {
    pub model: Option<String>,
    pub consent: Option<bool>,
    pub auto_delete: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskStarted {
    pub request_id: String,
    pub conversation_id: String,
    /// この回答として保存されるメッセージの ID(提案の適用済み記録に使う)。
    pub assistant_message_id: String,
}

struct Running {
    request_id: String,
    cancel: watch::Sender<bool>,
}

pub struct AssistantService {
    db: Db,
    client: AnthropicClient,
    sink: Arc<dyn AssistantSink>,
    running: Mutex<Option<Running>>,
}

impl AssistantService {
    pub fn new(db: Db, client: AnthropicClient, sink: Arc<dyn AssistantSink>) -> Arc<Self> {
        Arc::new(Self {
            db,
            client,
            sink,
            running: Mutex::new(None),
        })
    }

    pub async fn status(&self) -> AppResult<AssistantStatus> {
        self.db.call(|c| store::status(c)).await
    }

    pub async fn set_key(&self, key: String) -> AppResult<bool> {
        let key = key.trim().to_string();
        if key.is_empty() || key.len() > MAX_KEY_CHARS || !key.chars().all(|c| c.is_ascii_graphic())
        {
            return Err(AppError::validation(
                "key",
                "API キーの形式が正しくありません",
            ));
        }
        // 無効なキー(401/403)は保存しない。ネットワーク不可のときは未検証として保存する
        let verified = self.client.verify_key(&key).await?;
        self.db
            .call(move |c| store::set_api_key(c, &key, verified))
            .await?;
        Ok(verified)
    }

    pub async fn clear_key(&self) -> AppResult<()> {
        self.db.call(|c| store::clear_api_key(c)).await
    }

    pub async fn update(&self, patch: AssistantPatch) -> AppResult<AssistantStatus> {
        self.db
            .call(move |c| {
                if let Some(m) = &patch.model {
                    store::set_model(c, m)?;
                }
                if let Some(v) = patch.consent {
                    store::set_consent(c, v)?;
                }
                if let Some(v) = patch.auto_delete {
                    store::set_auto_delete(c, v)?;
                    if v {
                        // 有効に戻した時点で、上限を超えた分をすぐ削除する(確認はフロントの責務)
                        store::enforce_limit(c)?;
                    }
                }
                store::status(c)
            })
            .await
    }

    pub async fn conversation_list(&self) -> AppResult<Vec<ConversationSummary>> {
        self.db.call(|c| store::list_conversations(c)).await
    }

    pub async fn conversation_get(&self, id: String) -> AppResult<Conversation> {
        self.db.call(move |c| store::get_conversation(c, &id)).await
    }

    /// `ids = None` はすべて削除。
    pub async fn conversation_delete(&self, ids: Option<Vec<String>>) -> AppResult<usize> {
        self.db
            .call(move |c| match ids {
                Some(ids) => store::delete_conversations(c, &ids),
                None => store::delete_all_conversations(c),
            })
            .await
    }

    pub async fn suggestion_applied(&self, message_id: String, index: usize) -> AppResult<()> {
        self.db
            .call(move |c| store::mark_suggestion_applied(c, &message_id, index))
            .await
    }

    pub fn cancel(&self, request_id: &str) {
        if let Ok(g) = self.running.lock()
            && let Some(r) = g.as_ref()
            && r.request_id == request_id
        {
            let _ = r.cancel.send(true);
        }
    }

    /// 相談を開始する。応答はイベントで流れ、完了後に会話へ保存される。
    #[cfg(test)]
    pub async fn ask(
        self: &Arc<Self>,
        conversation_id: Option<String>,
        text: String,
        params: GenerationParams,
    ) -> AppResult<AskStarted> {
        self.ask_with(conversation_id, text, params, None, None)
            .await
    }

    /// `capabilities`: 今の ComfyUI で使える機能(docs/13 §4.2)。接続状態が不明なら `None`。
    /// `models`: 選択中・導入済みのモデルのファミリー ID(docs/18 §6.2)。無ければ `None`。
    pub async fn ask_with(
        self: &Arc<Self>,
        conversation_id: Option<String>,
        text: String,
        params: GenerationParams,
        capabilities: Option<serde_json::Value>,
        models: Option<context::ModelContext>,
    ) -> AppResult<AskStarted> {
        let text = text.trim().to_string();
        if text.is_empty() || text.chars().count() > MAX_QUESTION_CHARS {
            return Err(AppError::validation(
                "text",
                format!("質問は 1〜{MAX_QUESTION_CHARS} 文字で入力してください"),
            ));
        }

        // 同時に実行できるのは 1 件
        let request_id = uuid::Uuid::now_v7().to_string();
        let (cancel_tx, mut cancel_rx) = watch::channel(false);
        {
            let mut g = self
                .running
                .lock()
                .map_err(|_| AppError::internal("lock poisoned"))?;
            if g.is_some() {
                return Err(AppError::assistant(
                    "ASSISTANT_BUSY",
                    "別の相談が実行中です。完了するか停止してください",
                ));
            }
            *g = Some(Running {
                request_id: request_id.clone(),
                cancel: cancel_tx,
            });
        }

        let prepared = self.prepare(conversation_id, &text, &params).await;
        let (conversation_id, status, key, history) = match prepared {
            Ok(p) => p,
            Err(e) => {
                self.release();
                return Err(e);
            }
        };

        let started = AskStarted {
            request_id: request_id.clone(),
            conversation_id: conversation_id.clone(),
            assistant_message_id: uuid::Uuid::now_v7().to_string(),
        };
        let me = Arc::clone(self);
        let assistant_message_id = started.assistant_message_id.clone();
        // 選択中のモデルは、カタログにあるファミリー ID だけに絞る(ファイル名・パスは送らない)
        let catalog = capabilities::catalog();
        let current_family = models
            .as_ref()
            .and_then(|m| m.family_id.clone())
            .filter(|id| catalog.is_some_and(|c| c.families.iter().any(|f| &f.id == id)));
        let models_value = match (&models, catalog) {
            (Some(m), Some(c)) => Some(m.to_value(c)),
            _ => None,
        };
        let mut messages = history;
        messages.push((
            "user".to_string(),
            context::user_content(
                &context::snapshot(&params),
                capabilities.as_ref(),
                models_value.as_ref(),
                &text,
            ),
        ));
        tokio::spawn(async move {
            let tools = context::tool_definition();
            let system = capabilities::system_prompt();
            let req = MessageRequest {
                model: &status.model,
                system: &system,
                messages: &messages,
                tools,
            };
            let mut answer = String::new();
            let mut suggestions: Vec<StoredSuggestion> = Vec::new();
            let sink = Arc::clone(&me.sink);
            let rid = request_id.clone();
            let result = me
                .client
                .stream(&key, &req, &mut cancel_rx, |ev| match ev {
                    StreamEvent::Text(t) => {
                        answer.push_str(&t);
                        sink.delta(&rid, &t);
                    }
                    StreamEvent::ToolUse { name, input } => {
                        // 回答 1 回につき、設定の提案とモデルの提案をそれぞれ 1 件まで
                        let has_params = suggestions.iter().any(|s| s.model.is_none());
                        let has_model = suggestions.iter().any(|s| s.model.is_some());
                        let accepted = if name == context::TOOL_NAME && !has_params {
                            context::sanitize(&input, &params)
                        } else if name == context::TOOL_MODEL_NAME
                            && !has_model
                            && let Some(c) = capabilities::catalog()
                        {
                            context::sanitize_model(&input, c, current_family.as_deref())
                        } else {
                            None
                        };
                        if let Some(s) = accepted {
                            sink.suggestion(&rid, suggestions.len(), &s);
                            suggestions.push(s);
                        }
                    }
                })
                .await;

            // 途中までの内容も含め、あれば保存する
            if !answer.trim().is_empty() || !suggestions.is_empty() {
                let (cid, mid, text, sugg) = (
                    conversation_id.clone(),
                    assistant_message_id,
                    answer.clone(),
                    suggestions.clone(),
                );
                if let Err(e) = me
                    .db
                    .call(move |c| {
                        store::append_message(c, &cid, &mid, "assistant", &text, None, &sugg)
                    })
                    .await
                {
                    tracing::warn!(error = %e, "assistant message save failed");
                }
            }
            match result {
                Ok(end) => me.sink.done(&request_id, end == StreamEnd::Cancelled),
                Err(e) => {
                    tracing::warn!(code = e.code(), "assistant request failed");
                    me.sink.error(&request_id, &e);
                }
            }
            me.release();
        });
        Ok(started)
    }

    fn release(&self) {
        if let Ok(mut g) = self.running.lock() {
            *g = None;
        }
    }

    /// 検証・会話の作成・質問の保存・履歴の取得を 1 回の DB 呼び出しで行う。
    async fn prepare(
        &self,
        conversation_id: Option<String>,
        text: &str,
        params: &GenerationParams,
    ) -> AppResult<(String, AssistantStatus, String, Vec<(String, String)>)> {
        let params_json = serde_json::to_string(&context::snapshot(params))?;
        let text = text.to_string();
        self.db
            .call(move |c| {
                let status = store::status(c)?;
                if !status.consent {
                    return Err(AppError::assistant(
                        "ASSISTANT_NO_CONSENT",
                        "AI サポートの利用に同意してください",
                    ));
                }
                let key = store::api_key(c)?.ok_or_else(|| {
                    AppError::assistant("ASSISTANT_NO_KEY", "API キーが登録されていません")
                })?;
                let tx = c.transaction()?;
                let cid = match conversation_id {
                    Some(id) => {
                        if !store::conversation_exists(&tx, &id)? {
                            return Err(AppError::assistant(
                                "ASSISTANT_CONVERSATION_NOT_FOUND",
                                "会話が見つかりません",
                            ));
                        }
                        id
                    }
                    None => {
                        let id = uuid::Uuid::now_v7().to_string();
                        store::create_conversation(&tx, &id, &context::title_from(&text))?;
                        id
                    }
                };
                let mid = uuid::Uuid::now_v7().to_string();
                store::append_message(&tx, &cid, &mid, "user", &text, Some(&params_json), &[])?;
                store::enforce_limit(&tx)?;
                let mut rows = store::recent_messages(&tx, &cid, context::HISTORY_LIMIT)?;
                tx.commit()?;
                // 最新の質問は、フォームの内容を付けて別に組み立てる
                rows.pop();
                Ok((cid, status, key, context::build_history(rows)))
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::domain::params::{ModelKind, sample};
    use crate::engine::comfyui::testutil::{Route, spawn};

    #[derive(Default)]
    struct Rec(Mutex<Vec<String>>);
    impl Rec {
        fn log(&self, s: String) {
            self.0.lock().unwrap().push(s);
        }
        fn all(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }
    impl AssistantSink for Rec {
        fn delta(&self, _: &str, t: &str) {
            self.log(format!("delta:{t}"));
        }
        fn suggestion(&self, _: &str, i: usize, s: &StoredSuggestion) {
            match &s.model {
                Some(m) => self.log(format!("model-suggestion:{i}:{}", m.family_id)),
                None => self.log(format!(
                    "suggestion:{i}:{}",
                    serde_json::Value::Object(s.changes.clone())
                )),
            }
        }
        fn done(&self, _: &str, c: bool) {
            self.log(format!("done:{c}"));
        }
        fn error(&self, _: &str, e: &AppError) {
            self.log(format!("error:{}", e.code()));
        }
    }

    const SSE_OK: &str = concat!(
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"steps を増やすと良いです\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"name\":\"suggest_params\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"summary\\\":\\\"増やす\\\",\\\"changes\\\":{\\\"steps\\\":40,\\\"seed\\\":1}}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    /// 設定の提案(steps)と、モデルの提案(NoobAI-XL)と、カタログ外の ID の提案(捨てられる)。
    const SSE_MODEL: &str = concat!(
        "event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

",
        "event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"アニメ調なら NoobAI-XL がおすすめです\"}}

",
        "event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

",
        "event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"name\":\"suggest_model\"}}

",
        "event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"familyId\\\":\\\"checkpoint-noobai-xl\\\",\\\"reason\\\":\\\"Danbooru タグでアニメ調を指定できるため\\\"}\"}}

",
        "event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":1}

",
        "event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"name\":\"suggest_model\"}}

",
        "event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"familyId\\\":\\\"checkpoint-animagine-xl-4\\\",\\\"reason\\\":\\\"2 件目は捨てられる\\\"}\"}}

",
        "event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":2}

",
        "event: message_stop
data: {\"type\":\"message_stop\"}

",
    );

    async fn setup(
        routes: Vec<Route>,
    ) -> (
        Arc<AssistantService>,
        Arc<Rec>,
        Db,
        crate::engine::comfyui::testutil::Mock,
    ) {
        let mock = spawn(routes).await;
        let db = Db::open_in_memory().unwrap();
        let rec = Arc::new(Rec::default());
        let svc = AssistantService::new(
            db.clone(),
            AnthropicClient::with_base(&mock.url).unwrap(),
            rec.clone(),
        );
        (svc, rec, db, mock)
    }

    async fn ready(svc: &Arc<AssistantService>) {
        svc.set_key("sk-ant-test".into()).await.unwrap();
        svc.update(AssistantPatch {
            consent: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
    }

    async fn wait_done(rec: &Arc<Rec>) {
        for _ in 0..200 {
            if rec
                .all()
                .iter()
                .any(|l| l.starts_with("done:") || l.starts_with("error:"))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("did not finish: {:?}", rec.all());
    }

    #[tokio::test]
    async fn refuses_without_consent_or_key_and_makes_no_request() {
        let (svc, _rec, _db, mock) = setup(vec![]).await;
        let p = sample(ModelKind::Sd15);
        let e = svc.ask(None, "q".into(), p.clone()).await.unwrap_err();
        assert_eq!(e.code(), "ASSISTANT_NO_CONSENT");
        svc.update(AssistantPatch {
            consent: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
        let e = svc.ask(None, "q".into(), p).await.unwrap_err();
        assert_eq!(e.code(), "ASSISTANT_NO_KEY");
        assert!(mock.requests.lock().unwrap().is_empty(), "外部通信しない");
        // 失敗しても次の相談を妨げない
        assert!(svc.running.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn set_key_verifies_and_rejects_invalid() {
        let (svc, _rec, _db, _mock) = setup(vec![Route::get("/v1/models?limit=1", "{}")]).await;
        assert!(svc.set_key("sk-ant-ok".into()).await.unwrap());
        let s = svc.status().await.unwrap();
        assert!(s.has_key && s.verified);

        let (svc, _r, _d, _m) = setup(vec![Route::seq(
            "GET /v1/models?limit=1",
            vec![(401, "{}")],
        )])
        .await;
        let e = svc.set_key("sk-ant-bad".into()).await.unwrap_err();
        assert_eq!(e.code(), "ASSISTANT_AUTH");
        assert!(
            !svc.status().await.unwrap().has_key,
            "無効なキーは保存しない"
        );

        let e = svc.set_key("has space".into()).await.unwrap_err();
        assert_eq!(e.code(), "VALIDATION");
    }

    #[tokio::test]
    async fn model_suggestion_is_validated_saved_and_the_guide_is_sent() {
        let (svc, rec, _db, mock) = setup(vec![
            Route::get("/v1/models?limit=1", "{}"),
            Route::post("/v1/messages", 200, SSE_MODEL),
        ])
        .await;
        ready(&svc).await;
        let mut p = sample(ModelKind::Sdxl);
        p.model.name = "my-private-checkpoint-v7.safetensors".into();
        let models = context::ModelContext {
            family_id: None,
            installed_families: vec![
                "checkpoint-sdxl".into(),
                "my-private-checkpoint-v7.safetensors".into(),
            ],
        };
        let started = svc
            .ask_with(None, "アニメ調にしたい".into(), p, None, Some(models))
            .await
            .unwrap();
        wait_done(&rec).await;

        // 回答 1 回につきモデルの提案は 1 件まで(2 件目は捨てる)
        let log = rec.all();
        let model_logs: Vec<_> = log
            .iter()
            .filter(|l| l.starts_with("model-suggestion:"))
            .collect();
        assert_eq!(
            model_logs,
            vec!["model-suggestion:0:checkpoint-noobai-xl"],
            "{log:?}"
        );

        let conv = svc
            .conversation_get(started.conversation_id.clone())
            .await
            .unwrap();
        let saved = &conv.messages[1].suggestions;
        assert_eq!(saved.len(), 1);
        assert_eq!(
            saved[0].model.as_ref().unwrap().family_id,
            "checkpoint-noobai-xl"
        );
        assert!(saved[0].changes.is_empty());

        // 送った内容: モデルガイドとルール(システム)、導入済みのファミリー ID。ファイル名は送らない
        let sent = {
            let reqs = mock.requests.lock().unwrap();
            reqs.iter()
                .find(|r| r.starts_with("POST /v1/messages"))
                .unwrap()
                .clone()
        };
        assert!(sent.contains("# モデルガイド") && sent.contains("[checkpoint-noobai-xl]"));
        assert!(sent.contains("suggest_model"));
        assert!(sent.contains("installedFamilies") && sent.contains("checkpoint-sdxl"));
        // 導入済みの一覧に混ぜたファイル名は、カタログに無いので捨てられる(ModelContext のテストも参照)
        assert!(!sent.contains(
            "\"installedFamilies\": [
    \"checkpoint-sdxl\",
    \"my-private"
        ));
    }

    #[tokio::test]
    async fn ask_streams_saves_conversation_and_filters_suggestion() {
        let (svc, rec, db, mock) = setup(vec![
            Route::get("/v1/models?limit=1", "{}"),
            Route::post("/v1/messages", 200, SSE_OK),
        ])
        .await;
        ready(&svc).await;
        let mut p = sample(ModelKind::Sd15);
        p.prompt = "a cat".into();
        p.steps = 20;
        p.seed = 424242;
        let started = svc.ask(None, "もっと綺麗に".into(), p).await.unwrap();
        wait_done(&rec).await;

        let log = rec.all();
        assert!(log.iter().any(|l| l.starts_with("delta:steps")));
        // seed はホワイトリスト外なので除外され、steps だけが提案される
        assert!(
            log.contains(&"suggestion:0:{\"steps\":40}".to_string()),
            "{log:?}"
        );
        assert_eq!(log.last().unwrap(), "done:false");

        let conv = svc
            .conversation_get(started.conversation_id.clone())
            .await
            .unwrap();
        assert_eq!(conv.title, "もっと綺麗に");
        assert_eq!(conv.messages.len(), 2);
        assert_eq!(conv.messages[0].role, "user");
        assert_eq!(conv.messages[1].id, started.assistant_message_id);
        assert_eq!(conv.messages[1].suggestions.len(), 1);

        // 送ったリクエストに、キー・seed・ComfyUI の URL は含まれない
        let sent = {
            let reqs = mock.requests.lock().unwrap();
            reqs.iter()
                .find(|r| r.starts_with("POST /v1/messages"))
                .unwrap()
                .clone()
        };
        assert!(sent.contains("a cat") && sent.contains("もっと綺麗に"));
        assert!(!sent.contains("424242"));
        assert!(
            !sent.contains("sk-ant-test"),
            "キーは本文に含めない(ヘッダのみ)"
        );

        // 適用済みの記録
        svc.suggestion_applied(started.assistant_message_id.clone(), 0)
            .await
            .unwrap();
        let conv = svc.conversation_get(started.conversation_id).await.unwrap();
        assert!(conv.messages[1].suggestions[0].applied);
        let _ = db;
    }

    #[tokio::test]
    async fn api_errors_map_to_codes_and_keep_user_message() {
        let (svc, rec, _db, _mock) = setup(vec![
            Route::get("/v1/models?limit=1", "{}"),
            Route::post("/v1/messages", 429, "{}"),
        ])
        .await;
        ready(&svc).await;
        let started = svc
            .ask(None, "q".into(), sample(ModelKind::Sd15))
            .await
            .unwrap();
        wait_done(&rec).await;
        assert_eq!(rec.all().last().unwrap(), "error:ASSISTANT_RATE_LIMIT");
        let conv = svc.conversation_get(started.conversation_id).await.unwrap();
        assert_eq!(
            conv.messages.len(),
            1,
            "失敗しても質問は残り、回答は保存されない"
        );
        // 完了後は再度相談できる
        for _ in 0..100 {
            if svc.running.lock().unwrap().is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(svc.running.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn busy_while_running_and_unknown_conversation_rejected() {
        let (svc, _rec, _db, _mock) = setup(vec![Route::get("/v1/models?limit=1", "{}")]).await;
        ready(&svc).await;
        let e = svc
            .ask(Some("nope".into()), "q".into(), sample(ModelKind::Sd15))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "ASSISTANT_CONVERSATION_NOT_FOUND");

        let (tx, _rx) = watch::channel(false);
        *svc.running.lock().unwrap() = Some(Running {
            request_id: "x".into(),
            cancel: tx,
        });
        let e = svc
            .ask(None, "q".into(), sample(ModelKind::Sd15))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "ASSISTANT_BUSY");
    }

    #[tokio::test]
    async fn cancel_signals_only_the_matching_request() {
        let (svc, _rec, _db, _mock) = setup(vec![]).await;
        let (tx, rx) = watch::channel(false);
        *svc.running.lock().unwrap() = Some(Running {
            request_id: "r1".into(),
            cancel: tx,
        });
        svc.cancel("other");
        assert!(!*rx.borrow());
        svc.cancel("r1");
        assert!(*rx.borrow());
    }

    #[tokio::test]
    async fn follow_up_includes_history() {
        let (svc, rec, _db, mock) = setup(vec![
            Route::get("/v1/models?limit=1", "{}"),
            Route::post("/v1/messages", 200, SSE_OK),
        ])
        .await;
        ready(&svc).await;
        let p = sample(ModelKind::Sd15);
        let first = svc.ask(None, "最初の質問".into(), p.clone()).await.unwrap();
        wait_done(&rec).await;
        for _ in 0..100 {
            if svc.running.lock().unwrap().is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        rec.0.lock().unwrap().clear();
        svc.ask(Some(first.conversation_id.clone()), "続きの質問".into(), p)
            .await
            .unwrap();
        wait_done(&rec).await;
        let reqs = mock.requests.lock().unwrap();
        let second = reqs
            .iter()
            .filter(|r| r.starts_with("POST /v1/messages"))
            .nth(1)
            .unwrap();
        assert!(second.contains("最初の質問") && second.contains("steps を増やすと良いです"));
        assert!(second.contains("続きの質問"));
    }
}
