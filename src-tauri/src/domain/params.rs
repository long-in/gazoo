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
}

impl GenerationParams {
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
