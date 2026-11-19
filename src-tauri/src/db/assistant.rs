//! AI サポートの設定と会話履歴の永続化(docs/11-ai-assistant.md、docs/03-data-model.md)。
//!
//! API キーは設計(Q-27)により `settings` テーブルに平文で保存する。`services::settings::Settings`
//! には含めず、このモジュールからのみ読み書きする(`settings_get` で返さないため)。

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::util::now_ms;

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
pub const MODELS: [&str; 3] = ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"];
/// 自動削除が有効なときに残す会話の最大数(Q-31)。
pub const MAX_CONVERSATIONS: i64 = 30;

const KEY_API_KEY: &str = "assistant.api_key";
const KEY_VERIFIED: &str = "assistant.key_verified";
const KEY_MODEL: &str = "assistant.model";
const KEY_CONSENT: &str = "assistant.consent";
const KEY_AUTO_DELETE: &str = "assistant.auto_delete";

fn get(conn: &Connection, key: &str) -> AppResult<Option<Value>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

fn set(conn: &Connection, key: &str, value: &Value) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![key, value.to_string(), now_ms()],
    )?;
    Ok(())
}

/// フロントに返してよい設定(キー本体を含まない)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStatus {
    pub has_key: bool,
    pub verified: bool,
    pub model: String,
    pub consent: bool,
    pub auto_delete: bool,
}

pub fn status(conn: &Connection) -> AppResult<AssistantStatus> {
    let has_key = api_key(conn)?.is_some();
    let model = get(conn, KEY_MODEL)?
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|m| MODELS.contains(&m.as_str()))
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    Ok(AssistantStatus {
        has_key,
        verified: has_key && get(conn, KEY_VERIFIED)?.and_then(|v| v.as_bool()) == Some(true),
        model,
        consent: get(conn, KEY_CONSENT)?.and_then(|v| v.as_bool()) == Some(true),
        auto_delete: get(conn, KEY_AUTO_DELETE)?
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
    })
}

pub fn api_key(conn: &Connection) -> AppResult<Option<String>> {
    Ok(get(conn, KEY_API_KEY)?
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|k| !k.is_empty()))
}

pub fn set_api_key(conn: &Connection, key: &str, verified: bool) -> AppResult<()> {
    set(conn, KEY_API_KEY, &Value::String(key.to_string()))?;
    set(conn, KEY_VERIFIED, &Value::Bool(verified))
}

pub fn clear_api_key(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "DELETE FROM settings WHERE key IN (?1, ?2)",
        [KEY_API_KEY, KEY_VERIFIED],
    )?;
    Ok(())
}

pub fn set_model(conn: &Connection, model: &str) -> AppResult<()> {
    if !MODELS.contains(&model) {
        return Err(AppError::validation("model", "選べないモデルです"));
    }
    set(conn, KEY_MODEL, &Value::String(model.to_string()))
}

pub fn set_consent(conn: &Connection, consent: bool) -> AppResult<()> {
    set(conn, KEY_CONSENT, &Value::Bool(consent))
}

pub fn set_auto_delete(conn: &Connection, on: bool) -> AppResult<()> {
    set(conn, KEY_AUTO_DELETE, &Value::Bool(on))
}

// ---- 会話 ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
}

/// アシスタントの 1 回の回答に付く、検証済みの設定変更の提案。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSuggestion {
    pub summary: String,
    pub changes: serde_json::Map<String, Value>,
    #[serde(default)]
    pub applied: bool,
    /// モデルの提案(docs/18 §6.3)。設定変更の提案では `None`。保存済みの古い提案にはない。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSuggestion>,
}

/// 「このモデルを使うとよい」という提案。`family_id` はカタログに存在するものだけが入る。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSuggestion {
    pub family_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub role: String,
    pub text: String,
    pub suggestions: Vec<StoredSuggestion>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<ConversationMessage>,
}

pub fn conversation_exists(conn: &Connection, id: &str) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM assistant_conversations WHERE id = ?1",
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub fn create_conversation(conn: &Connection, id: &str, title: &str) -> AppResult<()> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO assistant_conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![id, title, now],
    )?;
    Ok(())
}

