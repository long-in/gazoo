//! ローカル LLM の導入物の固定マニフェスト(docs/30 §3)。
//!
//! ランタイム(llama.cpp のビルド)とモデル(GGUF)の URL・SHA-256・サイズを固定する。
//! 取得元は許可ホストの https のみ。

use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::services::engine_install::manifest::{Platform, Source};
use crate::services::models::download::DownloadPolicy;

const EMBEDDED: &str = include_str!("manifest.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ArchiveKind {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "tar.gz")]
    TarGz,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAsset {
    pub kind: ArchiveKind,
    /// 展開先(`bin/<build>/`)からの `llama-server` の相対パス。
    pub exe: String,
    pub source: Source,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimePin {
    pub build: String,
    pub license: String,
    #[serde(rename = "windows-x64")]
    pub windows_x64: RuntimeAsset,
    #[serde(rename = "macos-arm64")]
    pub macos_arm64: RuntimeAsset,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPin {
    pub name: String,
    pub quant: String,
    pub license: String,
    #[allow(dead_code)]
    pub repo: String,
    /// 記録用(URL がこのリビジョンに固定されていることをテストで確認する)。
    pub revision: String,
    pub file_name: String,
    pub source: Source,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[allow(dead_code)]
    pub schema_version: u32,
    pub manifest_version: String,
    pub allowed_hosts: Vec<String>,
    pub runtime: RuntimePin,
    pub model: ModelPin,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::internal(format!("llm manifest: {}", message.into()))
}

impl Manifest {
    pub fn embedded() -> AppResult<Self> {
        let m: Self = serde_json::from_str(EMBEDDED)?;
        m.validate()?;
        Ok(m)
    }

    pub fn policy(&self) -> DownloadPolicy {
        DownloadPolicy::with_hosts(self.allowed_hosts.clone())
    }

    pub fn runtime_asset(&self, platform: Platform) -> &RuntimeAsset {
        match platform {
            Platform::WindowsX64 => &self.runtime.windows_x64,
            Platform::MacosArm64 => &self.runtime.macos_arm64,
        }
    }

    /// 表示用のモデル名(例: `Qwen3.5-9B (Q4_K_M)`)。
    pub fn model_label(&self) -> String {
        format!("{} ({})", self.model.name, self.model.quant)
    }

    pub fn validate(&self) -> AppResult<()> {
        let policy = self.policy();
        let check = |label: &str, s: &Source| -> AppResult<()> {
            let url = reqwest::Url::parse(&s.url).map_err(|_| invalid(format!("{label}: url")))?;
            if !policy.url_allowed(&url) {
                return Err(invalid(format!("{label}: host not allowed")));
            }
            if s.sha256.len() != 64
                || !s
                    .sha256
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
            {
                return Err(invalid(format!("{label}: sha256")));
            }
            if s.size_bytes == 0 {
                return Err(invalid(format!("{label}: size")));
            }
            Ok(())
        };
        check("windows runtime", &self.runtime.windows_x64.source)?;
        check("macos runtime", &self.runtime.macos_arm64.source)?;
        check("model", &self.model.source)?;
        for (label, asset) in [
            ("windows", &self.runtime.windows_x64),
            ("macos", &self.runtime.macos_arm64),
        ] {
            // ビルド番号が URL に含まれていること(別ビルドの取り違えを防ぐ)
            if !asset.source.url.contains(&self.runtime.build) {
                return Err(invalid(format!("{label} runtime: build mismatch")));
            }
            if asset.exe.contains("..") || asset.exe.starts_with('/') || asset.exe.contains('\\') {
                return Err(invalid(format!("{label} runtime: exe path")));
            }
        }
        // モデルの URL はリビジョン固定(ブランチ名のままにしない)
        if !self.model.source.url.contains(&self.model.revision) {
            return Err(invalid("model: revision not pinned"));
        }
        if !self.model.source.url.ends_with(&self.model.file_name) {
            return Err(invalid("model: file name"));
        }
        if self.model.file_name.contains(['/', '\\']) || !self.model.file_name.ends_with(".gguf") {
            return Err(invalid("model: file name"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_is_valid() {
        let m = Manifest::embedded().unwrap();
        assert_eq!(m.model.quant, "Q4_K_M");
        assert_eq!(m.runtime.build, "b12426");
        assert!(m.model_label().contains("Qwen3.5-9B"));
    }

    #[test]
    fn rejects_unpinned_or_foreign_sources() {
        let mut m = Manifest::embedded().unwrap();
        m.model.source.url = m.model.source.url.replace(&m.model.revision, "main");
        assert!(m.validate().is_err());

        let mut m = Manifest::embedded().unwrap();
        m.runtime.windows_x64.source.url = "https://example.com/llama-b12426.zip".into();
        assert!(m.validate().is_err());

        let mut m = Manifest::embedded().unwrap();
        m.model.source.sha256 = "xyz".into();
        assert!(m.validate().is_err());

        let mut m = Manifest::embedded().unwrap();
        m.runtime.macos_arm64.exe = "../evil".into();
        assert!(m.validate().is_err());
    }
}
