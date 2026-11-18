//! SQLite 接続とマイグレーション(docs/03-data-model.md)。
//!
//! `rusqlite` は同期 API のため、`Db::call` で `spawn_blocking` 上に逃がして使う。

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

pub mod assistant;
pub mod images;
pub mod jobs;
pub mod presets;

use crate::error::{AppError, AppResult};

const MIGRATION_SLICE: &[M<'static>] = &[
    M::up(include_str!("migrations/0001_init.sql")),
    M::up(include_str!("migrations/0002_job_queue_hidden.sql")),
    M::up(include_str!("migrations/0003_assistant.sql")),
];
const MIGRATIONS: Migrations<'static> = Migrations::from_slice(MIGRATION_SLICE);

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// ファイル DB を開き、必要ならバックアップを取ってからマイグレーションを適用する。
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        configure(&conn)?;
        backup_before_migration(&conn, path)?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok(Self::wrap(conn))
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok(Self::wrap(conn))
    }

    fn wrap(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// ブロッキング用スレッドで DB 処理を実行する。
    pub async fn call<T, F>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| AppError::internal("database lock poisoned"))?;
            f(&mut guard)
        })
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
    }
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;",
    )?;
    Ok(())
}

/// 既存 DB のスキーマが古い場合、マイグレーション前に `gazoo.db.bak-<version>` を作る。
fn backup_before_migration(conn: &Connection, path: &Path) -> AppResult<()> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current == 0 || current as usize >= MIGRATION_SLICE.len() {
        return Ok(());
    }
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    let mut backup = path.as_os_str().to_owned();
    backup.push(format!(".bak-{current}"));
    std::fs::copy(path, std::path::PathBuf::from(backup))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[tokio::test]
    async fn creates_all_tables() {
        let db = Db::open_in_memory().unwrap();
        let tables: Vec<String> = db
            .call(|c| {
                let mut stmt = c.prepare(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                )?;
                let rows = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .unwrap();
        assert_eq!(
            tables,
            [
                "assistant_conversations",
                "assistant_messages",
                "images",
                "jobs",
                "presets",
                "settings"
            ]
        );
    }

    #[tokio::test]
    async fn foreign_keys_cascade_on_job_delete() {
        let db = Db::open_in_memory().unwrap();
        let count: i64 = db
            .call(|c| {
                c.execute(
                    "INSERT INTO jobs (id,status,position,engine,params_json,prompt_text,model_name,created_at)
                     VALUES ('j','queued',0,'comfyui','{}','p','m',0)",
                    [],
                )?;
                c.execute(
                    "INSERT INTO images (id,job_id,file_path,thumb_path,width,height,seed,file_size,created_at)
                     VALUES ('i','j','a.png','a.webp',1,1,1,1,0)",
                    [],
                )?;
                c.execute("DELETE FROM jobs WHERE id='j'", [])?;
                Ok(c.query_row("SELECT COUNT(*) FROM images", [], |r| r.get(0))?)
            })
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn file_db_uses_wal_and_persists() {
        let dir = std::env::temp_dir().join(format!("gazoo-test-{}", uuid::Uuid::now_v7()));
        let path = dir.join("gazoo.db");
        {
            let db = Db::open(&path).unwrap();
            let mode: String = db
                .conn
                .lock()
                .unwrap()
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
        }
        // 再オープン(マイグレーション済みでもエラーにならない)
        Db::open(&path).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