pub fn append_message(
    conn: &Connection,
    conversation_id: &str,
    id: &str,
    role: &str,
    text: &str,
    params_json: Option<&str>,
    suggestions: &[StoredSuggestion],
) -> AppResult<()> {
    let now = now_ms();
    let suggestions_json = if suggestions.is_empty() {
        None
    } else {
        Some(serde_json::to_string(suggestions)?)
    };
    conn.execute(
        "INSERT INTO assistant_messages (id, conversation_id, role, text, params_json, suggestions_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, conversation_id, role, text, params_json, suggestions_json, now],
    )?;
    conn.execute(
        "UPDATE assistant_conversations SET updated_at = ?2 WHERE id = ?1",
        params![conversation_id, now],
    )?;
    Ok(())
}

pub fn list_conversations(conn: &Connection) -> AppResult<Vec<ConversationSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, updated_at FROM assistant_conversations ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ConversationSummary {
                id: r.get(0)?,
                title: r.get(1)?,
                updated_at: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_conversation(conn: &Connection, id: &str) -> AppResult<Conversation> {
    let title: String = conn
        .query_row(
            "SELECT title FROM assistant_conversations WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            AppError::assistant("ASSISTANT_CONVERSATION_NOT_FOUND", "会話が見つかりません")
        })?;
    let mut stmt = conn.prepare(
        "SELECT id, role, text, suggestions_json, created_at FROM assistant_messages
         WHERE conversation_id = ?1 ORDER BY created_at ASC, rowid ASC",
    )?;
    let messages = stmt
        .query_map([id], |r| {
            let raw: Option<String> = r.get(3)?;
            Ok(ConversationMessage {
                id: r.get(0)?,
                role: r.get(1)?,
                text: r.get(2)?,
                suggestions: raw
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default(),
                created_at: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Conversation {
        id: id.to_string(),
        title,
        messages,
    })
}

/// 会話に渡す直近のメッセージ(古い順)。
pub fn recent_messages(
    conn: &Connection,
    conversation_id: &str,
    limit: i64,
) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT role, text FROM assistant_messages WHERE conversation_id = ?1
         ORDER BY created_at DESC, rowid DESC LIMIT ?2",
    )?;
    let mut rows = stmt
        .query_map(params![conversation_id, limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.reverse();
    Ok(rows)
}

pub fn mark_suggestion_applied(conn: &Connection, message_id: &str, index: usize) -> AppResult<()> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT suggestions_json FROM assistant_messages WHERE id = ?1",
            [message_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let mut list: Vec<StoredSuggestion> = raw
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let Some(item) = list.get_mut(index) else {
        return Err(AppError::validation("index", "提案が見つかりません"));
    };
    item.applied = true;
    conn.execute(
        "UPDATE assistant_messages SET suggestions_json = ?2 WHERE id = ?1",
        params![message_id, serde_json::to_string(&list)?],
    )?;
    Ok(())
}

/// 指定した会話を削除する(メッセージは CASCADE)。削除した件数を返す。
pub fn delete_conversations(conn: &mut Connection, ids: &[String]) -> AppResult<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    for id in ids {
        n += tx.execute("DELETE FROM assistant_conversations WHERE id = ?1", [id])?;
    }
    tx.commit()?;
    Ok(n)
}

pub fn delete_all_conversations(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute("DELETE FROM assistant_conversations", [])?)
}

/// 自動削除が有効なとき、更新が古い会話から超過分を削除する。削除した件数を返す。
pub fn enforce_limit(conn: &Connection) -> AppResult<usize> {
    if !status(conn)?.auto_delete {
        return Ok(0);
    }
    Ok(conn.execute(
        "DELETE FROM assistant_conversations WHERE id NOT IN (
           SELECT id FROM assistant_conversations ORDER BY updated_at DESC, id DESC LIMIT ?1
         )",
        [MAX_CONVERSATIONS],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn conv(c: &Connection, id: &str, title: &str) {
        create_conversation(c, id, title).unwrap();
        append_message(c, id, &format!("m-{id}"), "user", "q", None, &[]).unwrap();
    }

    #[tokio::test]
    async fn defaults_and_key_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let s = db.call(|c| status(c)).await.unwrap();
        assert_eq!(
            s,
            AssistantStatus {
                has_key: false,
                verified: false,
                model: DEFAULT_MODEL.into(),
                consent: false,
                auto_delete: true,
            }
        );
        let s = db
            .call(|c| {
                set_api_key(c, "sk-ant-x", true)?;
                status(c)
            })
            .await
            .unwrap();
        assert!(s.has_key && s.verified);
        // 状態にキー本体は含まれない
        assert!(!serde_json::to_string(&s).unwrap().contains("sk-ant"));
        let s = db
            .call(|c| {
                clear_api_key(c)?;
                status(c)
            })
            .await
            .unwrap();
        assert!(!s.has_key && !s.verified);
    }

    #[tokio::test]
    async fn rejects_unknown_model() {
        let db = Db::open_in_memory().unwrap();
        let err = db.call(|c| set_model(c, "gpt-x")).await.unwrap_err();
        assert_eq!(err.code(), "VALIDATION");
        db.call(|c| set_model(c, "claude-haiku-5-5")).await.unwrap();
        assert_eq!(
            db.call(|c| status(c)).await.unwrap().model,
            "claude-haiku-5-5"
        );
    }

    #[tokio::test]
    async fn conversation_roundtrip_and_cascade() {
        let db = Db::open_in_memory().unwrap();
        let conv_out = db
            .call(|c| {
                conv(c, "a", "title");
                let sug = StoredSuggestion {
                    summary: "s".into(),
                    changes: serde_json::from_value(serde_json::json!({"steps": 30})).unwrap(),
                    applied: false,
                    model: None,
                };
                append_message(c, "a", "m2", "assistant", "ans", None, &[sug])?;
                mark_suggestion_applied(c, "m2", 0)?;
                get_conversation(c, "a")
            })
            .await
            .unwrap();
        assert_eq!(conv_out.messages.len(), 2);
        assert!(conv_out.messages[1].suggestions[0].applied);
        db.call(|c| delete_conversations(c, &["a".to_string()]))
            .await
            .unwrap();
        let left: i64 = db
            .call(|c| Ok(c.query_row("SELECT COUNT(*) FROM assistant_messages", [], |r| r.get(0))?))
            .await
            .unwrap();
        assert_eq!(left, 0, "会話の削除でメッセージも消える");
        let err = db.call(|c| get_conversation(c, "a")).await.unwrap_err();
        assert_eq!(err.code(), "ASSISTANT_CONVERSATION_NOT_FOUND");
    }

    #[tokio::test]
    async fn auto_delete_keeps_30_newest_and_respects_flag() {
        let db = Db::open_in_memory().unwrap();
        db.call(|c| {
            for i in 0..32 {
                conv(c, &format!("c{i:02}"), "t");
                // updated_at を厳密に増やす
                c.execute(
                    "UPDATE assistant_conversations SET updated_at = ?2 WHERE id = ?1",
                    params![format!("c{i:02}"), 1000 + i],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();

        // 無効の間は削除されない
        let n = db
            .call(|c| {
                set_auto_delete(c, false)?;
                enforce_limit(c)
            })
            .await
            .unwrap();
        assert_eq!(n, 0);
        assert_eq!(db.call(|c| list_conversations(c)).await.unwrap().len(), 32);

        // 有効に戻すと超過分(最も古い 2 件)が消える
        let n = db
            .call(|c| {
                set_auto_delete(c, true)?;
                enforce_limit(c)
            })
            .await
            .unwrap();
        assert_eq!(n, 2);
        let list = db.call(|c| list_conversations(c)).await.unwrap();
        assert_eq!(list.len(), 30);
        assert_eq!(list.first().unwrap().id, "c31");
        assert!(list.iter().all(|c| c.id != "c00" && c.id != "c01"));
    }

    #[tokio::test]
    async fn recent_messages_returns_last_n_in_order() {
        let db = Db::open_in_memory().unwrap();
        let rows = db
            .call(|c| {
                create_conversation(c, "a", "t")?;
                for i in 0..5 {
                    let role = if i % 2 == 0 { "user" } else { "assistant" };
                    append_message(c, "a", &format!("m{i}"), role, &format!("t{i}"), None, &[])?;
                }
                recent_messages(c, "a", 3)
            })
            .await
            .unwrap();
        let texts: Vec<_> = rows.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(texts, ["t2", "t3", "t4"]);
    }
}
