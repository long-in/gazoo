//! 生成パラメータから ComfyUI の API 形式ワークフロー(プロンプト JSON)を組み立てる。
//! docs/02-architecture.md §3.4。

use serde_json::{Value, json};

use crate::domain::params::{GenerationParams, ModelKind};

/// ノード出力へのリンク `[node_id, output_index]`。
fn link(node: &str, index: u32) -> Value {
    json!([node, index])
}

fn node(class_type: &str, inputs: Value) -> Value {
    json!({ "class_type": class_type, "inputs": inputs })
}

pub fn build(params: &GenerationParams, filename_prefix: &str) -> Value {
    if params.model.kind.is_flux() {
        build_flux(params, filename_prefix)
    } else {
        build_sd(params, filename_prefix)
    }
}

/// SD 1.5 / SDXL(チェックポイント一体型)。
fn build_sd(p: &GenerationParams, prefix: &str) -> Value {
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
    wf
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
    use crate::domain::params::sample;

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
            let wf = build(&sample(kind), "gazoo");
            assert_well_formed(&wf);
        }
    }

    #[test]
    fn sd_uses_checkpoint_and_both_prompts() {
        let wf = build(&sample(ModelKind::Sdxl), "gazoo");
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
        let wf = build(&p, "gazoo");
        assert_well_formed(&wf);
        assert_eq!(wf["6"]["inputs"]["vae"], json!(["7", 0]));
        assert_eq!(wf["7"]["inputs"]["vae_name"], "custom.safetensors");
    }

    #[test]
    fn flux_uses_unet_dual_clip_and_guidance() {
        let wf = build(&sample(ModelKind::Flux), "gazoo");
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
        let wf = build(&sample(ModelKind::FluxGguf), "gazoo");
        assert_eq!(class_of(&wf, "1"), "UnetLoaderGGUF");
        assert_eq!(class_of(&wf, "2"), "DualCLIPLoader");
        assert!(wf["1"]["inputs"].get("weight_dtype").is_none());
    }

    #[test]
    fn gguf_text_encoder_switches_clip_loader() {
        let mut p = sample(ModelKind::FluxGguf);
        p.model.clip[0] = "t5-v1_1-xxl-Q4_K_M.gguf".into();
        let wf = build(&p, "gazoo");
        assert_eq!(class_of(&wf, "2"), "DualCLIPLoaderGGUF");
        assert_well_formed(&wf);
    }

    #[test]
    fn filename_prefix_and_size_are_applied() {
        let mut p = sample(ModelKind::Flux);
        p.width = 1024;
        p.height = 768;
        p.batch_size = 4;
        let wf = build(&p, "prefix");
        assert_eq!(wf["10"]["inputs"]["filename_prefix"], "prefix");
        assert_eq!(wf["7"]["inputs"]["width"], 1024);
        assert_eq!(wf["7"]["inputs"]["height"], 768);
        assert_eq!(wf["7"]["inputs"]["batch_size"], 4);
    }
}
