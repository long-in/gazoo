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
use crate::domain::job::ProgressPhase;
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
    /// 参照画像を `POST /upload/image`(multipart、`type=input`)で送り、`LoadImage` に渡す名前を返す。
    /// ファイル名は UUID を含めて衝突・上書きを避ける(ComfyUI 側の input に残るファイルは削除しない)。
    pub(super) async fn upload_image(&self, png: Vec<u8>) -> AppResult<String> {
        let filename = format!("gazoo-{}.png", uuid::Uuid::now_v7());
        let part = reqwest::multipart::Part::bytes(png)
            .file_name(filename)
            .mime_str("image/png")
            .map_err(|e| AppError::internal(e.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .part("image", part)
            .text("type", "input")
            .text("overwrite", "false");
        let resp = self
            .http
            .post(format!("{}/upload/image", self.base))
            .multipart(form)
            .send()
            .await
            .map_err(unreachable)?;
        if !resp.status().is_success() {
            return Err(AppError::EngineBadResponse(format!(
                "/upload/image: HTTP {}",
                resp.status()
            )));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| AppError::EngineBadResponse(e.without_url().to_string()))?;
        parse::parse_upload_name(&v)
            .ok_or_else(|| AppError::EngineBadResponse("/upload/image: name がありません".into()))
    }

    pub(super) async fn submit_prompt(
        &self,
        params: &GenerationParams,
        inputs: &[String],
    ) -> AppResult<String> {
        let needed = params.upload_count();
        if inputs.len() != needed {
            return Err(AppError::internal(format!(
                "参照画像の数が一致しません(必要 {needed}、指定 {})",
                inputs.len()
            )));
        }
        let body = json!({
            "prompt": workflow::build(params, "gazoo", inputs),
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
                            let phase = if node.as_deref() == Some(workflow::FACE_DETAILER_NODE) {
                                ProgressPhase::FaceDetail
                            } else {
                                ProgressPhase::Generate
                            };
                            on_progress(Progress {
                                step,
                                total,
                                node,
                                phase: Some(phase),
                            });
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
        let id = c.submit(&sample(ModelKind::Sdxl), &[]).await.unwrap();
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
    async fn upload_sends_multipart_and_returns_name() {
        let m = spawn(vec![Route::post(
            "/upload/image",
            200,
            r#"{"name":"gazoo-x.png","subfolder":"","type":"input"}"#,
        )])
        .await;
        let name = client(&m.url)
            .upload_input(b"PNGDATA".to_vec())
            .await
            .unwrap();
        assert_eq!(name, "gazoo-x.png");

        let reqs = m.requests.lock().unwrap();
        let req = reqs
            .iter()
            .find(|r| r.starts_with("POST /upload/image"))
            .unwrap();
        assert!(req.contains("name=\"image\""), "{req}");
        assert!(req.contains("PNGDATA"));
        assert!(req.contains("name=\"type\"") && req.contains("input"));
        assert!(req.contains("filename=\"gazoo-"), "ファイル名は UUID 入り");
    }

    #[tokio::test]
    async fn upload_failures_are_classified() {
        let m = spawn(vec![Route::post("/upload/image", 500, "oops")]).await;
        let err = client(&m.url).upload_input(vec![1]).await.unwrap_err();
        assert_eq!(err.code(), "ENGINE_BAD_RESPONSE");
        let m = spawn(vec![Route::post("/upload/image", 200, r#"{"x":1}"#)]).await;
        let err = client(&m.url).upload_input(vec![1]).await.unwrap_err();
        assert_eq!(err.code(), "ENGINE_BAD_RESPONSE");
    }

    #[tokio::test]
    async fn submit_with_controlnet_embeds_uploaded_name_and_checks_count() {
        use crate::domain::params::{ControlnetParams, ControlnetUnit, Preprocessor};
        let mut p = sample(ModelKind::Sd15);
        p.controlnet = Some(ControlnetParams {
            enabled: true,
            units: vec![ControlnetUnit {
                image: "inputs/a.png".into(),
                model: "canny.safetensors".into(),
                preprocessor: Preprocessor::Canny,
                strength: 1.0,
                start_percent: 0.0,
                end_percent: 1.0,
            }],
        });
        let m = spawn(vec![Route::post("/prompt", 200, r#"{"prompt_id":"cn1"}"#)]).await;
        let c = client(&m.url);

        // 参照画像の数が合わなければ、エンジンへ送る前に断る
        assert_eq!(c.submit(&p, &[]).await.unwrap_err().code(), "INTERNAL");
        assert!(m.requests.lock().unwrap().is_empty());

        let id = c.submit(&p, &["gazoo-x.png".to_string()]).await.unwrap();
        assert_eq!(id, "cn1");
        let reqs = m.requests.lock().unwrap();
        let body = reqs
            .iter()
            .find(|r| r.starts_with("POST /prompt "))
            .unwrap();
        assert!(body.contains("ControlNetApplyAdvanced"));
        assert!(body.contains("gazoo-x.png"));
    }

    #[tokio::test]
    async fn submit_maps_validation_errors() {
        let model_missing = r#"{"error":{"type":"prompt_outputs_failed_validation","message":"Prompt outputs failed validation"},
            "node_errors":{"1":{"class_type":"CheckpointLoaderSimple","errors":[
            {"type":"value_not_in_list","details":"ckpt_name: 'x' not in []"}]}}}"#;
        let m = spawn(vec![Route::post("/prompt", 400, model_missing)]).await;
        let err = client(&m.url)
            .submit(&sample(ModelKind::Sd15), &[])
            .await
            .unwrap_err();
        assert_eq!(err.code(), "ENGINE_MODEL_MISSING");

        let m = spawn(vec![Route::post("/prompt", 500, "oops")]).await;
        let err = client(&m.url)
            .submit(&sample(ModelKind::Sd15), &[])
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
                node: Some("5".into()),
                phase: Some(ProgressPhase::Generate),
            }],
            "他のジョブの進捗は無視され、自分の進捗だけが通知される"
        );
    }

    #[tokio::test]
    async fn progress_on_face_detailer_node_is_tagged_as_face_phase() {
        let main_pass =
            r#"{"type":"progress","data":{"value":20,"max":20,"prompt_id":"p1","node":"5"}}"#;
        let face_pass = format!(
            r#"{{"type":"progress","data":{{"value":3,"max":10,"prompt_id":"p1","node":"{}"}}}}"#,
            workflow::FACE_DETAILER_NODE
        );
        let done = r#"{"p1":{"status":{"status_str":"success","completed":true},
            "outputs":{"8":{"images":[{"filename":"a.png","subfolder":"","type":"output"}]}}}}"#;
        let m = spawn_with_ws(
            vec![
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
            vec![main_pass.to_string(), face_pass],
        )
        .await;

        let seen: Arc<Mutex<Vec<Progress>>> = Arc::default();
        let sink = Arc::clone(&seen);
        client(&m.url)
            .wait("p1", &move |p| sink.lock().unwrap().push(p))
            .await
            .unwrap();
        let phases: Vec<_> = seen.lock().unwrap().iter().map(|p| p.phase).collect();
        assert_eq!(
            phases,
            [
                Some(ProgressPhase::Generate),
                Some(ProgressPhase::FaceDetail)
            ]
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
            .submit(&p, &[])
            .await
            .unwrap_err();
        println!("{err}");
        assert_eq!(err.code(), "ENGINE_MODEL_MISSING");

        // GGUF ノードは導入済みなので、存在しない GGUF モデルもモデル不在になる
        let mut p = sample(ModelKind::FluxGguf);
        p.model.name = "nope.gguf".into();
        let err = ComfyUiClient::new(&url)
            .unwrap()
            .submit(&p, &[])
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
            let body =
                json!({ "prompt": workflow::build(&p, "gazoo", &[]), "client_id": c.client_id });
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
        let id = c
            .submit(&real_sd15_params(8, 1), &[])
            .await
            .expect("submit");

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
            let id = c
                .submit(&real_sd15_params(6, 777), &[])
                .await
                .expect("submit");
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
        let id = c
            .submit(&real_sd15_params(6, 778), &[])
            .await
            .expect("submit");
        let other = c.wait(&id, &|_| {}).await.expect("wait");
        let other = image::load_from_memory(&other[0].bytes)
            .unwrap()
            .to_rgb8()
            .into_raw();
        assert_ne!(pixels[0], other, "シードを変えても同じ画像になっている");
    }

    /// 実機: 顔補正(FaceDetailer)が動き、進捗が「顔補正」の段階で通知されること。
    /// 顔が写らない絵では補正をスキップして正常終了し、バッチ(2 枚)でも全枚が返ること。
    /// Impact Pack / Subpack と `bbox/face_yolov8m.pt` が必要(docs/08 §6.1)。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI with Impact Pack, a detector model and an SD 1.5 checkpoint"]
    async fn real_comfyui_face_detailer_runs_and_skips_when_no_face() {
        use crate::domain::params::FaceDetailerParams;
        let c = ComfyUiClient::new(&real_url()).unwrap();
        let face = |p: &mut GenerationParams| {
            p.face_detailer = Some(FaceDetailerParams {
                enabled: true,
                detector: "bbox/face_yolov8m.pt".into(),
                denoise: 0.4,
                prompt: String::new(),
                confidence: 0.3,
            });
        };

        // 顔あり: 補正の段階の進捗が来る
        let mut p = real_sd15_params(20, 3);
        p.prompt = "portrait photo of a woman, detailed face, looking at camera".into();
        p.width = 512;
        p.height = 512;
        face(&mut p);
        let id = c.submit(&p, &[]).await.expect("submit");
        let seen: Arc<Mutex<Vec<Progress>>> = Arc::default();
        let sink = Arc::clone(&seen);
        let images = c
            .wait(&id, &move |p| sink.lock().unwrap().push(p))
            .await
            .expect("wait");
        assert_eq!(images.len(), 1);
        assert_eq!(&images[0].bytes[..8], &PNG_MAGIC);
        let phases: Vec<_> = seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|p| p.phase)
            .collect();
        println!("phases: {phases:?}");
        assert!(phases.contains(&ProgressPhase::Generate));
        assert!(
            phases.contains(&ProgressPhase::FaceDetail),
            "顔補正の進捗が通知されない"
        );

        // 顔なし(リンゴ): 検出ゼロでもエラーにならず、画像が返る。バッチ 2 枚
        let mut p = real_sd15_params(8, 5);
        p.batch_size = 2;
        face(&mut p);
        let id = c.submit(&p, &[]).await.expect("submit");
        let images = c.wait(&id, &|_| {}).await.expect("wait");
        assert_eq!(images.len(), 2, "バッチの全枚が返る");
    }

    /// 参照画像用の合成画像(黒地に白い円と四角。輪郭がはっきりしていて Canny で抽出できる)。
    fn reference_png() -> Vec<u8> {
        use image::{ImageFormat, Rgba, RgbaImage};
        let img = RgbaImage::from_fn(512, 512, |x, y| {
            let (dx, dy) = (x as i32 - 180, y as i32 - 200);
            let circle = dx * dx + dy * dy < 100 * 100;
            let square = (300..460).contains(&x) && (280..440).contains(&y);
            let v = if circle || square { 255 } else { 0 };
            Rgba([v, v, v, 255])
        });
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn cn_unit(
        model: &str,
        pre: crate::domain::params::Preprocessor,
    ) -> crate::domain::params::ControlnetParams {
        use crate::domain::params::{ControlnetParams, ControlnetUnit};
        ControlnetParams {
            enabled: true,
            units: vec![ControlnetUnit {
                image: "inputs/ref.png".into(),
                model: model.into(),
                preprocessor: pre,
                strength: 1.0,
                start_percent: 0.0,
                end_percent: 1.0,
            }],
        }
    }

    /// 実機: 参照画像のアップロード → ControlNet(Canny)付きの生成が完了すること。
    /// 同じシードでも、ControlNet の有無で画像が変わる(条件が効いている)こと。
    /// `COMFYUI_CONTROLNET_MODEL`(既定 control_v11p_sd15_canny_fp16.safetensors)が必要(docs/08 §6.1)。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI with an SD 1.5 checkpoint and a canny ControlNet model"]
    async fn real_comfyui_controlnet_canny_generates_with_uploaded_reference() {
        use crate::domain::params::Preprocessor;
        let c = ComfyUiClient::new(&real_url()).unwrap();
        let model = std::env::var("COMFYUI_CONTROLNET_MODEL")
            .unwrap_or_else(|_| "control_v11p_sd15_canny_fp16.safetensors".into());

        let name = c.upload_input(reference_png()).await.expect("upload");
        println!("uploaded as {name}");

        let mut p = real_sd15_params(12, 11);
        p.prompt = "a still life with a glowing orb and a wooden box, studio photo".into();
        p.controlnet = Some(cn_unit(&model, Preprocessor::Canny));
        let id = c
            .submit(&p, std::slice::from_ref(&name))
            .await
            .expect("submit");
        let with_cn = c.wait(&id, &|_| {}).await.expect("wait");
        assert_eq!(with_cn.len(), 1);
        assert_eq!(&with_cn[0].bytes[..8], &PNG_MAGIC);

        let mut plain = p.clone();
        plain.controlnet = None;
        let id = c.submit(&plain, &[]).await.expect("submit");
        let without = c.wait(&id, &|_| {}).await.expect("wait");
        let px = |d: &ImageData| {
            image::load_from_memory(&d.bytes)
                .unwrap()
                .to_rgb8()
                .into_raw()
        };
        assert_ne!(
            px(&with_cn[0]),
            px(&without[0]),
            "ControlNet が効いていない"
        );
    }

    /// 実機: Inpaint が完了し、合成ありならマスク外が元画像と一致、マスク内は変わること。
    /// SDXL のみ導入されている環境向けに `COMFYUI_SD15_MODEL` と `COMFYUI_INPAINT_SIZE` を使える。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI with a checkpoint (COMFYUI_SD15_MODEL)"]
    async fn real_comfyui_inpaint_keeps_unmasked_pixels_identical() {
        use crate::domain::params::InpaintParams;
        use image::{GrayImage, ImageFormat, Luma};
        let c = ComfyUiClient::new(&real_url()).unwrap();
        let size: u32 = std::env::var("COMFYUI_INPAINT_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(512);

        // 参照画像(size に拡大)と、左半分を描き直すマスク
        let reference = image::load_from_memory(&reference_png())
            .unwrap()
            .resize_exact(size, size, image::imageops::FilterType::Nearest);
        let mut ref_png = Vec::new();
        reference
            .write_to(&mut std::io::Cursor::new(&mut ref_png), ImageFormat::Png)
            .unwrap();
        let mask = GrayImage::from_fn(size, size, |x, _| {
            Luma([if x < size / 2 { 255 } else { 0 }])
        });
        let mut mask_png = Vec::new();
        mask.write_to(&mut std::io::Cursor::new(&mut mask_png), ImageFormat::Png)
            .unwrap();
        let image_name = c.upload_input(ref_png).await.expect("upload image");
        let mask_name = c.upload_input(mask_png).await.expect("upload mask");

        let mut p = real_sd15_params(12, 5);
        p.width = size;
        p.height = size;
        p.prompt = "a wooden table with a bowl of fruit, studio photo".into();
        p.inpaint = Some(InpaintParams {
            enabled: true,
            image: "inputs/ref.png".into(),
            mask: "inputs/mask.png".into(),
            denoise: 0.8,
            grow_mask: 0,
            feather: 0,
            composite_back: true,
        });
        let id = c
            .submit(&p, &[image_name, mask_name])
            .await
            .expect("submit");
        let out = c.wait(&id, &|_| {}).await.expect("wait");
        assert_eq!(out.len(), 1);
        let got = image::load_from_memory(&out[0].bytes).unwrap().to_rgb8();
        let orig = reference.to_rgb8();
        assert_eq!(got.dimensions(), orig.dimensions());
        // 右半分(マスク外。境界から離れた範囲)は元画像と完全一致
        let mut diff_left = 0u64;
        for y in 0..size {
            for x in 0..size {
                let same = got.get_pixel(x, y) == orig.get_pixel(x, y);
                if x >= size / 2 + 2 {
                    assert!(same, "マスク外の画素が変わった: ({x}, {y})");
                } else if x < size / 2 - 2 && !same {
                    diff_left += 1;
                }
            }
        }
        assert!(
            diff_left > 1000,
            "マスク内が描き直されていない: {diff_left}"
        );
    }

    /// 実機: Depth / OpenPose の前処理を含むワークフローが、バリデータに構造的に受理されること。
    /// ControlNet モデル名だけ存在しないものにし、検出されるエラーがそれだけなら、
    /// ほかのノード種別・必須入力・リンクの型は正しい(前処理の重みは実行時に取得されるため、実行はしない)。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI with controlnet_aux and an SD 1.5 checkpoint"]
    async fn real_comfyui_controlnet_preprocessor_workflows_are_structurally_valid() {
        use crate::domain::params::Preprocessor;
        let c = ComfyUiClient::new(&real_url()).unwrap();
        let name = c.upload_input(reference_png()).await.expect("upload");

        for pre in [
            Preprocessor::Canny,
            Preprocessor::Depth,
            Preprocessor::Openpose,
            Preprocessor::None,
        ] {
            let mut p = real_sd15_params(8, 1);
            p.controlnet = Some(cn_unit("definitely-not-installed.safetensors", pre));
            let body = json!({
                "prompt": workflow::build(&p, "gazoo", std::slice::from_ref(&name)),
                "client_id": c.client_id,
            });
            let v: Value = c
                .http
                .post(format!("{}/prompt", c.base))
                .json(&body)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let errors: Vec<String> = v["node_errors"]
                .as_object()
                .expect("node_errors")
                .iter()
                .flat_map(|(node, ne)| {
                    ne["errors"].as_array().unwrap().iter().map(move |e| {
                        format!(
                            "{node}/{}: {} {}",
                            ne["class_type"], e["type"], e["details"]
                        )
                    })
                })
                .collect();
            println!("{pre:?}: {errors:?}");
            assert_eq!(errors.len(), 1, "{pre:?}: {errors:?}");
            assert!(
                errors[0].starts_with("33/\"ControlNetLoader\": \"value_not_in_list\""),
                "{pre:?}: {errors:?}"
            );
        }
    }
}
