//! gazooo の機能カタログ(docs/13 §4)。AI サポートのシステムプロンプトに連結して渡す。
//!
//! 機能を追加・変更したら、**このファイルを更新する**(docs/09 のチェックリストにも項目がある)。
//! 画面名・ボタン名は UI の文言(`src/lib/i18n/ja.ts`)と一致させる。
//! アプリ固定の信頼できる文面であり、ユーザー入力は含めない。

use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::engine::EngineFeatures;
use crate::services::models::catalog::{
    Catalog, Family, FamilyKind, NsfwSupport, PromptKind, PromptLanguage, Speed, Style,
};
use crate::services::models::download::DownloadPolicy;

pub const CAPABILITIES: &str = r#"
# gazooo でできること(画面名・ボタン名はアプリの表示どおり)
- 画像生成(生成画面): プロンプト・ネガティブ・モデル種別(SD 1.5 / SDXL / Flux / Flux GGUF / Anima / Qwen-Image 2.1 / Krea 2)・サイズ・steps・CFG(Flux は guidance)・サンプラー・シード・枚数を指定して生成する。「生成」でキューに入る。
- 顔補正(生成画面のスイッチ。SD 1.5 / SDXL のみ): 生成画像の顔を検出し、その部分だけを再生成して整える。強さ(denoise)で変化の大きさを調整する。ComfyUI に Impact Pack / Subpack と検出モデルが必要。
- ControlNet(生成画面のスイッチ。SD 1.5 / SDXL のみ、1 つまで): 参照画像から輪郭(Canny)・深度・ポーズを取り出し、構図の条件にする。ベースモデルに合う ControlNet モデルが必要。
- Inpaint(生成画面のスイッチ。SD 1.5 / SDXL のみ): 参照画像に「マスクを描く」で範囲を塗り、塗った範囲だけを描き直す。マスク外は「結果を元画像へ合成し直す」で元画像と一致させられる。顔を保って服・背景だけ変えたいときは、顔を塗って「反転」する。強さは 0.6〜0.8 が目安。プロンプトには描き直す部分の説明だけを書く。顔補正とは同時に使えない。参照画像は、ファイル選択か、ギャラリー詳細の「Inpaint に使う」で設定する。
- 画像から生成(生成画面のスイッチ「画像から生成(img2img)」。SD 1.5 / SDXL のみ): 元画像を出発点に、強さ(denoise)の分だけ描き直す。写真やイラストをアニメ調にしたいときは、アニメ系の SDXL モデル(Animagine XL 4.0・NoobAI-XL など)を選び、強さは弱 0.45・中 0.6(既定)・強 0.75 が目安。「構図を固定する」を入れると輪郭(Canny)の ControlNet が自動で設定され、形を保ったまま画風だけ変わる。「画像からアニメ調に」ボタンで、アニメ系モデルと推奨値を一度に設定できる。プロンプトには写っているものをタグで書く(元画像からの自動入力はしない)。Inpaint とは同時に使えない。顔補正・ControlNet とは併用できる。元画像は、ファイル選択か、ギャラリー詳細の「img2img に使う」で設定する。
- 参照画像つき生成(生成画面のスイッチ「参照画像」。Qwen-Image 2.1 のみ): 参照画像を 1〜4 枚与え、プロンプトで「どうするか」を指示して編集・合成する。参照画像はプロンプトの中で <image1> <image2> のように番号で指す(例: 「<image1> の人物とポーズはそのままで、<image2> の服を着せる」)。1 枚目が編集される画像で、出力サイズも 1 枚目で決まる。「保つもの」と「変えるもの」を言葉で書くのがコツ。CFG は 1 のまま(ネガティブは効かない)。参照画像の大きさは小(768)・標準(1024、既定)・大(1536)で、大きいほどメモリと時間がかかる。「指示でアニメ調に」ボタンで、アニメ調に描き直す指示の雛形が入る(画風を別の画像で指定するより、言葉で指示するほうが安定する)。Inpaint・img2img・ControlNet・顔補正とは同時に使えない。参照画像は、ファイル選択か、ギャラリー詳細の「Qwen の参照に使う」で追加する。
- プリセット(プリセット画面と生成画面のプリセット欄): 生成設定の保存と呼び出し。組み込みの「アニメ(SDXL・縦長)」「アニメ(SDXL・正方形)」(アニメ系 SDXL 向けのサイズ・steps・CFG・サンプラー・ネガティブ。プロンプトとモデルの選択は変えない)がある。
- ギャラリー: 生成画像の一覧・検索・お気に入り・書き出し。詳細の「この設定で生成」で同じ設定を再現し、「参照画像に使う」(ControlNet)や「Inpaint に使う」で画像を参照に設定できる。
- キュー: 実行中・待機中のジョブの確認、並べ替え、キャンセル、再試行。
- モデル(モデル画面): チェックポイント・ControlNet・検出モデル・Flux・Anima・Qwen-Image 2.1・Krea 2 を、バージョンを選んで取得できる(Anima などは「必要なファイルをまとめて取得」で、本体・テキストエンコーダ・VAE が揃う)。取得はユーザーが「ダウンロード」を押したときだけ行う。チェックポイントには、アニメ・イラスト特化の SDXL の NoobAI-XL と Animagine XL 4.0(どちらも成人向けにも対応。「用途」で絞り込める)がある。
- アニメ系モデルの使い方: NoobAI-XL・Animagine XL 4.0 は自然な文章ではなく Danbooru タグ(1girl, solo, …)で書く。品質タグ(例: masterpiece, best quality)を入れ、ネガティブに lowres, bad anatomy, worst quality などを入れる。サイズは 832×1216 か 1024×1024、steps 28 前後、CFG 5 前後、サンプラーは euler_ancestral が目安。成人向けの表現はタグ(Animagine は rating: safe / sensitive / nsfw / explicit)で制御し、避けたいときはネガティブに nsfw を入れる。顔補正・ControlNet・Inpaint はそのまま使える。
- AI に相談(このパネル): フォームの設定へのアドバイス。提案はユーザーが「適用」を押して初めてフォームに反映される。送信先は設定の「AI サポート」で選ぶ: Claude(Anthropic の API。内容は外部へ送られ、利用料がかかる)、ローカル LLM(Qwen3.5-9B。この PC の中だけで処理され、外部へは送られず、料金もかからない)、ローカル LLM の外部サーバ(Ollama など)。
- ローカル LLM(設定の「AI サポート」で、モデルとランタイムをアプリが導入・起動・停止する): 使わない状態が続くと自動で停止してメモリ(RAM / VRAM)を解放し、次の相談で自動的に起動し直す(数秒〜十数秒かかる)。画像生成の開始前にも自動で解放できる。メモリ不足の相談には、「設定 > AI サポートの [メモリを解放]」や自動解放の設定も案内する(外部サーバのメモリはアプリでは管理できない)。

