//! 生成パラメータから ComfyUI の API 形式ワークフロー(プロンプト JSON)を組み立てる。
//! docs/02-architecture.md §3.4。

use serde_json::{Value, json};

use crate::domain::params::{
    ControlnetUnit, FaceDetailerParams, GenerationParams, ModelKind, Preprocessor,
};

/// 顔補正(`FaceDetailer`)のノード ID。進捗の段階判定(generate.rs)でも使う。
pub const FACE_DETAILER_NODE: &str = "22";
/// 顔検出モデルのローダー。
const FACE_DETECTOR_NODE: &str = "20";
/// 補正用プロンプトのエンコード(指定時のみ)。
const FACE_PROMPT_NODE: &str = "21";

/// ノード出力へのリンク `[node_id, output_index]`。
fn link(node: &str, index: u32) -> Value {
    json!([node, index])
}

fn node(class_type: &str, inputs: Value) -> Value {
    json!({ "class_type": class_type, "inputs": inputs })
}

/// `control_images` は有効な ControlNet ユニットの順に並べた、エンジンへアップロード済みの画像名。
pub fn build(params: &GenerationParams, filename_prefix: &str, control_images: &[String]) -> Value {
    if params.model.kind.is_flux() {
        build_flux(params, filename_prefix)
    } else {
        build_sd(params, filename_prefix, control_images)
    }
}

/// ユニット `i` のノード ID の起点。LoadImage、ImageScale、前処理、ControlNetLoader、
/// ControlNetApplyAdvanced の順に連番を振る(顔補正の 20〜22 と重ならない)。
fn controlnet_base(i: usize) -> usize {
    30 + 10 * i
}

/// SD 1.5 / SDXL(チェックポイント一体型)。
fn build_sd(p: &GenerationParams, prefix: &str, control_images: &[String]) -> Value {
    // VAE を別指定した場合のみ VAELoader を使う
    let (vae_link, extra_vae) = match &p.model.vae {
        Some(vae) if !vae.trim().is_empty() => (
            link("7", 0),
            Some(node("VAELoader", json!({ "vae_name": vae }))),
        ),
        _ => (link("1", 2), None),
    };

    let mut wf = json!({
        "1": node("CheckpointLoaderSimple", json!({ "ckpt_name": p.model.name })),
        "2": node("CLIPTextEncode", json!({ "text": p.prompt, "clip": link("1", 1) })),
        "3": node("CLIPTextEncode", json!({ "text": p.negative_prompt, "clip": link("1", 1) })),
        "4": node("EmptyLatentImage", json!({
            "width": p.width, "height": p.height, "batch_size": p.batch_size
        })),
        "5": node("KSampler", json!({
            "model": link("1", 0),
            "seed": p.seed,
            "steps": p.steps,
            "cfg": p.cfg,
            "sampler_name": p.sampler,
            "scheduler": p.scheduler,
            "positive": link("2", 0),
            "negative": link("3", 0),
            "latent_image": link("4", 0),
            "denoise": 1.0
        })),
        "6": node("VAEDecode", json!({ "samples": link("5", 0), "vae": vae_link })),
        "8": node("SaveImage", json!({ "images": link("6", 0), "filename_prefix": prefix })),
    });
    if let Some(vae_node) = extra_vae {
        wf["7"] = vae_node;
    }
    // ControlNet は本体のサンプリングにだけ効かせる(顔補正は元の conditioning を使う)
    add_controlnet(&mut wf, p, control_images);
    if let Some(fd) = p.active_face_detailer() {
        add_face_detailer(&mut wf, p, fd, &vae_link);
    }
    wf
}

