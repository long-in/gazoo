//! ComfyUI アダプタ(HTTP)。docs/02-architecture.md §3.4。

mod generate;
mod parse;
#[cfg(test)]
mod testutil;
pub mod workflow;

use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use super::{EngineFeatures, EngineModels, EngineStatus, GenerationEngine, ImageData, Progress};
use crate::domain::params::GenerationParams;
use crate::error::{AppError, AppResult};

const NODE_CHECKPOINT: &str = "CheckpointLoaderSimple";
const NODE_UNET: &str = "UNETLoader";
const NODE_UNET_GGUF: &str = "UnetLoaderGGUF";
const NODE_CLIP: &str = "CLIPLoader";
const NODE_VAE: &str = "VAELoader";
const NODE_LORA: &str = "LoraLoader";
const NODE_KSAMPLER: &str = "KSampler";

pub struct ComfyUiClient {
    base: String,
    http: reqwest::Client,
    /// ComfyUI の WebSocket でイベントを受け取るためのクライアント ID。
    client_id: String,
    /// 完了確認(履歴ポーリング)の間隔。
    poll_interval: Duration,
}

impl ComfyUiClient {
    pub fn new(url: &str) -> AppResult<Self> {
        // reqwest(rustls-no-provider)は暗号プロバイダ未登録だと生成時に panic するため、
        // クライアントを作る前に必ず ring を登録する(登録済みなら無視される)。
        // aws-lc を避けることで Windows でも追加のビルドツールが不要になる。
        let _ = rustls::crypto::ring::default_provider().install_default();

        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(Self {
            base: url.trim_end_matches('/').to_string(),
            http,
            client_id: uuid::Uuid::now_v7().to_string(),
            poll_interval: Duration::from_millis(500),
        })
    }

    #[cfg(test)]
    fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// GET して JSON を返す。404 は `Ok(None)`(未導入ノードの問い合わせ用)。
    async fn get_json(&self, path: &str) -> AppResult<Option<Value>> {
        let resp = self
            .http
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .map_err(|e| AppError::EngineUnreachable(e.without_url().to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(AppError::EngineBadResponse(format!("HTTP {status}")));
        }
        resp.json::<Value>()
            .await
            .map(Some)
            .map_err(|e| AppError::EngineBadResponse(e.without_url().to_string()))
    }

    /// `/object_info/<node>`。未導入のノードは `Null`。
    async fn node_info(&self, node: &str) -> AppResult<Value> {
        Ok(self
            .get_json(&format!("/object_info/{node}"))
            .await?
            .unwrap_or(Value::Null))
    }
}

#[async_trait]
impl GenerationEngine for ComfyUiClient {
    fn url(&self) -> String {
        self.base.clone()
    }

    async fn health(&self) -> AppResult<EngineStatus> {
        let stats = self
            .get_json("/system_stats")
            .await?
            .as_ref()
            .and_then(parse::parse_system_stats)
            .ok_or_else(|| AppError::EngineBadResponse("/system_stats".into()))?;

        // GGUF ノードの有無。確認に失敗しても接続自体は成功扱い。
        let gguf_node = match self.node_info(NODE_UNET_GGUF).await {
            Ok(info) => parse::has_node(&info, NODE_UNET_GGUF),
            Err(_) => false,
        };

        Ok(EngineStatus {
            connected: true,
            url: self.url(),
            version: stats.version,
            device: stats.device,
            vram_free: stats.vram_free,
            features: EngineFeatures { gguf_node },
            error: None,
        })
    }

    async fn list_models(&self) -> AppResult<EngineModels> {
        let (ckpt, unet, gguf, clip, vae, lora, ks) = tokio::join!(
            self.node_info(NODE_CHECKPOINT),
            self.node_info(NODE_UNET),
            self.node_info(NODE_UNET_GGUF),
            self.node_info(NODE_CLIP),
            self.node_info(NODE_VAE),
            self.node_info(NODE_LORA),
            self.node_info(NODE_KSAMPLER),
        );
        let (ckpt, unet, gguf, clip, vae, lora, ks) =
            (ckpt?, unet?, gguf?, clip?, vae?, lora?, ks?);

        Ok(EngineModels {
            checkpoints: parse::extract_options(&ckpt, NODE_CHECKPOINT, "ckpt_name"),
            unets: parse::extract_options(&unet, NODE_UNET, "unet_name"),
            gguf_unets: parse::extract_options(&gguf, NODE_UNET_GGUF, "unet_name"),
            clips: parse::extract_options(&clip, NODE_CLIP, "clip_name"),
            vaes: parse::extract_options(&vae, NODE_VAE, "vae_name"),
            loras: parse::extract_options(&lora, NODE_LORA, "lora_name"),
            samplers: parse::extract_options(&ks, NODE_KSAMPLER, "sampler_name"),
            schedulers: parse::extract_options(&ks, NODE_KSAMPLER, "scheduler"),
        })
    }

    async fn submit(&self, params: &GenerationParams) -> AppResult<String> {
        self.submit_prompt(params).await
    }

