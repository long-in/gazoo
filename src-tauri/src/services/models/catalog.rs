//! モデルカタログ(docs/12-model-downloader.md §4)。
//!
//! アプリに同梱した JSON(`resources/models/catalog.json`)を読み込み、**構造と安全性を検証**する。
//! 取得元・ハッシュ・配置先・ファイル名はすべてここで固定され、実行時に外部から差し替えられない。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::download::DownloadPolicy;
use crate::error::{AppError, AppResult};

const EMBEDDED: &str = include_str!("../../../resources/models/catalog.json");
const SCHEMA_VERSION: u32 = 1;
/// 配置を許すファイルの拡張子(ComfyUI のモデルとして読まれる形式)。
const ALLOWED_EXTENSIONS: &[&str] = &["safetensors", "pt", "ckpt", "pth"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Localized {
    pub ja: String,
    pub en: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Checkpoint,
    Controlnet,
    Detector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseModel {
    Sd15,
    Sdxl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Fp16,
    Fp32,
}

/// `models/` 直下からの配置先。列挙にして、カタログから任意のパスを指定できないようにする。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DestDir {
    #[serde(rename = "checkpoints")]
    Checkpoints,
    #[serde(rename = "controlnet")]
    Controlnet,
    #[serde(rename = "ultralytics/bbox")]
    UltralyticsBbox,
}

impl DestDir {
    /// `models/` からのパス要素(区切り文字を直書きしない)。
    pub fn components(self) -> &'static [&'static str] {
        match self {
            Self::Checkpoints => &["checkpoints"],
            Self::Controlnet => &["controlnet"],
            Self::UltralyticsBbox => &["ultralytics", "bbox"],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommercialUse {
    Flag(bool),
    /// `"see-license"`(ライセンス本文を確認する)。
    Note(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct License {
    pub name: String,
    pub url: String,
    pub commercial_use: CommercialUse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub id: String,
    pub label: Localized,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_model: Option<BaseModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<Precision>,
    #[serde(default)]
    pub recommended: bool,
    pub source: Source,
    pub size_bytes: u64,
    pub sha256: String,
    pub dest_dir: DestDir,
    pub file_name: String,
    pub license: License,
    #[serde(default)]
    pub requires_consent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<Localized>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Family {
    pub id: String,
    pub category: Category,
    pub name: Localized,
    pub description: Localized,
    #[serde(default)]
    pub used_by: Vec<String>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema_version: u32,
    pub families: Vec<Family>,
}

impl Catalog {
    /// 同梱カタログを読み込んで検証する。
    pub fn embedded(policy: &DownloadPolicy) -> AppResult<Self> {
        Self::parse(EMBEDDED, policy)
    }

    pub fn parse(json: &str, policy: &DownloadPolicy) -> AppResult<Self> {
        let catalog: Catalog = serde_json::from_str(json)
            .map_err(|e| AppError::internal(format!("モデルカタログを読めません: {e}")))?;
        catalog.validate(policy)?;
        Ok(catalog)
    }

    pub fn find(&self, family_id: &str, variant_id: &str) -> Option<(&Family, &Variant)> {
        let family = self.families.iter().find(|f| f.id == family_id)?;
        let variant = family.variants.iter().find(|v| v.id == variant_id)?;
        Some((family, variant))
    }

    fn validate(&self, policy: &DownloadPolicy) -> AppResult<()> {
        let bad = |msg: String| {
            Err(AppError::internal(format!(
                "モデルカタログが不正です: {msg}"
            )))
        };
        if self.schema_version != SCHEMA_VERSION {
            return bad(format!(
                "schemaVersion {} は未対応です",
                self.schema_version
            ));
        }
        let mut family_ids = HashSet::new();
        let mut targets = HashSet::new();
        for f in &self.families {
            if f.id.is_empty() || !family_ids.insert(f.id.as_str()) {
                return bad(format!("ファミリー ID が空か重複しています: {}", f.id));
            }
            if f.variants.is_empty() {
                return bad(format!("{}: variant がありません", f.id));
            }
            let mut variant_ids = HashSet::new();
            let mut recommended = HashSet::new();
            for v in &f.variants {
                let at = format!("{}/{}", f.id, v.id);
                if v.id.is_empty() || !variant_ids.insert(v.id.as_str()) {
                    return bad(format!("{at}: variant ID が空か重複しています"));
                }
                if v.recommended && !recommended.insert(v.base_model) {
                    return bad(format!("{at}: 同じベースモデルに推奨が複数あります"));
                }
                if v.size_bytes == 0 {
                    return bad(format!("{at}: sizeBytes が 0 です"));
                }
                if !is_sha256(&v.sha256) {
                    return bad(format!(
                        "{at}: sha256 は小文字 16 進 64 桁で指定してください"
                    ));
                }
                if let Err(why) = check_file_name(&v.file_name) {
                    return bad(format!("{at}: fileName が不正です({why})"));
                }
                if !targets.insert((v.dest_dir, v.file_name.to_ascii_lowercase())) {
                    return bad(format!(
                        "{at}: 配置先のファイルが他の variant と重複しています"
                    ));
                }
                match reqwest::Url::parse(&v.source.url) {
                    Ok(url) if policy.url_allowed(&url) => {}
                    _ => return bad(format!("{at}: 取得元 URL が許可されていません")),
                }
                if let CommercialUse::Note(n) = &v.license.commercial_use
                    && n != "see-license"
                {
                    return bad(format!("{at}: commercialUse が不正です"));
                }
            }
        }
        Ok(())
    }
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 配置するファイル名の安全性。区切り文字・`..`・Windows の代替データストリーム(`:`)などを拒否する。
fn check_file_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("空");
    }
    if name.starts_with('.') || name.contains("..") {
        return Err("先頭のドットまたは ..");
    }
    if name.chars().any(|c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return Err("使えない文字");
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext {
        Some(e) if ALLOWED_EXTENSIONS.contains(&e.as_str()) => Ok(()),
        _ => Err("未対応の拡張子"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> DownloadPolicy {
        DownloadPolicy::production()
    }

    #[test]
    fn embedded_catalog_is_valid() {
        let c = Catalog::embedded(&policy()).unwrap();
        assert!(c.families.len() >= 6);
        // 初期カタログの要点(docs/12 §4.2)
        let (_, v) = c.find("controlnet-canny", "sdxl-xinsir").unwrap();
        assert_eq!(v.base_model, Some(BaseModel::Sdxl));
        assert!(v.file_name.contains("canny"));
        let (_, face) = c.find("detector-face", "yolov9c").unwrap();
        assert_eq!(face.dest_dir, DestDir::UltralyticsBbox);
        assert!(face.note.is_some(), "pickle 形式の注意書きがある");
    }

    #[test]
    fn controlnet_file_names_carry_the_preprocessor_for_auto_switch() {
        // 前処理に応じた ControlNet の自動切り替え(docs/10 §8)が、ファイル名の語に依存する
        let c = Catalog::embedded(&policy()).unwrap();
        for f in c
            .families
            .iter()
            .filter(|f| f.category == Category::Controlnet)
        {
            let word = f.id.trim_start_matches("controlnet-");
            for v in &f.variants {
                assert!(v.file_name.contains(word), "{}/{}", f.id, v.id);
            }
        }
    }

    #[test]
    fn embedded_catalog_pins_revisions_and_every_checkpoint_has_a_base_model() {
        let c = Catalog::embedded(&policy()).unwrap();
        for f in &c.families {
            for v in &f.variants {
                assert!(v.source.url.contains("/resolve/"), "{}", v.id);
                assert!(
                    !v.source.url.contains("/resolve/main/"),
                    "リビジョンを固定する: {}",
                    v.id
                );
                if f.category != Category::Detector {
                    assert!(v.base_model.is_some(), "{}/{}", f.id, v.id);
                }
            }
        }
    }

    fn catalog_with(file_name: &str, url: &str, sha: &str) -> String {
        format!(
            r#"{{"schemaVersion":1,"families":[{{"id":"f","category":"checkpoint",
            "name":{{"ja":"a","en":"a"}},"description":{{"ja":"a","en":"a"}},
            "variants":[{{"id":"v","label":{{"ja":"a","en":"a"}},"source":{{"url":"{url}","repo":"r"}},
            "sizeBytes":10,"sha256":"{sha}","destDir":"checkpoints","fileName":"{file_name}",
            "license":{{"name":"l","url":"https://x.test","commercialUse":true}}}}]}}]}}"#
        )
    }

    const SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const OK_URL: &str = "https://huggingface.co/a/b/resolve/abc/m.safetensors";

    #[test]
    fn accepts_a_minimal_valid_catalog() {
        Catalog::parse(&catalog_with("m.safetensors", OK_URL, SHA), &policy()).unwrap();
    }

    #[test]
    fn rejects_unsafe_file_names() {
        for name in [
            "../m.safetensors",
            "sub/m.safetensors",
            "sub\\\\m.safetensors",
            "m.safetensors:evil",
            ".hidden.safetensors",
            "m.exe",
            "m",
            "",
        ] {
            let json = catalog_with(name, OK_URL, SHA);
            assert!(
                Catalog::parse(&json, &policy()).is_err(),
                "{name:?} は拒否される"
            );
        }
    }

    #[test]
    fn rejects_disallowed_hosts_and_schemes() {
        for url in [
            "https://evil.example.com/m.safetensors",
            "http://huggingface.co/m.safetensors",
            "https://huggingface.co.evil.com/m.safetensors",
            "https://nothuggingface.co/m.safetensors",
            "file:///etc/passwd",
        ] {
            let json = catalog_with("m.safetensors", url, SHA);
            assert!(
                Catalog::parse(&json, &policy()).is_err(),
                "{url} は拒否される"
            );
        }
    }

    #[test]
    fn rejects_malformed_hashes() {
        for sha in ["", "abc", &"G".repeat(64), &"A".repeat(64)] {
            let json = catalog_with("m.safetensors", OK_URL, sha);
            assert!(
                Catalog::parse(&json, &policy()).is_err(),
                "{sha:?} は拒否される"
            );
        }
    }

    #[test]
    fn rejects_duplicate_targets() {
        let one = catalog_with("m.safetensors", OK_URL, SHA);
        let mut v: serde_json::Value = serde_json::from_str(&one).unwrap();
        let fam = v["families"][0].clone();
        v["families"].as_array_mut().unwrap().push(fam);
        assert!(Catalog::parse(&v.to_string(), &policy()).is_err());
    }
}