/// 参照画像を生成サイズへ中央クロップでリサイズし、前処理を通して ControlNet を適用する。
/// KSampler の positive / negative を、適用後の conditioning に付け替える。
fn add_controlnet(wf: &mut Value, p: &GenerationParams, images: &[String]) {
    let mut positive = link("2", 0);
    let mut negative = link("3", 0);
    for (i, unit) in p.active_controlnet_units().iter().enumerate() {
        let base = controlnet_base(i);
        let id = |k: usize| (base + k).to_string();
        wf[id(0)] = node(
            "LoadImage",
            json!({ "image": images.get(i).cloned().unwrap_or_default() }),
        );
        wf[id(1)] = node(
            "ImageScale",
            json!({
                "image": link(&id(0), 0),
                "upscale_method": "lanczos",
                "width": p.width,
                "height": p.height,
                "crop": "center",
            }),
        );
        // 前処理なしなら、リサイズした参照画像をそのまま条件にする
        let hint = match preprocessor_node(unit, p, link(&id(1), 0)) {
            Some(pre) => {
                wf[id(2)] = pre;
                link(&id(2), 0)
            }
            None => link(&id(1), 0),
        };
        wf[id(3)] = node(
            "ControlNetLoader",
            json!({ "control_net_name": unit.model }),
        );
        wf[id(4)] = node(
            "ControlNetApplyAdvanced",
            json!({
                "positive": positive,
                "negative": negative,
                "control_net": link(&id(3), 0),
                "image": hint,
                "strength": unit.strength,
                "start_percent": unit.start_percent,
                "end_percent": unit.end_percent,
            }),
        );
        positive = link(&id(4), 0);
        negative = link(&id(4), 1);
    }
    wf["5"]["inputs"]["positive"] = positive;
    wf["5"]["inputs"]["negative"] = negative;
}

/// 前処理ノード(Canny はコア、Depth / OpenPose は comfyui_controlnet_aux)。
fn preprocessor_node(unit: &ControlnetUnit, p: &GenerationParams, image: Value) -> Option<Value> {
    // 前処理の解像度は短辺に合わせる(64 の倍数。生成サイズが 64 の倍数のため)
    let resolution = p.width.min(p.height);
    match unit.preprocessor {
        Preprocessor::Canny => Some(node(
            "Canny",
            json!({ "image": image, "low_threshold": 0.4, "high_threshold": 0.8 }),
        )),
        Preprocessor::Depth => Some(node(
            "DepthAnythingV2Preprocessor",
            json!({
                "image": image,
                "ckpt_name": "depth_anything_v2_vitl.pth",
                "resolution": resolution,
            }),
        )),
        Preprocessor::Openpose => Some(node(
            "OpenposePreprocessor",
            json!({
                "image": image,
                "detect_hand": "enable",
                "detect_body": "enable",
                "detect_face": "enable",
                "resolution": resolution,
                "scale_stick_for_xinsr_cn": "disable",
            }),
        )),
        Preprocessor::None => None,
    }
}

/// VAEDecode の後ろに顔補正を差し込み、SaveImage の入力を補正後の画像に付け替える。
/// 検出されなければ `FaceDetailer` は元の画像をそのまま返す。
/// ステップ・CFG・サンプラー・シードは本体の値を引き継ぐ。ほかの引数は固定値(docs/10 §4.2)。
fn add_face_detailer(wf: &mut Value, p: &GenerationParams, fd: &FaceDetailerParams, vae: &Value) {
    // SDXL は 1024px 級で学習されているため、切り出し領域の目標サイズを大きくする
    let (guide_size, max_size) = if p.model.kind == ModelKind::Sdxl {
        (1024, 1536)
    } else {
        (512, 1024)
    };
    let positive = if fd.prompt.trim().is_empty() {
        link("2", 0)
    } else {
        wf[FACE_PROMPT_NODE] = node(
            "CLIPTextEncode",
            json!({ "text": fd.prompt, "clip": link("1", 1) }),
        );
        link(FACE_PROMPT_NODE, 0)
    };
    wf[FACE_DETECTOR_NODE] = node(
        "UltralyticsDetectorProvider",
        json!({ "model_name": fd.detector }),
    );
    wf[FACE_DETAILER_NODE] = node(
        "FaceDetailer",
        json!({
            "image": link("6", 0),
            "model": link("1", 0),
            "clip": link("1", 1),
            "vae": vae,
            "guide_size": guide_size,
            "guide_size_for": true,
            "max_size": max_size,
            "seed": p.seed,
            "steps": p.steps,
            "cfg": p.cfg,
            "sampler_name": p.sampler,
            "scheduler": p.scheduler,
            "positive": positive,
            "negative": link("3", 0),
            "denoise": fd.denoise,
            "feather": 5,
            "noise_mask": true,
            "force_inpaint": true,
            "bbox_threshold": fd.confidence,
            "bbox_dilation": 10,
            "bbox_crop_factor": 3.0,
            // SAM は使わない(下の 4 項目は必須入力のため既定値を渡す)
            "sam_detection_hint": "center-1",
            "sam_dilation": 0,
            "sam_threshold": 0.93,
            "sam_bbox_expansion": 0,
            "sam_mask_hint_threshold": 0.7,
            "sam_mask_hint_use_negative": "False",
            "drop_size": 10,
            "bbox_detector": link(FACE_DETECTOR_NODE, 0),
            "wildcard": "",
            "cycle": 1,
        }),
    );
    wf["8"]["inputs"]["images"] = link(FACE_DETAILER_NODE, 0);
}