# gazooo にまだ無い機能(「ある」と案内しない。必要なら ComfyUI を直接使う方法を勧める)
- LoRA、生成中のプレビュー画像、元画像からのプロンプト自動生成、参照画像への色マークによる領域指定、アウトペイント(キャンバスの拡張)、複数の ControlNet、動画生成。
- Flux 系・Anima・Qwen-Image 2.1・Krea 2 では画像から生成・Inpaint・顔補正・ControlNet を使えない(SD 1.5 / SDXL のみ)。参照画像つき生成は Qwen-Image 2.1 のみ。
- あなた(AI)はアプリを操作できない。シード・ControlNet・img2img・Inpaint・参照画像の画像やマスクは、提案ではなく手順を本文で説明する。モデルの切替は suggest_model で提案できる(切り替えるのはユーザーが「このモデルに切り替える」を押したとき)。
- 「現在のフォーム」の capabilities が false の機能は、今の ComfyUI では使えない。足りないものを案内する(例: Inpaint が false なら ComfyUI の更新)。"#;

/// 同梱のモデルカタログ(読み込みに失敗したら `None`。その場合モデルガイドは付けない)。
pub fn catalog() -> Option<&'static Catalog> {
    static CATALOG: OnceLock<Option<Catalog>> = OnceLock::new();
    CATALOG
        .get_or_init(|| Catalog::embedded(&DownloadPolicy::production()).ok())
        .as_ref()
}

