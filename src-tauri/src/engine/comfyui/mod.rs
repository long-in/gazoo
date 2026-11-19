//! ComfyUI アダプタ(HTTP)。docs/02-architecture.md §3.4。

mod generate;
mod parse;
#[cfg(test)]
pub(crate) mod testutil;
pub mod workflow;

use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use super::{EngineFeatures, EngineModels, EngineStatus, GenerationEngine, ImageData, Progress};
use crate::domain::params::{GenerationParams, Preprocessor};
use crate::error::{AppError, AppResult};

const NODE_CHECKPOINT: &str = "CheckpointLoaderSimple";
const NODE_UNET: &str = "UNETLoader";
const NODE_UNET_GGUF: &str = "UnetLoaderGGUF";
const NODE_CLIP: &str = "CLIPLoader";
const NODE_VAE: &str = "VAELoader";
const NODE_LORA: &str = "LoraLoader";
const NODE_KSAMPLER: &str = "KSampler";
const NODE_CONTROLNET: &str = "ControlNetLoader";
const NODE_CANNY: &str = "Canny";
const NODE_DEPTH: &str = "DepthAnythingV2Preprocessor";
const NODE_OPENPOSE: &str = "OpenposePreprocessor";
const NODE_FACE_DETAILER: &str = "FaceDetailer";
const NODE_FACE_DETECTOR: &str = "UltralyticsDetectorProvider";

/// Inpaint のワークフロー(workflow.rs の `add_inpaint`)が使うコアノード。
const INPAINT_NODES: [&str; 11] = [
    "LoadImageMask",
    "MaskToImage",
    "ImageToMask",
    "GrowMask",
    "ImageBlur",
    "ImageScale",
    "VAEEncode",
    "SetLatentNoiseMask",
    "RepeatLatentBatch",
    "RepeatImageBatch",
    "ImageCompositeMasked",
];