    async fn wait(
        &self,
        remote_id: &str,
        on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> AppResult<Vec<ImageData>> {
        self.wait_for_result(remote_id, on_progress).await
    }

    async fn cancel(&self, remote_id: &str) -> AppResult<()> {
        self.cancel_prompt(remote_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::{Route, spawn};
    use super::*;
    use tokio::net::TcpListener;

    const STATS: &str =
        r#"{"system":{"comfyui_version":"0.3.50"},"devices":[{"name":"mps","vram_free":42}]}"#;

    #[tokio::test]
    async fn health_connected_with_gguf() {
        let url = spawn(vec![
            Route::get("/system_stats", STATS),
            Route::get(
                "/object_info/UnetLoaderGGUF",
                r#"{"UnetLoaderGGUF":{"input":{}}}"#,
            ),
        ])
        .await
        .url;
        let s = ComfyUiClient::new(&url).unwrap().health().await.unwrap();
        assert!(s.connected && s.features.gguf_node);
        assert_eq!(s.version.as_deref(), Some("0.3.50"));
        assert_eq!(s.device.as_deref(), Some("mps"));
        assert_eq!(s.vram_free, Some(42));
    }

    #[tokio::test]
    async fn health_without_gguf_node_when_info_is_empty_or_404() {
        // 空オブジェクト応答
        let url = spawn(vec![
            Route::get("/system_stats", STATS),
            Route::get("/object_info/UnetLoaderGGUF", "{}"),
        ])
        .await
        .url;
        assert!(
            !ComfyUiClient::new(&url)
                .unwrap()
                .health()
                .await
                .unwrap()
                .features
                .gguf_node
        );
        // 404
        let url = spawn(vec![Route::get("/system_stats", STATS)]).await.url;
        assert!(
            !ComfyUiClient::new(&url)
                .unwrap()
                .health()
                .await
                .unwrap()
                .features
                .gguf_node
        );
    }

    #[tokio::test]
    async fn health_unreachable_when_port_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = ComfyUiClient::new(&format!("http://{addr}"))
            .unwrap()
            .health()
            .await
            .unwrap_err();
        assert_eq!(err.code(), "ENGINE_UNREACHABLE");
    }

    #[tokio::test]
    async fn health_bad_response_for_foreign_service() {
        let url = spawn(vec![Route::get("/system_stats", r#"{"hello":"world"}"#)])
            .await
            .url;
        let err = ComfyUiClient::new(&url)
            .unwrap()
            .health()
            .await
            .unwrap_err();
        assert_eq!(err.code(), "ENGINE_BAD_RESPONSE");
        // 404(別サービスなど)
        let url = spawn(vec![]).await.url;
        let err = ComfyUiClient::new(&url)
            .unwrap()
            .health()
            .await
            .unwrap_err();
        assert_eq!(err.code(), "ENGINE_BAD_RESPONSE");
    }

    #[tokio::test]
    async fn list_models_collects_all_kinds_and_tolerates_missing_nodes() {
        let url = spawn(vec![
            Route::get("/object_info/CheckpointLoaderSimple", r#"{"CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["sdxl.safetensors"],{}]}}}}"#),
            Route::get("/object_info/UNETLoader", r#"{"UNETLoader":{"input":{"required":{"unet_name":["COMBO",{"options":["flux.safetensors"]}]}}}}"#),
            Route::get("/object_info/CLIPLoader", r#"{"CLIPLoader":{"input":{"required":{"clip_name":[["t5xxl.safetensors","clip_l.safetensors"]]}}}}"#),
            Route::get("/object_info/KSampler", r#"{"KSampler":{"input":{"required":{"sampler_name":[["euler","dpmpp_2m"]],"scheduler":[["simple","karras"]]}}}}"#),
            // VAELoader / LoraLoader / UnetLoaderGGUF は 404(未提供)
        ])
        .await.url;
        let m = ComfyUiClient::new(&url)
            .unwrap()
            .list_models()
            .await
            .unwrap();
        assert_eq!(m.checkpoints, ["sdxl.safetensors"]);
        assert_eq!(m.unets, ["flux.safetensors"]);
        assert_eq!(m.clips, ["t5xxl.safetensors", "clip_l.safetensors"]);
        assert_eq!(m.samplers, ["euler", "dpmpp_2m"]);
        assert_eq!(m.schedulers, ["simple", "karras"]);
        assert!(m.gguf_unets.is_empty() && m.vaes.is_empty() && m.loras.is_empty());
    }

    /// 実際に起動している ComfyUI に対するスモークテスト(通常は実行しない)。
    /// `npm run comfyui:start` の後に `cargo test -- --ignored real_comfyui` で実行する。
    /// 接続先は環境変数 `COMFYUI_URL`(既定 http://127.0.0.1:8188)。
    #[tokio::test]
    #[ignore = "requires a running ComfyUI"]
    async fn real_comfyui_smoke() {
        let url = std::env::var("COMFYUI_URL").unwrap_or_else(|_| "http://127.0.0.1:8188".into());
        let client = ComfyUiClient::new(&url).unwrap();

        let status = client.health().await.expect("health");
        assert!(status.connected);
        assert!(status.version.is_some(), "version should be parsed");
        println!("status: {status:?}");

        let models = client.list_models().await.expect("list_models");
        println!("models: {models:?}");
        // 実際の KSampler から選択肢が解釈できていること(形式の差異の検出)
        assert!(
            models.samplers.iter().any(|s| s == "euler"),
            "samplers: {:?}",
            models.samplers
        );
        assert!(
            !models.schedulers.is_empty(),
            "schedulers should not be empty"
        );
    }

    #[tokio::test]
    async fn list_models_fails_when_unreachable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = ComfyUiClient::new(&format!("http://{addr}"))
            .unwrap()
            .list_models()
            .await
            .unwrap_err();
        assert_eq!(err.code(), "ENGINE_UNREACHABLE");
    }
}