const MODEL_RULES: &str = r#"
# モデルの提案のルール
- 依頼の目的(絵柄・人数・文字・成人向け・速度・使えるメモリ)に合うモデルを、1〜2 件、理由つきで挙げる。
- 「選択中のモデルと導入済みのモデル」にある導入済みのモデルを優先する。未導入のモデルを勧めたら、モデル画面で取得できると案内する。
- メモリが足りなそうなときは、軽いモデルも添える。
- 下のモデルガイドに無いモデルを、推測で勧めない。選択中のモデルがガイドに無い(カタログ外)ときは、特徴が分からないと答える。
- プロンプトの作成依頼では、下の「プロンプトの書き方」に従う(docs/11 §14)。suggest_params の prompt / negativePrompt は、ユーザーの言語ではなく、モデルガイドの「プロンプト」欄の言語で書く。「英語で書く」のモデルには、日本語で依頼されても英語で書き、本文に日本語で要点を一言添える。同じ回答で suggest_model を使うときは、勧めたモデルに合わせる。フォームのプロンプトが推奨言語と違うときは、意味を変えずに書き直しを提案する。ネガティブも同じ言語にする。
- 成人向けの内容は、ガイドで「成人向け対応」とあるモデルだけを勧める。利用者が適法な範囲で使うことが前提で、未成年・実在の人物に関わる依頼には協力しない。
- 参照画像(img2img・ControlNet・Inpaint・参照画像つき生成)を使う成人向けの依頼は、参照画像が、成人の架空キャラクター(イラストや CG)であるとユーザーが説明したときに限って手伝う。画像は見えないので、実在の人物の写真や、未成年に見えるキャラクターの画像だと分かる(または疑われる)ときは、協力しない。
"#;

/// プロンプトの書き方(docs/11 §14.2)。AI サポートのシステムプロンプトと、プロンプト翻訳(docs/34)で
/// **同じ文面を共有する**(手書きで二重に持たない)。
pub const PROMPT_BEST_PRACTICE: &str = r#"
# プロンプトの書き方
- 言語: 「英語で書く」のモデル(SD 1.5 / SDXL / NoobAI-XL / Animagine XL / Anima / Flux 系 / Krea 2)は英語で書く。「日本語でも可」のモデル(Qwen-Image 2.1)は、ユーザーの言語のまま書いてよい。
- SD 1.5 / SDXL: 短いキーワードをカンマ区切り(被写体、構図・ポーズ、場所、光、画風・画質の順。重要なものを前に)。
- NoobAI-XL・Animagine XL・Anima: 自然文ではなくタグ形式(1girl, solo、キャラ、作品、外見・服装・ポーズ、場所、品質タグの順。rating タグを指定)。
- Flux 系・Krea 2・Qwen-Image 2.1: 文章で具体的に(被写体・構図・光・画風)。Flux 系は重み付けとネガティブを使わない。Qwen-Image 2.1 は画像内に描く文字を引用符で囲む。
- 矛盾する要素(昼と夜、写実とアニメ)を混ぜない。
"#;

