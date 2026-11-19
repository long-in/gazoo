//! 生成パラメータ(docs/03-data-model.md「params_json」)。エンジン非依存。

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// JS の `Number` で安全に扱える最大の整数(2^53 - 1)。シードの上限。
pub const MAX_SEED: u64 = (1 << 53) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Sd15,
    Sdxl,
    Flux,
    FluxGguf,
}

impl ModelKind {
    pub fn is_flux(self) -> bool {
        matches!(self, Self::Flux | Self::FluxGguf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub kind: ModelKind,
    pub name: String,
    /// Flux 系: `[T5, CLIP-L]` の順。SD 系は空。
    #[serde(default)]
    pub clip: Vec<String>,
    /// Flux 系は必須。SD 系は未指定ならチェックポイント内蔵の VAE を使う。
    #[serde(default)]
    pub vae: Option<String>,
}

/// 顔補正(ADetailer 相当。docs/10-assist-features.md §4)。SD 1.5 / SDXL のみ。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceDetailerParams {
    #[serde(default)]
    pub enabled: bool,
    /// 検出モデル(`UltralyticsDetectorProvider` の `bbox/…`)。
    #[serde(default)]
    pub detector: String,
    /// 補正の強さ(再サンプリングの denoise)。
    #[serde(default = "default_face_denoise")]
    pub denoise: f64,
    /// 補正用プロンプト。空なら本体のプロンプトを使う。
    #[serde(default)]
    pub prompt: String,
    /// 検出しきい値。
    #[serde(default = "default_face_confidence")]
    pub confidence: f64,
}

/// ControlNet の前処理(docs/10-assist-features.md §5)。参照画像を条件画像に変換する方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preprocessor {
    Canny,
    Depth,
    Openpose,
    /// 前処理なし(線画・深度マップ・ポーズ画像として用意した画像をそのまま使う)。
    None,
}

/// ControlNet の 1 ユニット。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlnetUnit {
    /// 参照画像。画像保存先からの相対パス(`inputs/<uuid>.png`、`/` 区切り)。
    pub image: String,
    /// ControlNet モデル(`ControlNetLoader` の候補)。
    pub model: String,
    #[serde(default = "default_preprocessor")]
    pub preprocessor: Preprocessor,
    #[serde(default = "default_strength")]
    pub strength: f64,
    #[serde(default)]
    pub start_percent: f64,
    #[serde(default = "default_end_percent")]
    pub end_percent: f64,
}

/// ControlNet(docs/10 §5)。SD 1.5 / SDXL のみ。v0.2 は 1 ユニットまで。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlnetParams {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub units: Vec<ControlnetUnit>,
}

/// Inpaint(docs/13 §3.2)。SD 1.5 / SDXL のみ。マスクの白い範囲だけを描き直す。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InpaintParams {
    #[serde(default)]
    pub enabled: bool,
    /// 参照画像(`inputs/<uuid>.png`)。
    #[serde(default)]
    pub image: String,
    /// マスク(`inputs/<uuid>.png`。白 = 描き直す範囲)。
    #[serde(default)]
    pub mask: String,
    #[serde(default = "default_inpaint_denoise")]
    pub denoise: f64,
    /// マスクを広げる量(px)。
    #[serde(default = "default_inpaint_grow")]
    pub grow_mask: u32,
    /// 境界のぼかし(px)。
    #[serde(default = "default_inpaint_feather")]
    pub feather: u32,
    /// 結果を元画像へ合成し直す(マスク外を元画像と一致させる)。
    #[serde(default = "default_true")]
    pub composite_back: bool,
}

fn default_inpaint_denoise() -> f64 {
    0.7
}
fn default_inpaint_grow() -> u32 {
    8
}
fn default_inpaint_feather() -> u32 {
    8
}
fn default_true() -> bool {
    true
}

fn default_preprocessor() -> Preprocessor {
    Preprocessor::Canny
}
fn default_strength() -> f64 {
    1.0
}
fn default_end_percent() -> f64 {
    1.0
}