/// Flux.1(通常 / GGUF)。ネガティブプロンプトは使わない(cfg = 1.0 固定、FluxGuidance で誘導)。
fn build_flux(p: &GenerationParams, prefix: &str) -> Value {
    let is_gguf = p.model.kind == ModelKind::FluxGguf;
    let clip1 = p.model.clip.first().cloned().unwrap_or_default(); // T5
    let clip2 = p.model.clip.get(1).cloned().unwrap_or_default(); // CLIP-L
    let gguf_clip = clip1.ends_with(".gguf") || clip2.ends_with(".gguf");

    let unet = if is_gguf {
        node("UnetLoaderGGUF", json!({ "unet_name": p.model.name }))
    } else {
        node(
            "UNETLoader",
            json!({ "unet_name": p.model.name, "weight_dtype": "default" }),
        )
    };
    let clip_inputs = json!({ "clip_name1": clip1, "clip_name2": clip2, "type": "flux" });
    let clip_loader = if gguf_clip {
        node("DualCLIPLoaderGGUF", clip_inputs)
    } else {
        node("DualCLIPLoader", clip_inputs)
    };

    json!({
        "1": unet,
        "2": clip_loader,
        "3": node("VAELoader", json!({ "vae_name": p.model.vae.clone().unwrap_or_default() })),
        "4": node("CLIPTextEncode", json!({ "text": p.prompt, "clip": link("2", 0) })),
        "5": node("FluxGuidance", json!({ "conditioning": link("4", 0), "guidance": p.guidance })),
        "6": node("ConditioningZeroOut", json!({ "conditioning": link("4", 0) })),
        "7": node("EmptySD3LatentImage", json!({
            "width": p.width, "height": p.height, "batch_size": p.batch_size
        })),
        "8": node("KSampler", json!({
            "model": link("1", 0),
            "seed": p.seed,
            "steps": p.steps,
            "cfg": 1.0,
            "sampler_name": p.sampler,
            "scheduler": p.scheduler,
            "positive": link("5", 0),
            "negative": link("6", 0),
            "latent_image": link("7", 0),
            "denoise": 1.0
        })),
        "9": node("VAEDecode", json!({ "samples": link("8", 0), "vae": link("3", 0) })),
        "10": node("SaveImage", json!({ "images": link("9", 0), "filename_prefix": prefix })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::params::{ControlnetParams, sample};

    const ALL: [ModelKind; 4] = [
        ModelKind::Sd15,
        ModelKind::Sdxl,
        ModelKind::Flux,
        ModelKind::FluxGguf,
    ];

    fn class_of<'a>(wf: &'a Value, id: &str) -> &'a str {
        wf[id]["class_type"].as_str().unwrap()
    }

    fn classes(wf: &Value) -> Vec<&str> {
        wf.as_object()
            .unwrap()
            .values()
            .map(|n| n["class_type"].as_str().unwrap())
            .collect()
    }

    /// すべてのリンクが存在するノードを指し、出力 SaveImage が 1 つあること。
    fn assert_well_formed(wf: &Value) {
        let nodes = wf.as_object().unwrap();
        for (id, n) in nodes {
            assert!(n["class_type"].is_string(), "node {id} has no class_type");
            for (name, input) in n["inputs"].as_object().unwrap() {
                if let Some(arr) = input.as_array() {
                    let target = arr[0]
                        .as_str()
                        .unwrap_or_else(|| panic!("{id}.{name}: bad link"));
                    assert!(
                        nodes.contains_key(target),
                        "{id}.{name} -> missing node {target}"
                    );
                    assert!(arr[1].is_u64(), "{id}.{name}: output index");
                    assert_ne!(target, id, "{id}.{name}: self link");
                }
            }
        }
        assert_eq!(classes(wf).iter().filter(|c| **c == "SaveImage").count(), 1);
    }

    #[test]
    fn all_kinds_are_well_formed() {
        for kind in ALL {
            let wf = build(&sample(kind), "gazoo", &[]);
            assert_well_formed(&wf);
        }
    }

    #[test]
    fn sd_uses_checkpoint_and_both_prompts() {
        let wf = build(&sample(ModelKind::Sdxl), "gazoo", &[]);
        assert_eq!(class_of(&wf, "1"), "CheckpointLoaderSimple");
        assert_eq!(wf["1"]["inputs"]["ckpt_name"], "model.safetensors");
        assert_eq!(wf["2"]["inputs"]["text"], "a cat");
        assert_eq!(wf["3"]["inputs"]["text"], "blurry");
        assert_eq!(wf["5"]["inputs"]["seed"], 42);
        assert_eq!(wf["4"]["inputs"]["batch_size"], 1);
        // VAE 未指定ならチェックポイントの VAE(出力 2)を使う
        assert_eq!(wf["6"]["inputs"]["vae"], json!(["1", 2]));
        assert!(!classes(&wf).contains(&"VAELoader"));
    }

    #[test]
    fn sd_with_explicit_vae_uses_vae_loader() {
        let mut p = sample(ModelKind::Sd15);
        p.model.vae = Some("custom.safetensors".into());
        let wf = build(&p, "gazoo", &[]);
        assert_well_formed(&wf);
        assert_eq!(wf["6"]["inputs"]["vae"], json!(["7", 0]));
        assert_eq!(wf["7"]["inputs"]["vae_name"], "custom.safetensors");
    }

    fn face_params(kind: ModelKind, prompt: &str) -> GenerationParams {
        let mut p = sample(kind);
        p.face_detailer = Some(FaceDetailerParams {
            enabled: true,
            detector: "bbox/face_yolov8m.pt".into(),
            denoise: 0.35,
            prompt: prompt.into(),
            confidence: 0.6,
        });
        p
    }

    #[test]
    fn face_detailer_sits_between_decode_and_save() {
        for kind in [ModelKind::Sd15, ModelKind::Sdxl] {
            let wf = build(&face_params(kind, ""), "gazoo", &[]);
            assert_well_formed(&wf);
            assert_eq!(class_of(&wf, FACE_DETAILER_NODE), "FaceDetailer");
            assert_eq!(class_of(&wf, "20"), "UltralyticsDetectorProvider");
            assert_eq!(wf["20"]["inputs"]["model_name"], "bbox/face_yolov8m.pt");
            let fd = &wf[FACE_DETAILER_NODE]["inputs"];
            assert_eq!(fd["image"], json!(["6", 0]));
            assert_eq!(fd["bbox_detector"], json!(["20", 0]));
            assert_eq!(fd["denoise"], 0.35);
            assert_eq!(fd["bbox_threshold"], 0.6);
            // 本体のサンプリング設定を引き継ぐ
            assert_eq!(
                (&fd["seed"], &fd["steps"], &fd["sampler_name"]),
                (&json!(42), &json!(20), &json!("euler"))
            );
            assert_eq!(fd["negative"], json!(["3", 0]));
            // 保存されるのは補正後の画像
            assert_eq!(wf["8"]["inputs"]["images"], json!([FACE_DETAILER_NODE, 0]));
        }
    }

    #[test]
    fn face_detailer_prompt_override_adds_encoder() {
        let wf = build(&face_params(ModelKind::Sd15, "detailed eyes"), "gazoo", &[]);
        assert_well_formed(&wf);
        assert_eq!(wf["21"]["inputs"]["text"], "detailed eyes");
        assert_eq!(
            wf[FACE_DETAILER_NODE]["inputs"]["positive"],
            json!(["21", 0])
        );

        // 空(空白のみ)なら本体のプロンプトを使い、エンコーダは増やさない
        let wf = build(&face_params(ModelKind::Sd15, "  "), "gazoo", &[]);
        assert!(wf.get("21").is_none());
        assert_eq!(
            wf[FACE_DETAILER_NODE]["inputs"]["positive"],
            json!(["2", 0])
        );
    }

    #[test]
    fn face_detailer_uses_explicit_vae_and_sdxl_sizes() {
        let mut p = face_params(ModelKind::Sdxl, "");
        p.model.vae = Some("custom.safetensors".into());
        let wf = build(&p, "gazoo", &[]);
        assert_well_formed(&wf);
        assert_eq!(wf[FACE_DETAILER_NODE]["inputs"]["vae"], json!(["7", 0]));
        assert_eq!(wf[FACE_DETAILER_NODE]["inputs"]["guide_size"], 1024);
        let wf = build(&face_params(ModelKind::Sd15, ""), "gazoo", &[]);
        assert_eq!(wf[FACE_DETAILER_NODE]["inputs"]["guide_size"], 512);
        assert_eq!(wf[FACE_DETAILER_NODE]["inputs"]["vae"], json!(["1", 2]));
    }

    #[test]
    fn disabled_face_detailer_leaves_workflow_unchanged() {
        let plain = build(&sample(ModelKind::Sd15), "gazoo", &[]);
        let mut p = face_params(ModelKind::Sd15, "x");
        p.face_detailer.as_mut().unwrap().enabled = false;
        assert_eq!(build(&p, "gazoo", &[]), plain);
        assert!(!classes(&plain).contains(&"FaceDetailer"));
    }

    fn cn_params(kind: ModelKind, pre: Preprocessor) -> GenerationParams {
        let mut p = sample(kind);
        p.width = 768;
        p.height = 512;
        p.controlnet = Some(ControlnetParams {
            enabled: true,
            units: vec![ControlnetUnit {
                image: "inputs/a.png".into(),
                model: "control_canny.safetensors".into(),
                preprocessor: pre,
                strength: 0.8,
                start_percent: 0.1,
                end_percent: 0.9,
            }],
        });
        p
    }

    const IMG: [&str; 1] = ["gazoo-1.png"];

    #[test]
    fn controlnet_conditions_the_sampler_with_scaled_preprocessed_image() {
        for kind in [ModelKind::Sd15, ModelKind::Sdxl] {
            let wf = build(
                &cn_params(kind, Preprocessor::Canny),
                "gazoo",
                &IMG.map(String::from),
            );
            assert_well_formed(&wf);
            assert_eq!(class_of(&wf, "30"), "LoadImage");
            assert_eq!(wf["30"]["inputs"]["image"], "gazoo-1.png");
            // 生成サイズへ中央クロップ
            assert_eq!(class_of(&wf, "31"), "ImageScale");
            assert_eq!(wf["31"]["inputs"]["width"], 768);
            assert_eq!(wf["31"]["inputs"]["height"], 512);
            assert_eq!(wf["31"]["inputs"]["crop"], "center");
            assert_eq!(class_of(&wf, "32"), "Canny");
            assert_eq!(wf["32"]["inputs"]["image"], json!(["31", 0]));
            assert_eq!(class_of(&wf, "33"), "ControlNetLoader");
            assert_eq!(
                wf["33"]["inputs"]["control_net_name"],
                "control_canny.safetensors"
            );
            let apply = &wf["34"]["inputs"];
            assert_eq!(class_of(&wf, "34"), "ControlNetApplyAdvanced");
            assert_eq!(apply["image"], json!(["32", 0]));
            assert_eq!(apply["positive"], json!(["2", 0]));
            assert_eq!(apply["negative"], json!(["3", 0]));
            assert_eq!(
                (
                    &apply["strength"],
                    &apply["start_percent"],
                    &apply["end_percent"]
                ),
                (&json!(0.8), &json!(0.1), &json!(0.9))
            );
            // KSampler は適用後の conditioning を使う
            assert_eq!(wf["5"]["inputs"]["positive"], json!(["34", 0]));
            assert_eq!(wf["5"]["inputs"]["negative"], json!(["34", 1]));
        }
    }

    #[test]
    fn controlnet_preprocessor_choices() {
        let imgs = IMG.map(String::from);
        let depth = build(&cn_params(ModelKind::Sd15, Preprocessor::Depth), "g", &imgs);
        assert_well_formed(&depth);
        assert_eq!(class_of(&depth, "32"), "DepthAnythingV2Preprocessor");
        assert_eq!(depth["32"]["inputs"]["resolution"], 512, "短辺に合わせる");

        let pose = build(
            &cn_params(ModelKind::Sd15, Preprocessor::Openpose),
            "g",
            &imgs,
        );
        assert_well_formed(&pose);
        assert_eq!(class_of(&pose, "32"), "OpenposePreprocessor");

        // 前処理なし: リサイズ後の画像をそのまま条件にする(前処理ノードを作らない)
        let none = build(&cn_params(ModelKind::Sd15, Preprocessor::None), "g", &imgs);
        assert_well_formed(&none);
        assert!(none.get("32").is_none());
        assert_eq!(none["34"]["inputs"]["image"], json!(["31", 0]));
    }

    #[test]
    fn controlnet_and_face_detailer_coexist_and_face_keeps_plain_conditioning() {
        let mut p = cn_params(ModelKind::Sd15, Preprocessor::Canny);
        p.face_detailer = face_params(ModelKind::Sd15, "").face_detailer;
        let wf = build(&p, "gazoo", &IMG.map(String::from));
        assert_well_formed(&wf);
        assert_eq!(wf["5"]["inputs"]["positive"], json!(["34", 0]));
        // 顔補正は ControlNet 適用前の conditioning を使う(構図条件は顔の切り出しに合わない)
        let fd = &wf[FACE_DETAILER_NODE]["inputs"];
        assert_eq!(fd["positive"], json!(["2", 0]));
        assert_eq!(fd["negative"], json!(["3", 0]));
        assert_eq!(wf["8"]["inputs"]["images"], json!([FACE_DETAILER_NODE, 0]));
    }

    #[test]
    fn disabled_controlnet_leaves_workflow_unchanged() {
        let plain = build(&sample(ModelKind::Sd15), "gazoo", &[]);
        let mut p = cn_params(ModelKind::Sd15, Preprocessor::Canny);
        p.controlnet.as_mut().unwrap().enabled = false;
        p.width = 512;
        p.height = 512;
        assert_eq!(build(&p, "gazoo", &[]), plain);
        assert!(!classes(&plain).contains(&"ControlNetApplyAdvanced"));
    }

    #[test]
    fn flux_uses_unet_dual_clip_and_guidance() {
        let wf = build(&sample(ModelKind::Flux), "gazoo", &[]);
        assert_eq!(class_of(&wf, "1"), "UNETLoader");
        assert_eq!(class_of(&wf, "2"), "DualCLIPLoader");
        assert_eq!(wf["2"]["inputs"]["clip_name1"], "t5xxl.safetensors");
        assert_eq!(wf["2"]["inputs"]["clip_name2"], "clip_l.safetensors");
        assert_eq!(wf["2"]["inputs"]["type"], "flux");
        assert_eq!(wf["5"]["inputs"]["guidance"], 3.5);
        // Flux は cfg 1.0 固定でネガティブプロンプトは使わない
        assert_eq!(wf["8"]["inputs"]["cfg"], 1.0);
        assert!(!wf.to_string().contains("blurry"));
        assert!(classes(&wf).contains(&"EmptySD3LatentImage"));
    }

    #[test]
    fn flux_gguf_uses_gguf_loader() {
        let wf = build(&sample(ModelKind::FluxGguf), "gazoo", &[]);
        assert_eq!(class_of(&wf, "1"), "UnetLoaderGGUF");
        assert_eq!(class_of(&wf, "2"), "DualCLIPLoader");
        assert!(wf["1"]["inputs"].get("weight_dtype").is_none());
    }

    #[test]
    fn gguf_text_encoder_switches_clip_loader() {
        let mut p = sample(ModelKind::FluxGguf);
        p.model.clip[0] = "t5-v1_1-xxl-Q4_K_M.gguf".into();
        let wf = build(&p, "gazoo", &[]);
        assert_eq!(class_of(&wf, "2"), "DualCLIPLoaderGGUF");
        assert_well_formed(&wf);
    }

    #[test]
    fn filename_prefix_and_size_are_applied() {
        let mut p = sample(ModelKind::Flux);
        p.width = 1024;
        p.height = 768;
        p.batch_size = 4;
        let wf = build(&p, "prefix", &[]);
        assert_eq!(wf["10"]["inputs"]["filename_prefix"], "prefix");
        assert_eq!(wf["7"]["inputs"]["width"], 1024);
        assert_eq!(wf["7"]["inputs"]["height"], 768);
        assert_eq!(wf["7"]["inputs"]["batch_size"], 4);
    }
}