/// 画像から生成(workflow.rs の `add_img2img`)が使うコアノード(docs/19 §5)。
const IMG2IMG_NODES: [&str; 4] = ["LoadImage", "ImageScale", "VAEEncode", "RepeatLatentBatch"];

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

    async fn launch_args(&self) -> AppResult<Vec<String>> {
        let stats = self.get_json("/system_stats").await?;
        Ok(stats
            .as_ref()
            .and_then(|v| v.get("system")?.get("argv")?.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
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

        // 顔補正には FaceDetailer(Impact Pack)と検出モデルのローダー(Subpack)の両方が要る
        let (face, detector) = tokio::join!(
            self.node_info(NODE_FACE_DETAILER),
            self.node_info(NODE_FACE_DETECTOR),
        );
        let face_detailer = matches!(
            (face, detector),
            (Ok(f), Ok(d)) if parse::has_node(&f, NODE_FACE_DETAILER) && parse::has_node(&d, NODE_FACE_DETECTOR)
        );

        // ControlNet の前処理。Canny はコア、Depth / OpenPose は controlnet_aux が提供する
        let (canny, depth, openpose) = tokio::join!(
            self.node_info(NODE_CANNY),
            self.node_info(NODE_DEPTH),
            self.node_info(NODE_OPENPOSE),
        );
        let controlnet_preprocessors = [
            (Preprocessor::Canny, NODE_CANNY, canny),
            (Preprocessor::Depth, NODE_DEPTH, depth),
            (Preprocessor::Openpose, NODE_OPENPOSE, openpose),
        ]
        .into_iter()
        .filter(|(_, node, info)| matches!(info, Ok(i) if parse::has_node(i, node)))
        .map(|(kind, _, _)| kind)
        .collect();

        // Inpaint・img2img はコアノードだけで組む。すべて揃っているときだけ使える
        let all_core = |nodes: &'static [&'static str]| async move {
            let found = futures_util::future::join_all(nodes.iter().map(|n| async move {
                matches!(self.node_info(n).await, Ok(i) if parse::has_node(&i, n))
            }))
            .await;
            found.into_iter().all(|ok| ok)
        };
        let (inpaint, img2img) = tokio::join!(all_core(&INPAINT_NODES), all_core(&IMG2IMG_NODES));

        Ok(EngineStatus {
            connected: true,
            url: self.url(),
            version: stats.version,
            device: stats.device,
            vram_free: stats.vram_free,
            features: EngineFeatures {
                gguf_node,
                face_detailer,
                controlnet_preprocessors,
                inpaint,
                img2img,
            },
            error: None,
        })
    }

    async fn list_models(&self) -> AppResult<EngineModels> {
        let (ckpt, unet, gguf, clip, vae, lora, ks, detector, controlnet) = tokio::join!(
            self.node_info(NODE_CHECKPOINT),
            self.node_info(NODE_UNET),
            self.node_info(NODE_UNET_GGUF),
            self.node_info(NODE_CLIP),
            self.node_info(NODE_VAE),
            self.node_info(NODE_LORA),
            self.node_info(NODE_KSAMPLER),
            self.node_info(NODE_FACE_DETECTOR),
            self.node_info(NODE_CONTROLNET),
        );
        let (ckpt, unet, gguf, clip, vae, lora, ks, detector, controlnet) = (
            ckpt?,
            unet?,
            gguf?,
            clip?,
            vae?,
            lora?,
            ks?,
            detector?,
            controlnet?,
        );

        Ok(EngineModels {
            checkpoints: parse::extract_options(&ckpt, NODE_CHECKPOINT, "ckpt_name"),
            unets: parse::extract_options(&unet, NODE_UNET, "unet_name"),
            gguf_unets: parse::extract_options(&gguf, NODE_UNET_GGUF, "unet_name"),
            clips: parse::extract_options(&clip, NODE_CLIP, "clip_name"),
            vaes: parse::extract_options(&vae, NODE_VAE, "vae_name"),
            loras: parse::extract_options(&lora, NODE_LORA, "lora_name"),
            controlnets: parse::extract_options(&controlnet, NODE_CONTROLNET, "control_net_name"),
            face_detectors: parse::extract_options(&detector, NODE_FACE_DETECTOR, "model_name")
                .into_iter()
                .filter(|m| m.starts_with("bbox/"))
                .collect(),
            samplers: parse::extract_options(&ks, NODE_KSAMPLER, "sampler_name"),
            schedulers: parse::extract_options(&ks, NODE_KSAMPLER, "scheduler"),
        })
    }

    async fn upload_input(&self, png: Vec<u8>) -> AppResult<String> {
        self.upload_image(png).await
    }

    async fn submit(&self, params: &GenerationParams, inputs: &[String]) -> AppResult<String> {
        self.submit_prompt(params, inputs).await
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
    async fn health_face_detailer_needs_both_nodes() {
        let face = || {
            Route::get(
                "/object_info/FaceDetailer",
                r#"{"FaceDetailer":{"input":{}}}"#,
            )
        };
        let detector = || {
            Route::get(
                "/object_info/UltralyticsDetectorProvider",
                r#"{"UltralyticsDetectorProvider":{"input":{}}}"#,
            )
        };
        let check = |routes: Vec<Route>| async move {
            let mut all = vec![Route::get("/system_stats", STATS)];
            all.extend(routes);
            let url = spawn(all).await.url;
            ComfyUiClient::new(&url)
                .unwrap()
                .health()
                .await
                .unwrap()
                .features
                .face_detailer
        };
        assert!(check(vec![face(), detector()]).await);
        // 片方だけ(Impact Pack のみ / Subpack のみ)では使えない
        assert!(!check(vec![face()]).await);
        assert!(!check(vec![detector()]).await);
        assert!(!check(vec![]).await);
    }

    #[tokio::test]
    async fn health_lists_available_controlnet_preprocessors() {
        let node = |name: &str| {
            Route::get(
                &format!("/object_info/{name}"),
                &format!(r#"{{"{name}":{{"input":{{}}}}}}"#),
            )
        };
        let check = |routes: Vec<Route>| async move {
            let mut all = vec![Route::get("/system_stats", STATS)];
            all.extend(routes);
            let url = spawn(all).await.url;
            ComfyUiClient::new(&url)
                .unwrap()
                .health()
                .await
                .unwrap()
                .features
                .controlnet_preprocessors
        };
        // コアの Canny だけ(controlnet_aux 未導入)
        assert_eq!(check(vec![node("Canny")]).await, [Preprocessor::Canny]);
        // すべて導入済み(並びは Canny、Depth、OpenPose)
        assert_eq!(
            check(vec![
                node("OpenposePreprocessor"),
                node("Canny"),
                node("DepthAnythingV2Preprocessor"),
            ])
            .await,
            [
                Preprocessor::Canny,
                Preprocessor::Depth,
                Preprocessor::Openpose
            ]
        );
        assert!(check(vec![]).await.is_empty());
    }

    #[tokio::test]
    async fn launch_args_reads_system_argv() {
        let stats = r#"{"system":{"argv":["C:\\w\\main.py","--base-directory","C:\\w\\data",7]},"devices":[]}"#;
        let url = spawn(vec![Route::get("/system_stats", stats)]).await.url;
        let args = ComfyUiClient::new(&url)
            .unwrap()
            .launch_args()
            .await
            .unwrap();
        assert_eq!(
            args,
            vec!["C:\\w\\main.py", "--base-directory", "C:\\w\\data"]
        );

        // argv が無い応答(古いバージョンなど)は空
        let url = spawn(vec![Route::get("/system_stats", STATS)]).await.url;
        let args = ComfyUiClient::new(&url)
            .unwrap()
            .launch_args()
            .await
            .unwrap();
        assert!(args.is_empty() || args.iter().all(|a| !a.is_empty()));
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
            Route::get("/object_info/ControlNetLoader", r#"{"ControlNetLoader":{"input":{"required":{"control_net_name":[["canny.safetensors","depth.safetensors"]]}}}}"#),
            Route::get("/object_info/UltralyticsDetectorProvider", r#"{"UltralyticsDetectorProvider":{"input":{"required":{"model_name":[["bbox/face_yolov8m.pt","bbox/hand_yolov8s.pt","segm/person_yolov8m-seg.pt"]]}}}}"#),
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
        assert_eq!(m.controlnets, ["canny.safetensors", "depth.safetensors"]);
        // 検出モデルは bbox/ のものだけ(segm/ は顔補正に使わない)
        assert_eq!(
            m.face_detectors,
            ["bbox/face_yolov8m.pt", "bbox/hand_yolov8s.pt"]
        );
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