/// 参照画像の相対パスとして妥当か(`inputs/` 直下の安全なファイル名だけを許す)。
/// ここを通ったパスだけを、保存先に結合してファイルを読む。
pub fn is_valid_input_path(rel: &str) -> bool {
    let Some(name) = rel.strip_prefix("inputs/") else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn default_face_denoise() -> f64 {
    0.4
}
fn default_face_confidence() -> f64 {
    0.5
}

fn default_schema_version() -> u32 {
    1
}
fn default_cfg() -> f64 {
    7.0
}
fn default_guidance() -> f64 {
    3.5
}
fn default_batch() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationParams {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub model: ModelSpec,
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    #[serde(default = "default_cfg")]
    pub cfg: f64,
    /// Flux 系の FluxGuidance。
    #[serde(default = "default_guidance")]
    pub guidance: f64,
    pub sampler: String,
    pub scheduler: String,
    pub seed: u64,
    #[serde(default)]
    pub seed_randomize: bool,
    #[serde(default = "default_batch")]
    pub batch_size: u32,
    /// 顔補正。無ければ無効(docs/03「生成補助機能のフィールド」)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_detailer: Option<FaceDetailerParams>,
    /// ControlNet。無ければ無効。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlnet: Option<ControlnetParams>,
    /// Inpaint。無ければ無効。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint: Option<InpaintParams>,
}

impl GenerationParams {
    /// 有効な Inpaint の設定(無効・未指定なら `None`)。
    pub fn active_inpaint(&self) -> Option<&InpaintParams> {
        self.inpaint.as_ref().filter(|i| i.enabled)
    }

    /// エンジンへ送る画像の数。順序は ControlNet の各ユニット、Inpaint の画像、Inpaint のマスク。
    pub fn upload_count(&self) -> usize {
        self.active_controlnet_units().len()
            + if self.active_inpaint().is_some() {
                2
            } else {
                0
            }
    }

    /// 有効な ControlNet のユニット(無効・未指定・ユニットなしなら空)。
    pub fn active_controlnet_units(&self) -> &[ControlnetUnit] {
        match &self.controlnet {
            Some(c) if c.enabled => &c.units,
            _ => &[],
        }
    }

    /// 有効な顔補正の設定(無効・未指定なら `None`)。
    pub fn active_face_detailer(&self) -> Option<&FaceDetailerParams> {
        self.face_detailer.as_ref().filter(|f| f.enabled)
    }

    pub fn validate(&self) -> AppResult<()> {
        let bad = |field: &str, msg: &str| Err(AppError::validation(field, msg));

        if self.prompt.trim().is_empty() {
            return bad("prompt", "プロンプトを入力してください");
        }
        if self.model.name.trim().is_empty() {
            return bad("model", "モデルを選択してください");
        }
        for (field, v) in [("width", self.width), ("height", self.height)] {
            if !(256..=2048).contains(&v) || v % 64 != 0 {
                return bad(field, "256〜2048 の 64 の倍数で指定してください");
            }
        }
        if !(1..=150).contains(&self.steps) {
            return bad("steps", "ステップ数は 1〜150 で指定してください");
        }
        if !(0.0..=30.0).contains(&self.cfg) || !self.cfg.is_finite() {
            return bad("cfg", "CFG は 0〜30 で指定してください");
        }
        if !(0.0..=100.0).contains(&self.guidance) || !self.guidance.is_finite() {
            return bad("guidance", "Guidance は 0〜100 で指定してください");
        }
        if self.sampler.trim().is_empty() {
            return bad("sampler", "サンプラーを選択してください");
        }
        if self.scheduler.trim().is_empty() {
            return bad("scheduler", "スケジューラを選択してください");
        }
        if self.seed > MAX_SEED {
            return bad("seed", "シードが大きすぎます");
        }
        if !(1..=8).contains(&self.batch_size) {
            return bad("batchSize", "枚数は 1〜8 で指定してください");
        }
        if self.model.kind.is_flux() {
            if self.model.clip.len() != 2 || self.model.clip.iter().any(|c| c.trim().is_empty()) {
                return bad("clip", "テキストエンコーダ(T5 と CLIP-L)を選択してください");
            }
            if self
                .model
                .vae
                .as_deref()
                .is_none_or(|v| v.trim().is_empty())
            {
                return bad("vae", "VAE を選択してください");
            }
        }
        if let Some(fd) = self.active_face_detailer() {
            if self.model.kind.is_flux() {
                return bad("faceDetailer", "顔補正は SD 1.5 / SDXL のみ対応です");
            }
            if fd.detector.trim().is_empty() {
                return bad("faceDetailer.detector", "検出モデルを選択してください");
            }
            if !(0.05..=1.0).contains(&fd.denoise) || !fd.denoise.is_finite() {
                return bad(
                    "faceDetailer.denoise",
                    "強さは 0.05〜1.0 で指定してください",
                );
            }
            if !(0.1..=1.0).contains(&fd.confidence) || !fd.confidence.is_finite() {
                return bad(
                    "faceDetailer.confidence",
                    "検出しきい値は 0.1〜1.0 で指定してください",
                );
            }
        }
        if self.controlnet.as_ref().is_some_and(|c| c.enabled) {
            self.validate_controlnet()?;
        }
        if let Some(ip) = self.active_inpaint() {
            self.validate_inpaint(ip)?;
        }
        Ok(())
    }

    fn validate_inpaint(&self, ip: &InpaintParams) -> AppResult<()> {
        let bad = |field: &str, msg: &str| Err(AppError::validation(field, msg));
        if self.model.kind.is_flux() {
            return bad("inpaint", "Inpaint は SD 1.5 / SDXL のみ対応です");
        }
        if self.active_face_detailer().is_some() {
            return bad("inpaint", "Inpaint と顔補正は同時に使えません");
        }
        if !is_valid_input_path(&ip.image) {
            return bad("inpaint.image", "参照画像を選択してください");
        }
        if !is_valid_input_path(&ip.mask) {
            return bad("inpaint.mask", "マスクを描いてください");
        }
        if !(0.05..=1.0).contains(&ip.denoise) || !ip.denoise.is_finite() {
            return bad("inpaint.denoise", "強さは 0.05〜1.0 で指定してください");
        }
        if ip.grow_mask > 64 {
            return bad(
                "inpaint.growMask",
                "マスクの拡張は 0〜64 で指定してください",
            );
        }
        if ip.feather > 31 {
            return bad("inpaint.feather", "境界のぼかしは 0〜31 で指定してください");
        }
        Ok(())
    }

    fn validate_controlnet(&self) -> AppResult<()> {
        let bad = |field: &str, msg: &str| Err(AppError::validation(field, msg));
        if self.model.kind.is_flux() {
            return bad("controlnet", "ControlNet は SD 1.5 / SDXL のみ対応です");
        }
        let units = self.active_controlnet_units();
        if units.is_empty() {
            return bad("controlnet.units", "参照画像を選択してください");
        }
        if units.len() > 1 {
            return bad("controlnet.units", "ControlNet は 1 つまで指定できます");
        }
        let u = &units[0];
        if u.image.trim().is_empty() {
            return bad("controlnet.units[0].image", "参照画像を選択してください");
        }
        if !is_valid_input_path(&u.image) {
            return bad("controlnet.units[0].image", "参照画像のパスが不正です");
        }
        if u.model.trim().is_empty() {
            return bad(
                "controlnet.units[0].model",
                "ControlNet モデルを選択してください",
            );
        }
        if !(0.0..=2.0).contains(&u.strength) || !u.strength.is_finite() {
            return bad(
                "controlnet.units[0].strength",
                "強さは 0〜2 で指定してください",
            );
        }
        let range_ok = (0.0..=1.0).contains(&u.start_percent)
            && (0.0..=1.0).contains(&u.end_percent)
            && u.start_percent < u.end_percent;
        if !range_ok {
            return bad(
                "controlnet.units[0].endPercent",
                "適用範囲は 0〜1 で、開始より終了を大きくしてください",
            );
        }
        Ok(())
    }
}

#[cfg(test)]
pub fn sample(kind: ModelKind) -> GenerationParams {
    let flux = kind.is_flux();
    GenerationParams {
        schema_version: 1,
        model: ModelSpec {
            kind,
            name: if flux {
                "flux.gguf".into()
            } else {
                "model.safetensors".into()
            },
            clip: if flux {
                vec!["t5xxl.safetensors".into(), "clip_l.safetensors".into()]
            } else {
                vec![]
            },
            vae: flux.then(|| "ae.safetensors".to_string()),
        },
        prompt: "a cat".into(),
        negative_prompt: "blurry".into(),
        width: 512,
        height: 512,
        steps: 20,
        cfg: 7.0,
        guidance: 3.5,
        sampler: "euler".into(),
        scheduler: "normal".into(),
        seed: 42,
        seed_randomize: false,
        batch_size: 1,
        face_detailer: None,
        controlnet: None,
        inpaint: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_of(r: AppResult<()>) -> String {
        match r.unwrap_err() {
            AppError::Validation { field, .. } => field,
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn valid_samples_pass() {
        for k in [
            ModelKind::Sd15,
            ModelKind::Sdxl,
            ModelKind::Flux,
            ModelKind::FluxGguf,
        ] {
            sample(k).validate().unwrap();
        }
    }

    #[test]
    fn rejects_each_invalid_field() {
        let mut p = sample(ModelKind::Sd15);
        p.prompt = "  ".into();
        assert_eq!(field_of(p.validate()), "prompt");

        let mut p = sample(ModelKind::Sd15);
        p.width = 500; // 64 の倍数でない
        assert_eq!(field_of(p.validate()), "width");

        let mut p = sample(ModelKind::Sd15);
        p.height = 4096;
        assert_eq!(field_of(p.validate()), "height");

        let mut p = sample(ModelKind::Sd15);
        p.steps = 0;
        assert_eq!(field_of(p.validate()), "steps");

        let mut p = sample(ModelKind::Sd15);
        p.cfg = f64::NAN;
        assert_eq!(field_of(p.validate()), "cfg");

        let mut p = sample(ModelKind::Sd15);
        p.seed = MAX_SEED + 1;
        assert_eq!(field_of(p.validate()), "seed");

        let mut p = sample(ModelKind::Sd15);
        p.batch_size = 9;
        assert_eq!(field_of(p.validate()), "batchSize");
    }

    #[test]
    fn flux_requires_clip_and_vae() {
        let mut p = sample(ModelKind::Flux);
        p.model.clip.pop();
        assert_eq!(field_of(p.validate()), "clip");

        let mut p = sample(ModelKind::FluxGguf);
        p.model.vae = None;
        assert_eq!(field_of(p.validate()), "vae");
    }

    fn with_face(kind: ModelKind) -> GenerationParams {
        let mut p = sample(kind);
        p.face_detailer = Some(FaceDetailerParams {
            enabled: true,
            detector: "bbox/face_yolov8m.pt".into(),
            denoise: 0.4,
            prompt: String::new(),
            confidence: 0.5,
        });
        p
    }

    #[test]
    fn face_detailer_is_valid_for_sd_only() {
        with_face(ModelKind::Sd15).validate().unwrap();
        with_face(ModelKind::Sdxl).validate().unwrap();
        for k in [ModelKind::Flux, ModelKind::FluxGguf] {
            assert_eq!(field_of(with_face(k).validate()), "faceDetailer");
        }
        // 無効のままなら Flux でも通る(モデル切替で残った設定を弾かない)
        let mut p = with_face(ModelKind::Flux);
        p.face_detailer.as_mut().unwrap().enabled = false;
        p.validate().unwrap();
        assert!(p.active_face_detailer().is_none());
    }

    #[test]
    fn face_detailer_rejects_bad_values() {
        let mut p = with_face(ModelKind::Sd15);
        p.face_detailer.as_mut().unwrap().detector = " ".into();
        assert_eq!(field_of(p.validate()), "faceDetailer.detector");

        for bad in [0.0, 1.5, f64::NAN] {
            let mut p = with_face(ModelKind::Sd15);
            p.face_detailer.as_mut().unwrap().denoise = bad;
            assert_eq!(field_of(p.validate()), "faceDetailer.denoise");
        }
        for bad in [0.0, 1.1] {
            let mut p = with_face(ModelKind::Sd15);
            p.face_detailer.as_mut().unwrap().confidence = bad;
            assert_eq!(field_of(p.validate()), "faceDetailer.confidence");
        }
    }

    #[test]
    fn face_detailer_is_optional_and_round_trips() {
        // 古いデータ(フィールド無し)は無効として読める。保存時にも出力しない
        let p = sample(ModelKind::Sd15);
        let v = serde_json::to_value(&p).unwrap();
        assert!(v.get("faceDetailer").is_none());

        let p: GenerationParams = serde_json::from_str(
            r#"{"model":{"kind":"sd15","name":"m"},"prompt":"x","width":512,"height":512,
                "steps":20,"sampler":"euler","scheduler":"normal","seed":1,
                "faceDetailer":{"enabled":true,"detector":"bbox/face_yolov8m.pt"}}"#,
        )
        .unwrap();
        let fd = p.active_face_detailer().unwrap();
        assert_eq!((fd.denoise, fd.confidence), (0.4, 0.5));
        let again: GenerationParams =
            serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(again, p);
    }

    fn with_cn(kind: ModelKind) -> GenerationParams {
        let mut p = sample(kind);
        p.controlnet = Some(ControlnetParams {
            enabled: true,
            units: vec![ControlnetUnit {
                image: "inputs/0190-abc.png".into(),
                model: "control_canny.safetensors".into(),
                preprocessor: Preprocessor::Canny,
                strength: 1.0,
                start_percent: 0.0,
                end_percent: 1.0,
            }],
        });
        p
    }

    fn unit(p: &mut GenerationParams) -> &mut ControlnetUnit {
        &mut p.controlnet.as_mut().unwrap().units[0]
    }

    #[test]
    fn controlnet_is_valid_for_sd_only() {
        with_cn(ModelKind::Sd15).validate().unwrap();
        with_cn(ModelKind::Sdxl).validate().unwrap();
        for k in [ModelKind::Flux, ModelKind::FluxGguf] {
            assert_eq!(field_of(with_cn(k).validate()), "controlnet");
        }
        // 無効のままなら Flux でも通り、ユニットも作られない
        let mut p = with_cn(ModelKind::Flux);
        p.controlnet.as_mut().unwrap().enabled = false;
        p.validate().unwrap();
        assert!(p.active_controlnet_units().is_empty());
    }

    fn with_inpaint(kind: ModelKind) -> GenerationParams {
        let mut p = sample(kind);
        p.inpaint = Some(InpaintParams {
            enabled: true,
            image: "inputs/a.png".into(),
            mask: "inputs/m.png".into(),
            denoise: 0.7,
            grow_mask: 8,
            feather: 8,
            composite_back: true,
        });
        p
    }

    #[test]
    fn inpaint_valid_for_sd_and_rejected_for_flux_or_with_face_detailer() {
        for k in [ModelKind::Sd15, ModelKind::Sdxl] {
            assert!(with_inpaint(k).validate().is_ok());
        }
        for k in [ModelKind::Flux, ModelKind::FluxGguf] {
            assert_eq!(field_of(with_inpaint(k).validate()), "inpaint");
        }
        let mut p = with_inpaint(ModelKind::Sd15);
        p.face_detailer = Some(FaceDetailerParams {
            enabled: true,
            detector: "bbox/face.pt".into(),
            denoise: 0.4,
            prompt: String::new(),
            confidence: 0.5,
        });
        assert_eq!(field_of(p.validate()), "inpaint");
        // 無効なら何も検証しない(Flux でも通る)
        let mut off = with_inpaint(ModelKind::Flux);
        off.inpaint.as_mut().unwrap().enabled = false;
        assert!(off.validate().is_ok());
    }

    #[test]
    fn inpaint_rejects_bad_values() {
        type Tweak = fn(&mut InpaintParams);
        let cases: [(&str, Tweak); 6] = [
            ("inpaint.image", |i| i.image = "../x.png".into()),
            ("inpaint.mask", |i| i.mask = String::new()),
            ("inpaint.denoise", |i| i.denoise = 0.0),
            ("inpaint.denoise", |i| i.denoise = f64::NAN),
            ("inpaint.growMask", |i| i.grow_mask = 65),
            ("inpaint.feather", |i| i.feather = 32),
        ];
        for (field, f) in cases {
            let mut p = with_inpaint(ModelKind::Sd15);
            f(p.inpaint.as_mut().unwrap());
            assert_eq!(field_of(p.validate()), field);
        }
    }

    #[test]
    fn upload_count_covers_controlnet_and_inpaint() {
        assert_eq!(sample(ModelKind::Sd15).upload_count(), 0);
        let mut p = with_inpaint(ModelKind::Sd15);
        assert_eq!(p.upload_count(), 2);
        p.controlnet = with_cn(ModelKind::Sd15).controlnet;
        assert_eq!(p.upload_count(), 3);
    }

    #[test]
    fn inpaint_defaults_apply_to_a_minimal_json() {
        let ip: InpaintParams = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert_eq!(
            (ip.denoise, ip.grow_mask, ip.feather, ip.composite_back),
            (0.7, 8, 8, true)
        );
    }

    #[test]
    fn controlnet_rejects_bad_values() {
        let mut p = with_cn(ModelKind::Sd15);
        p.controlnet.as_mut().unwrap().units.clear();
        assert_eq!(field_of(p.validate()), "controlnet.units");

        let mut p = with_cn(ModelKind::Sd15);
        let extra = unit(&mut p).clone();
        p.controlnet.as_mut().unwrap().units.push(extra);
        assert_eq!(field_of(p.validate()), "controlnet.units");

        let mut p = with_cn(ModelKind::Sd15);
        unit(&mut p).image = String::new();
        assert_eq!(field_of(p.validate()), "controlnet.units[0].image");

        let mut p = with_cn(ModelKind::Sd15);
        unit(&mut p).model = " ".into();
        assert_eq!(field_of(p.validate()), "controlnet.units[0].model");

        for bad in [-0.1, 2.1, f64::NAN] {
            let mut p = with_cn(ModelKind::Sd15);
            unit(&mut p).strength = bad;
            assert_eq!(field_of(p.validate()), "controlnet.units[0].strength");
        }
        for (s, e) in [(0.5, 0.5), (0.8, 0.2), (-0.1, 1.0), (0.0, 1.1)] {
            let mut p = with_cn(ModelKind::Sd15);
            unit(&mut p).start_percent = s;
            unit(&mut p).end_percent = e;
            assert_eq!(field_of(p.validate()), "controlnet.units[0].endPercent");
        }
    }

    #[test]
    fn input_paths_must_stay_inside_inputs_dir() {
        for ok in ["inputs/a.png", "inputs/0190-ab_c.PNG"] {
            assert!(is_valid_input_path(ok), "{ok}");
        }
        for bad in [
            "",
            "inputs/",
            "a.png",
            "../inputs/a.png",
            "inputs/../a.png",
            "inputs/..",
            "inputs/.hidden",
            "inputs/sub/a.png",
            "inputs\\a.png",
            "/etc/passwd",
            "inputs/C:evil.png",
            "2026/11/a.png",
            "thumbs/a.jpg",
        ] {
            assert!(!is_valid_input_path(bad), "{bad}");
        }
        let mut p = with_cn(ModelKind::Sd15);
        unit(&mut p).image = "inputs/../secret.png".into();
        assert_eq!(field_of(p.validate()), "controlnet.units[0].image");
    }

    #[test]
    fn controlnet_is_optional_and_round_trips() {
        let v = serde_json::to_value(sample(ModelKind::Sd15)).unwrap();
        assert!(v.get("controlnet").is_none());

        let p: GenerationParams = serde_json::from_str(
            r#"{"model":{"kind":"sd15","name":"m"},"prompt":"x","width":512,"height":512,
                "steps":20,"sampler":"euler","scheduler":"normal","seed":1,
                "controlnet":{"enabled":true,"units":[{"image":"inputs/a.png","model":"c.safetensors",
                  "preprocessor":"openpose"}]}}"#,
        )
        .unwrap();
        let u = &p.active_controlnet_units()[0];
        assert_eq!(u.preprocessor, Preprocessor::Openpose);
        assert_eq!(
            (u.strength, u.start_percent, u.end_percent),
            (1.0, 0.0, 1.0)
        );
        let again: GenerationParams =
            serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(again, p);
        assert_eq!(
            serde_json::to_value(Preprocessor::None).unwrap(),
            serde_json::json!("none")
        );
    }

    #[test]
    fn deserializes_with_defaults() {
        let p: GenerationParams = serde_json::from_str(
            r#"{"model":{"kind":"sdxl","name":"m.safetensors"},"prompt":"x",
                "width":1024,"height":1024,"steps":25,"sampler":"euler","scheduler":"normal","seed":1}"#,
        )
        .unwrap();
        assert_eq!(p.schema_version, 1);
        assert_eq!(p.batch_size, 1);
        assert_eq!(p.cfg, 7.0);
        assert!(!p.seed_randomize && p.negative_prompt.is_empty());
        // 往復で値が保たれる
        let again: GenerationParams =
            serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(again, p);
    }
}
