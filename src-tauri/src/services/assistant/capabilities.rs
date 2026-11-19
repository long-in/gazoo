//! gazoo の機能カタログ(docs/13 §4)。AI サポートのシステムプロンプトに連結して渡す。
//!
//! 機能を追加・変更したら、**このファイルを更新する**(docs/09 のチェックリストにも項目がある)。
//! 画面名・ボタン名は UI の文言(`src/lib/i18n/ja.ts`)と一致させる。
//! アプリ固定の信頼できる文面であり、ユーザー入力は含めない。

use serde_json::{Value, json};

use crate::engine::EngineFeatures;

pub const CAPABILITIES: &str = r#"
# gazoo でできること(画面名・ボタン名はアプリの表示どおり)
- 画像生成(生成画面): プロンプト・ネガティブ・モデル種別(SD 1.5 / SDXL / Flux / Flux GGUF)・サイズ・steps・CFG(Flux は guidance)・サンプラー・シード・枚数を指定して生成する。「生成」でキューに入る。
- 顔補正(生成画面のスイッチ。SD 1.5 / SDXL のみ): 生成画像の顔を検出し、その部分だけを再生成して整える。強さ(denoise)で変化の大きさを調整する。ComfyUI に Impact Pack / Subpack と検出モデルが必要。
- ControlNet(生成画面のスイッチ。SD 1.5 / SDXL のみ、1 つまで): 参照画像から輪郭(Canny)・深度・ポーズを取り出し、構図の条件にする。ベースモデルに合う ControlNet モデルが必要。
- Inpaint(生成画面のスイッチ。SD 1.5 / SDXL のみ): 参照画像に「マスクを描く」で範囲を塗り、塗った範囲だけを描き直す。マスク外は「結果を元画像へ合成し直す」で元画像と一致させられる。顔を保って服・背景だけ変えたいときは、顔を塗って「反転」する。強さは 0.6〜0.8 が目安。プロンプトには描き直す部分の説明だけを書く。顔補正とは同時に使えない。参照画像は、ファイル選択か、ギャラリー詳細の「Inpaint に使う」で設定する。
- プリセット(プリセット画面): 生成設定の保存と呼び出し。
- ギャラリー: 生成画像の一覧・検索・お気に入り・書き出し。詳細の「この設定で生成」で同じ設定を再現し、「参照画像に使う」(ControlNet)や「Inpaint に使う」で画像を参照に設定できる。
- キュー: 実行中・待機中のジョブの確認、並べ替え、キャンセル、再試行。
- モデル(モデル画面): チェックポイント・ControlNet・検出モデル・Flux 一式を、バージョンを選んで取得できる。取得はユーザーが「ダウンロード」を押したときだけ行う。
- AI に相談(このパネル): フォームの設定へのアドバイス。提案はユーザーが「適用」を押して初めてフォームに反映される。

# gazoo にまだ無い機能(「ある」と案内しない。必要なら ComfyUI を直接使う方法を勧める)
- LoRA、生成中のプレビュー画像、Inpaint 以外の img2img、アウトペイント(キャンバスの拡張)、複数の ControlNet、動画生成。
- Flux 系では Inpaint・顔補正・ControlNet を使えない(SD 1.5 / SDXL のみ)。
- あなた(AI)はアプリを操作できない。モデルの切替・シード・ControlNet・Inpaint の画像やマスクは、提案ではなく手順を本文で説明する。
- 「現在のフォーム」の capabilities が false の機能は、今の ComfyUI では使えない。足りないものを案内する(例: Inpaint が false なら ComfyUI の更新)。"#;

/// システムプロンプト全体(基本の指示 + 機能カタログ)。
pub fn system_prompt() -> String {
    format!("{}\n{}", super::context::SYSTEM_PROMPT, CAPABILITIES)
}

/// 今の ComfyUI で使える機能(「現在のフォーム」に添える)。接続状態が不明なら `None`。
pub fn capabilities_json(features: Option<&EngineFeatures>) -> Option<Value> {
    let f = features?;
    Some(json!({
        "faceFix": f.face_detailer,
        "controlnet": !f.controlnet_preprocessors.is_empty(),
        "controlnetPreprocessors": f.controlnet_preprocessors,
        "inpaint": f.inpaint,
        "flux_gguf": f.gguf_node,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::params::Preprocessor;

    /// システムプロンプトの上限の目安(文字数)。トークン消費を抑えるためのテストで使う。
    pub const MAX_PROMPT_CHARS: usize = 6000;

    /// 「できること」の各機能の名前(テストで、カタログに載っているかの確認に使う)。
    pub const FEATURE_NAMES: [&str; 9] = [
        "画像生成",
        "顔補正",
        "ControlNet",
        "Inpaint",
        "プリセット",
        "ギャラリー",
        "キュー",
        "モデル",
        "AI に相談",
    ];

    /// 「まだ無い機能」を表すキーワード。「できること」の節に載せない(テストで確認)。
    pub const NOT_SUPPORTED: [&str; 4] = ["LoRA", "プレビュー", "アウトペイント", "動画生成"];

    #[test]
    fn catalog_mentions_every_feature() {
        for name in FEATURE_NAMES {
            assert!(CAPABILITIES.contains(name), "{name} がカタログにない");
        }
    }

    #[test]
    fn unsupported_features_are_listed_and_not_claimed() {
        let (supported, unsupported) = CAPABILITIES
            .split_once("# gazoo にまだ無い機能")
            .expect("セクションが分かれている");
        for name in NOT_SUPPORTED {
            assert!(!supported.contains(name), "{name} が「できること」にある");
            assert!(unsupported.contains(name), "{name} が「無い機能」にない");
        }
        assert!(unsupported.contains("複数の ControlNet"));
    }

    #[test]
    fn ui_names_match_the_app_strings() {
        // 画面のボタン名(ja.ts)と、カタログの引用が食い違わないこと
        let ja = include_str!("../../../../src/lib/i18n/ja.ts");
        for label in [
            "マスクを描く",
            "Inpaint に使う",
            "参照画像に使う",
            "この設定で生成",
            "結果を元画像へ合成し直す",
            "反転",
        ] {
            assert!(CAPABILITIES.contains(label), "{label} がカタログにない");
            assert!(ja.contains(label), "{label} が ja.ts にない");
        }
    }

    #[test]
    fn system_prompt_stays_within_the_size_budget() {
        let n = system_prompt().chars().count();
        assert!(n <= MAX_PROMPT_CHARS, "{n} 文字");
        assert!(system_prompt().contains("suggest_params"));
    }

    #[test]
    fn capabilities_reflect_engine_features() {
        assert!(capabilities_json(None).is_none());
        let f = EngineFeatures {
            gguf_node: true,
            face_detailer: false,
            controlnet_preprocessors: vec![Preprocessor::Canny],
            inpaint: true,
        };
        let v = capabilities_json(Some(&f)).unwrap();
        assert_eq!(v["faceFix"], false);
        assert_eq!(v["controlnet"], true);
        assert_eq!(v["controlnetPreprocessors"], json!(["canny"]));
        assert_eq!(v["inpaint"], true);
    }
}