/// モデルガイド(docs/18 §6.1)。カタログのプロフィールから自動生成する(手書きで二重に持たない)。
/// 1 ファミリー 1 項目。`[ID]` は `suggest_model` の familyId に渡すもの。
pub fn model_guide(catalog: &Catalog) -> String {
    let mut out =
        String::from("\n# モデルガイド(`[ID]` を suggest_model の familyId に指定する)\n");
    for f in catalog.families.iter().filter(|f| f.is_selectable_model()) {
        if let Some(line) = guide_line(f) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn guide_line(f: &Family) -> Option<String> {
    let p = f.profile.as_ref()?;
    let styles: Vec<&str> = p
        .style
        .iter()
        .map(|s| match s {
            Style::Anime => "アニメ",
            Style::Realistic => "実写",
            Style::Illustration => "イラスト",
            Style::General => "汎用",
        })
        .collect();
    let prompt = match p.prompt_style.kind {
        PromptKind::Tags => "タグ形式",
        PromptKind::Natural => "文章",
        PromptKind::Both => "タグでも文章でも可",
    };
    // プロンプトを書く言語(docs/11 §14.1)
    let prompt = match p.prompt_style.language {
        PromptLanguage::En => format!("{prompt}・英語で書く"),
        PromptLanguage::Multilingual => format!("{prompt}・日本語でも可"),
    };
    let speed = match p.resources.speed {
        Speed::Fast => "速い",
        Speed::Medium => "普通",
        Speed::Slow => "遅い",
    };
    // 対応機能は種別から決まる(SD 1.5 / SDXL 以外は txt2img のみ。docs/17 §3、18 §3)
    let features = match f.kind {
        Some(FamilyKind::FluxGguf) => {
            "Flux 系(ネガティブ・顔補正・ControlNet・Inpaint・画像から生成は使えない)"
        }
        Some(FamilyKind::Anima | FamilyKind::QwenImage) => {
            "ネガティブは使える。顔補正・ControlNet・Inpaint・画像から生成は使えない"
        }
        Some(FamilyKind::Krea2) => {
            "ネガティブ・顔補正・ControlNet・Inpaint・画像から生成は使えない"
        }
        None => "顔補正・ControlNet・Inpaint・画像から生成・ネガティブが使える",
    };
    let nsfw = match p.nsfw {
        NsfwSupport::Capable | NsfwSupport::Limited => "。成人向け対応",
        NsfwSupport::NotIntended | NsfwSupport::Unknown => "",
    };
    Some(format!(
        "- [{}] {}: {}。絵柄: {}。得意: {}。不得意: {}。プロンプト: {}。目安: VRAM {}GB 以上・{}{}。{}",
        f.id,
        f.name.ja,
        p.summary.ja,
        styles.join("・"),
        p.best_for.ja.join(" / "),
        p.weak_at.ja.join(" / "),
        prompt,
        p.resources.vram_gb,
        speed,
        nsfw,
        features,
    ))
}

/// システムプロンプトの初期値(編集できる部分。基本の指示 + 機能カタログ + モデルの提案のルール。docs/11 §12.1)。
pub fn default_prompt() -> String {
    let mut s = format!(
        "{}
{}",
        super::context::SYSTEM_PROMPT,
        CAPABILITIES
    );
    if catalog().is_some() {
        s.push_str(MODEL_RULES);
    }
    s.push_str(PROMPT_BEST_PRACTICE);
    s
}

/// 自動生成のモデルガイド(編集不可。常にシステムプロンプトの末尾に付く)。カタログが読めなければ空。
pub fn model_guide_text() -> String {
    catalog().map(model_guide).unwrap_or_default()
}

/// 送信するシステムプロンプト全体。`custom` が `None` なら初期値。モデルガイドは常に末尾に付ける。
pub fn system_prompt(custom: Option<&str>) -> String {
    let mut s = custom.map_or_else(default_prompt, str::to_owned);
    s.push_str(&model_guide_text());
    s
}

/// 今の ComfyUI で使える機能(「現在のフォーム」に添える)。接続状態が不明なら `None`。
pub fn capabilities_json(features: Option<&EngineFeatures>) -> Option<Value> {
    let f = features?;
    Some(json!({
        "faceFix": f.face_detailer,
        "controlnet": !f.controlnet_preprocessors.is_empty(),
        "controlnetPreprocessors": f.controlnet_preprocessors,
        "inpaint": f.inpaint,
        "img2img": f.img2img,
        "referenceEdit": f.reference_edit,
        "flux_gguf": f.gguf_node,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::params::Preprocessor;

    /// システムプロンプトの上限の目安(文字数)。トークン消費を抑えるためのテストで使う。
    pub const MAX_PROMPT_CHARS: usize = 9000;

    /// 「できること」の各機能の名前(テストで、カタログに載っているかの確認に使う)。
    pub const FEATURE_NAMES: [&str; 10] = [
        "画像生成",
        "顔補正",
        "ControlNet",
        "Inpaint",
        "画像から生成",
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
            .split_once("# gazooo にまだ無い機能")
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
            "img2img に使う",
            "画像からアニメ調に",
            "構図を固定する",
        ] {
            assert!(CAPABILITIES.contains(label), "{label} がカタログにない");
            assert!(ja.contains(label), "{label} が ja.ts にない");
        }
    }

    #[test]
    fn model_guide_lists_every_selectable_family_with_its_id() {
        // docs/18 §6.1: 選べるモデルすべてが ID つきで載る(suggest_model の familyId の元)
        let c = catalog().expect("同梱カタログを読める");
        let guide = model_guide(c);
        let selectable: Vec<_> = c
            .families
            .iter()
            .filter(|f| f.is_selectable_model())
            .collect();
        assert!(selectable.len() >= 6);
        for f in &selectable {
            assert!(
                guide.contains(&format!("[{}]", f.id)),
                "{} がガイドにない",
                f.id
            );
        }
        // ControlNet・検出モデル・Flux のテキストエンコーダ / VAE は載らない
        assert!(!guide.contains("[controlnet-canny]"));
        assert!(!guide.contains("[flux-vae]"));
        // 説明の内容(書き方・メモリ)が入る
        assert!(guide.contains("Danbooru"), "NoobAI-XL の説明");
        assert!(guide.contains("VRAM"));
    }

    #[test]
    fn nsfw_is_only_claimed_for_models_marked_capable() {
        let c = catalog().unwrap();
        let guide = model_guide(c);
        for line in guide.lines().filter(|l| l.starts_with("- [")) {
            let id = line.trim_start_matches("- [").split(']').next().unwrap();
            let fam = c.families.iter().find(|f| f.id == id).unwrap();
            let capable = fam.profile.as_ref().unwrap().nsfw.is_nsfw();
            assert_eq!(line.contains("成人向け対応"), capable, "{id}");
        }
        assert!(guide.contains("[checkpoint-noobai-xl]"));
    }

    #[test]
    fn system_prompt_includes_rules_and_guide() {
        let p = system_prompt(None);
        assert!(p.contains("suggest_model"));
        assert!(p.contains("# モデルの提案のルール"));
        assert!(p.contains("# モデルガイド"));
        assert!(p.contains("[checkpoint-sdxl]"));
        // 「アプリを操作できない」の項で、モデルの切替は提案できることになった
        assert!(!CAPABILITIES.contains("モデルの切替・シード"));
    }

    #[test]
    fn guide_states_the_prompt_language_per_model() {
        // docs/11 §14: 英語推奨のモデルは「英語で書く」、Qwen-Image 2.1 だけ「日本語でも可」
        let guide = model_guide(catalog().unwrap());
        let line = |id: &str| {
            guide
                .lines()
                .find(|l| l.starts_with(&format!("- [{id}]")))
                .unwrap_or_else(|| panic!("{id} がガイドにない"))
                .to_owned()
        };
        assert!(line("checkpoint-noobai-xl").contains("英語で書く"));
        assert!(line("checkpoint-sdxl").contains("英語で書く"));
        assert!(line("qwen-image-2-1").contains("日本語でも可"));
        assert!(default_prompt().contains("日本語で要点を一言添える"));
    }

    #[test]
    fn anime_models_and_builtin_presets_are_described() {
        // docs/16 §6: アニメ系モデルと組み込みプリセットを案内できる。名前は UI・カタログと一致させる
        let presets = include_str!("../../../../src/lib/generation/builtin-presets.ts");
        let catalog = include_str!("../../../resources/models/catalog.json");
        for name in [
            "NoobAI-XL",
            "Animagine XL 4.0",
            "Danbooru",
            "アニメ(SDXL・縦長)",
            "アニメ(SDXL・正方形)",
        ] {
            assert!(CAPABILITIES.contains(name), "{name} がカタログにない");
        }
        for name in ["アニメ(SDXL・縦長)", "アニメ(SDXL・正方形)"] {
            assert!(presets.contains(name), "{name} が組み込みプリセットにない");
        }
        for name in ["NoobAI-XL", "Animagine XL 4.0"] {
            assert!(catalog.contains(name), "{name} がモデルカタログにない");
        }
    }

    #[test]
    fn custom_prompt_replaces_the_editable_part_and_keeps_the_guide() {
        let p = system_prompt(Some("カスタム指示"));
        assert!(p.starts_with("カスタム指示"));
        assert!(!p.contains("# 守ること"));
        assert!(p.contains("# モデルガイド"));
        assert!(system_prompt(None).starts_with(&default_prompt()));
    }

    #[test]
    fn system_prompt_stays_within_the_size_budget() {
        let n = default_prompt().chars().count();
        assert!(n <= MAX_PROMPT_CHARS, "{n} 文字");
        assert!(default_prompt().contains("suggest_params"));
    }

    #[test]
    fn capabilities_reflect_engine_features() {
        assert!(capabilities_json(None).is_none());
        let f = EngineFeatures {
            gguf_node: true,
            face_detailer: false,
            controlnet_preprocessors: vec![Preprocessor::Canny],
            inpaint: true,
            img2img: true,
            reference_edit: true,
        };
        let v = capabilities_json(Some(&f)).unwrap();
        assert_eq!(v["faceFix"], false);
        assert_eq!(v["controlnet"], true);
        assert_eq!(v["controlnetPreprocessors"], json!(["canny"]));
        assert_eq!(v["inpaint"], true);
    }
}
