//! 生成の投入・完了待ち・キャンセル(ComfyUI の `/prompt` `/history` `/view` `/ws`)。

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::ComfyUiClient;
use super::parse::{self, HistoryState, ImageRef};
use super::workflow;
use crate::domain::params::GenerationParams;
use crate::engine::{ImageData, Progress};
use crate::error::{AppError, AppResult};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 履歴の取得が連続して失敗したら、エンジンが落ちたとみなして諦める回数。
const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 20;

fn unreachable(e: reqwest::Error) -> AppError {
    AppError::EngineUnreachable(e.without_url().to_string())
}

impl ComfyUiClient {
    pub(super) async fn submit_prompt(&self, params: &GenerationParams) -> AppResult<String> {
        let body = json!({
            "prompt": workflow::build(params, "gazoo"),
            "client_id": self.client_id,
        });
        let resp = self
            .http
            .post(format!("{}/prompt", self.base))
            .json(&body)
            .send()
            .await
            .map_err(unreachable)?;

        let status = resp.status();
        if status == reqwest::StatusCode::BAD_REQUEST {
            // 検証エラー(モデル不在、ノード不在など)。本文で分類する。
            return Err(match resp.json::<Value>().await {
                Ok(v) => parse::parse_prompt_error(&v),
                Err(_) => AppError::EngineExecutionFailed(format!("HTTP {status}")),
            });
        }
        if !status.is_success() {
            return Err(AppError::EngineBadResponse(format!("HTTP {status}")));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| AppError::EngineBadResponse(e.without_url().to_string()))?;
        v.get("prompt_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| AppError::EngineBadResponse("/prompt: prompt_id がありません".into()))
    }

    pub(super) async fn wait_for_result(
        &self,
        id: &str,
        on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> AppResult<Vec<ImageData>> {
        // 完了判定は履歴のポーリングで確実に行い、WebSocket は進捗のベストエフォート。
        let mut ws = self.connect_ws().await;
        let mut ticker = tokio::time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut failures = 0u32;

        loop {
            tokio::select! {
                msg = next_ws_message(&mut ws) => match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Some((step, total, node)) = parse::parse_ws_progress(text.as_str(), id) {
                            on_progress(Progress { step, total, node });
                        }
                    }
                    Some(Ok(_)) => {}
                    // 切断・エラー: 以後はポーリングのみで続行する
                    Some(Err(_)) | None => ws = None,
                },
                _ = ticker.tick() => {
                    match self.get_json(&format!("/history/{id}")).await {
                        Ok(v) => {
                            failures = 0;
                            match parse::parse_history(&v.unwrap_or(Value::Null), id) {
                                HistoryState::Pending => {}
                                HistoryState::Failed(msg) => {
                                    return Err(AppError::EngineExecutionFailed(msg));
                                }
                                HistoryState::Done(images) => {
                                    if images.is_empty() {
                                        return Err(AppError::EngineExecutionFailed(
                                            "画像が生成されませんでした".into(),
                                        ));
                                    }
                                    return self.fetch_images(&images).await;
                                }
                            }
                        }
                        Err(e) => {
                            failures += 1;
                            if failures >= MAX_CONSECUTIVE_POLL_FAILURES {
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
    }

    pub(super) async fn cancel_prompt(&self, id: &str) -> AppResult<()> {
        // 待機中なら取り消し、実行中なら中断する(このアプリは同時に 1 ジョブしか投入しない)
        let _ = self
            .http
            .post(format!("{}/queue", self.base))
            .json(&json!({ "delete": [id] }))
            .send()
            .await;
        let resp = self
            .http
            .post(format!("{}/interrupt", self.base))
            .send()
            .await
            .map_err(unreachable)?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(AppError::EngineBadResponse(format!(
                "/interrupt: HTTP {}",
                resp.status()
            )))
        }
    }

    /// WebSocket に接続する。http のみ対応(https はポーリングだけで進捗なし)。失敗時は `None`。
    async fn connect_ws(&self) -> Option<WsStream> {
        let host = self.base.strip_prefix("http://")?;
        let url = format!("ws://{host}/ws?clientId={}", self.client_id);
        match tokio::time::timeout(
            Duration::from_secs(3),
            tokio_tungstenite::connect_async(url),
        )
        .await
        {
            Ok(Ok((ws, _))) => Some(ws),
            _ => None,
        }
    }

    async fn fetch_images(&self, refs: &[ImageRef]) -> AppResult<Vec<ImageData>> {
        let mut out = Vec::with_capacity(refs.len());
        for r in refs {
            let mut url = reqwest::Url::parse(&format!("{}/view", self.base))
                .map_err(|e| AppError::internal(e.to_string()))?;
            url.query_pairs_mut()
                .append_pair("filename", &r.filename)
                .append_pair("subfolder", &r.subfolder)
                .append_pair("type", &r.kind);
            let resp = self.http.get(url).send().await.map_err(unreachable)?;
            if !resp.status().is_success() {
                return Err(AppError::EngineBadResponse(format!(
                    "/view: HTTP {}",
                    resp.status()
                )));
            }
            let bytes = resp.bytes().await.map_err(unreachable)?;
            out.push(ImageData {
                filename: r.filename.clone(),
                bytes: bytes.to_vec(),
            });
        }
        Ok(out)
    }
}

/// WebSocket が無い(または切れた)場合は永遠に待つ(`select!` で他の枝だけが動く)。
async fn next_ws_message(
    ws: &mut Option<WsStream>,
) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
    match ws {
        Some(stream) => stream.next().await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::testutil::{Route, spawn, spawn_with_ws};
    use super::*;
    use crate::domain::params::{ModelKind, sample};
    use crate::engine::GenerationEngine;

    fn client(url: &str) -> ComfyUiClient {
        ComfyUiClient::new(url)
            .unwrap()
            .with_poll_interval(Duration::from_millis(20))
    }

    #[tokio::test]
    async fn submit_posts_workflow_and_returns_prompt_id() {
        let m = spawn(vec![Route::post(
            "/prompt",
            200,
            r#"{"prompt_id":"abc","number":1}"#,
        )])
        .await;
        let c = client(&m.url);
        let id = c.submit(&sample(ModelKind::Sdxl)).await.unwrap();
        assert_eq!(id, "abc");

        let reqs = m.requests.lock().unwrap();
        let body = reqs
            .iter()
            .find(|r| r.starts_with("POST /prompt "))
            .unwrap();
        assert!(body.contains("CheckpointLoaderSimple"));
        assert!(body.contains(&c.client_id), "client_id should be sent");
    }

    #[tokio::test]
    async fn submit_maps_validation_errors() {
        let model_missing = r#"{"error":{"type":"prompt_outputs_failed_validation","message":"Prompt outputs failed validation"},
            "node_errors":{"1":{"class_type":"CheckpointLoaderSimple","errors":[
            {"type":"value_not_in_list","details":"ckpt_name: 'x' not in []"}]}}}"#;
        let m = spawn(vec![Route::post("/prompt", 400, model_missing)]).await;
        let err = client(&m.url)
            .submit(&sample(ModelKind::Sd15))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "ENGINE_MODEL_MISSING");

        let m = spawn(vec![Route::post("/prompt", 500, "oops")]).await;
        let err = client(&m.url)
            .submit(&sample(ModelKind::Sd15))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "ENGINE_BAD_RESPONSE");
    }

    #[tokio::test]
    async fn wait_polls_until_done_and_fetches_images() {
        let done = r#"{"p1":{"status":{"status_str":"success","completed":true},
            "outputs":{"8":{"images":[{"filename":"a.png","subfolder":"","type":"output"}]}}}}"#;
        let m = spawn(vec![
            Route::seq(
                "GET /history/p1",
                vec![(200, "{}"), (200, "{}"), (200, done)],
            ),
            Route::bytes("GET /view", b"PNGDATA".to_vec()),
        ])
        .await;
        let images = client(&m.url).wait("p1", &|_| {}).await.unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].filename, "a.png");
        assert_eq!(images[0].bytes, b"PNGDATA");

        let reqs = m.requests.lock().unwrap();
        assert!(
            reqs.iter()
                .any(|r| r.starts_with("GET /view?filename=a.png&subfolder=&type=output"))
        );
    }

    #[tokio::test]
    async fn wait_reports_progress_from_websocket() {
        let progress =
            r#"{"type":"progress","data":{"value":5,"max":20,"prompt_id":"p1","node":"5"}}"#;
        let other = r#"{"type":"progress","data":{"value":9,"max":9,"prompt_id":"someone-else"}}"#;
        let done = r#"{"p1":{"status":{"status_str":"success","completed":true},
            "outputs":{"8":{"images":[{"filename":"a.png","subfolder":"","type":"output"}]}}}}"#;
        let m = spawn_with_ws(
            vec![
                // ポーリングは WebSocket 通知を受け取る時間を取るため、数回は空で返す
                Route::seq(
                    "GET /history/p1",
                    vec![
                        (200, "{}"),
                        (200, "{}"),
                        (200, "{}"),
                        (200, "{}"),
                        (200, done),
                    ],
                ),
                Route::bytes("GET /view", b"x".to_vec()),
            ],
            vec![other.to_string(), progress.to_string()],
        )
        .await;

        let seen: Arc<Mutex<Vec<Progress>>> = Arc::default();
        let sink = Arc::clone(&seen);
        client(&m.url)
            .wait("p1", &move |p| sink.lock().unwrap().push(p))
            .await
            .unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(
            *seen,
            [Progress {
                step: 5,
                total: 20,
                node: Some("5".into())
            }],
            "他のジョブの進捗は無視され、自分の進捗だけが通知される"
        );
    }

    #[tokio::test]
    async fn wait_returns_execution_error() {
        let failed = r#"{"p1":{"status":{"status_str":"error","completed":false,"messages":[
            ["execution_error",{"node_type":"KSampler","exception_message":"MPS backend out of memory"}]]}}}"#;
        let m = spawn(vec![Route::get("/history/p1", failed)]).await;
        let err = client(&m.url).wait("p1", &|_| {}).await.unwrap_err();
        assert_eq!(err.code(), "ENGINE_EXECUTION_FAILED");
        assert!(err.to_string().contains("out of memory"));
    }

    #[tokio::test]
    async fn wait_fails_when_no_images_produced() {
        let empty = r#"{"p1":{"status":{"status_str":"success","completed":true},"outputs":{}}}"#;
        let m = spawn(vec![Route::get("/history/p1", empty)]).await;
        let err = client(&m.url).wait("p1", &|_| {}).await.unwrap_err();
        assert_eq!(err.code(), "ENGINE_EXECUTION_FAILED");
    }

    #[tokio::test]
    async fn cancel_deletes_from_queue_and_interrupts() {
        let m = spawn(vec![
            Route::post("/queue", 200, "{}"),
            Route::post("/interrupt", 200, ""),
        ])
        .await;
        client(&m.url).cancel("p1").await.unwrap();
        let reqs = m.requests.lock().unwrap();
        assert!(
            reqs.iter()
                .any(|r| r.starts_with("POST /queue ") && r.contains(r#""delete":["p1"]"#))
        );
        assert!(reqs.iter().any(|r| r == "POST /interrupt"));
    }

    /// 実機: 存在しないモデルの投入が ENGINE_MODEL_MISSING に分類されること(通常は実行しない)。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI"]
    async fn real_comfyui_missing_model_is_classified() {
        let url = std::env::var("COMFYUI_URL").unwrap_or_else(|_| "http://127.0.0.1:8188".into());
        let mut p = sample(ModelKind::Sdxl);
        p.model.name = "definitely-not-installed.safetensors".into();
        let err = ComfyUiClient::new(&url)
            .unwrap()
            .submit(&p)
            .await
            .unwrap_err();
        println!("{err}");
        assert_eq!(err.code(), "ENGINE_MODEL_MISSING");

        // GGUF ノードは導入済みなので、存在しない GGUF モデルもモデル不在になる
        let mut p = sample(ModelKind::FluxGguf);
        p.model.name = "nope.gguf".into();
        let err = ComfyUiClient::new(&url)
            .unwrap()
            .submit(&p)
            .await
            .unwrap_err();
        println!("{err}");
        assert_eq!(err.code(), "ENGINE_MODEL_MISSING");
    }

    /// 実機: 4 種類すべてのワークフローが ComfyUI のバリデータに構造的に受理されること。
    /// モデルが未導入でも、検出されるエラーが「モデル名が一覧に無い」だけであれば、
    /// ノード種別・必須入力・リンクの型はすべて正しいと確認できる(通常は実行しない)。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI"]
    async fn real_comfyui_workflows_are_structurally_valid() {
        let url = std::env::var("COMFYUI_URL").unwrap_or_else(|_| "http://127.0.0.1:8188".into());
        let c = ComfyUiClient::new(&url).unwrap();
        let model_inputs = [
            "ckpt_name",
            "unet_name",
            "clip_name1",
            "clip_name2",
            "vae_name",
        ];

        for kind in [
            ModelKind::Sd15,
            ModelKind::Sdxl,
            ModelKind::Flux,
            ModelKind::FluxGguf,
        ] {
            let mut p = sample(kind);
            p.model.name = "not-installed.safetensors".into();
            let body = json!({ "prompt": workflow::build(&p, "gazoo"), "client_id": c.client_id });
            let resp = c
                .http
                .post(format!("{}/prompt", c.base))
                .json(&body)
                .send()
                .await
                .unwrap();
            let v: Value = resp.json().await.unwrap();

            let mut seen = vec![];
            for (node, ne) in v["node_errors"].as_object().expect("node_errors") {
                for e in ne["errors"].as_array().unwrap() {
                    let kind_str = e["type"].as_str().unwrap_or("?");
                    let detail = e["details"].as_str().unwrap_or("");
                    let input = detail.split(':').next().unwrap_or("");
                    seen.push(format!("{node}/{}: {kind_str} {input}", ne["class_type"]));
                    assert_eq!(kind_str, "value_not_in_list", "{kind:?} node {node}: {e}");
                    assert!(
                        model_inputs.contains(&input),
                        "{kind:?} node {node}: {detail}"
                    );
                }
            }
            assert!(
                !seen.is_empty(),
                "{kind:?}: ComfyUI accepted a non-existent model?! {v}"
            );
            println!("{kind:?}: {seen:?}");
        }
    }

    fn real_url() -> String {
        std::env::var("COMFYUI_URL").unwrap_or_else(|_| "http://127.0.0.1:8188".into())
    }

    /// 実機での検証用パラメータ(SD 1.5、小さく速い設定)。モデルは `COMFYUI_SD15_MODEL` で変更できる。
    fn real_sd15_params(steps: u32, seed: u64) -> GenerationParams {
        let mut p = sample(ModelKind::Sd15);
        p.model.name = std::env::var("COMFYUI_SD15_MODEL")
            .unwrap_or_else(|_| "v1-5-pruned-emaonly.safetensors".into());
        p.prompt = "a red apple on a wooden table, studio lighting".into();
        p.negative_prompt = "blurry, low quality".into();
        p.width = 512;
        p.height = 512;
        p.steps = steps;
        p.seed = seed;
        p
    }

    const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    /// 実機: 画像が実際に生成され、進捗が通知され、PNG として取得できること。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI with an SD 1.5 checkpoint"]
    async fn real_comfyui_generates_sd15_image_with_progress() {
        let c = ComfyUiClient::new(&real_url()).unwrap();
        let id = c.submit(&real_sd15_params(8, 1)).await.expect("submit");

        let seen: Arc<Mutex<Vec<Progress>>> = Arc::default();
        let sink = Arc::clone(&seen);
        let images = c
            .wait(&id, &move |p| sink.lock().unwrap().push(p))
            .await
            .expect("wait");

        assert_eq!(images.len(), 1);
        assert_eq!(&images[0].bytes[..8], &PNG_MAGIC, "PNG として取得できる");
        assert!(images[0].bytes.len() > 10_000, "画像として妥当なサイズ");
        let decoded = image::load_from_memory(&images[0].bytes).expect("decodable");
        assert_eq!((decoded.width(), decoded.height()), (512, 512));

        let seen = seen.lock().unwrap();
        println!("progress events: {}", seen.len());
        assert!(!seen.is_empty(), "WebSocket から進捗を受信できる");
        assert_eq!(seen.last().unwrap().total, 8);
    }

    /// 実機: 同じパラメータ・同じシードなら同じ画像になること(ギャラリーの「この設定で生成」の前提)。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI with an SD 1.5 checkpoint"]
    async fn real_comfyui_same_seed_reproduces_same_pixels() {
        let c = ComfyUiClient::new(&real_url()).unwrap();
        let mut pixels = Vec::new();
        for _ in 0..2 {
            let id = c.submit(&real_sd15_params(6, 777)).await.expect("submit");
            let images = c.wait(&id, &|_| {}).await.expect("wait");
            pixels.push(
                image::load_from_memory(&images[0].bytes)
                    .unwrap()
                    .to_rgb8()
                    .into_raw(),
            );
        }
        assert_eq!(pixels[0], pixels[1], "同じシードで画素が一致しない");

        // 別のシードなら別の画像になる(シードが実際に効いている)
        let id = c.submit(&real_sd15_params(6, 778)).await.expect("submit");
        let other = c.wait(&id, &|_| {}).await.expect("wait");
        let other = image::load_from_memory(&other[0].bytes)
            .unwrap()
            .to_rgb8()
            .into_raw();
        assert_ne!(pixels[0], other, "シードを変えても同じ画像になっている");
    }
}
