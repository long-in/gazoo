import { describe, expect, it } from "vitest";

import type { EngineModels } from "@/lib/tauri/engine";
import type { ModelCatalog, ModelFamily, ModelProfile, ModelVariant } from "@/lib/tauri/models";

import { defaultParams, supportsImg2img, switchKind } from "./defaults";
import {
  animeStart,
  DENOISE_LEVELS,
  defaultImg2img,
  findAnimeTarget,
  img2imgReady,
  isCompositionLocked,
  levelOf,
  withCompositionLock,
  withImg2imgEnabled,
  withImg2imgImage,
} from "./img2img";

const L = { ja: "x", en: "x" };
const profile: ModelProfile = {
  summary: L,
  style: ["anime"],
  bestFor: { ja: ["a"], en: ["a"] },
  weakAt: { ja: ["b"], en: ["b"] },
  promptStyle: { kind: "tags", hint: L },
  recommended: { steps: 28, cfg: 5, sampler: "euler_ancestral", scheduler: "normal", size: { width: 832, height: 1216 } },
  resources: { vramGb: 8, speed: "medium" },
  nsfw: "capable",
  sourceNote: L,
};
const variant = (fileName: string, over: Partial<ModelVariant> = {}): ModelVariant => ({
  id: fileName,
  label: L,
  recommended: true,
  source: { url: "https://huggingface.co/a/b/resolve/c/m.safetensors", repo: "a/b" },
  sizeBytes: 1,
  sha256: "0".repeat(64),
  destDir: "checkpoints",
  fileName,
  license: { name: "l", url: "https://x", commercialUse: true },
  requiresConsent: false,
  baseModel: "sdxl",
  ...over,
});
const fam = (id: string, fileName: string, over: Partial<ModelFamily> = {}): ModelFamily => ({
  id,
  category: "checkpoint",
  name: L,
  description: L,
  usedBy: [],
  tags: ["anime"],
  profile,
  variants: [variant(fileName)],
  ...over,
});
const catalog: ModelCatalog = {
  schemaVersion: 2,
  families: [
    fam("checkpoint-sdxl", "sd_xl_base_1.0.safetensors", { tags: [] }),
    fam("checkpoint-animagine-xl-4", "animagine-xl-4.0-opt.safetensors"),
  ],
};
const models = (over: Partial<EngineModels> = {}): EngineModels => ({
  checkpoints: ["sd_xl_base_1.0.safetensors", "animagine-xl-4.0-opt.safetensors"],
  unets: [],
  ggufUnets: [],
  clips: [],
  vaes: [],
  loras: [],
  faceDetectors: [],
  controlnets: ["controlnet-canny-sdxl.safetensors"],
  samplers: ["euler_ancestral", "euler"],
  schedulers: ["normal", "karras"],
  ...over,
});
const IMG = { path: "inputs/src.png", width: 1000, height: 600 };

describe("img2img の基本", () => {
  it("既定は無効・強さ 0.6 で、SD 系だけが対応する", () => {
    expect(defaultImg2img()).toEqual({ enabled: false, image: "", denoise: 0.6 });
    expect(supportsImg2img("sd15") && supportsImg2img("sdxl")).toBe(true);
    for (const k of ["flux", "flux_gguf", "anima", "qwen_image", "krea2"] as const) expect(supportsImg2img(k)).toBe(false);
  });

  it("強さのボタンと数値が対応し、元画像が無ければ実行できない", () => {
    expect(DENOISE_LEVELS).toEqual({ weak: 0.45, medium: 0.6, strong: 0.75 });
    expect(levelOf(0.6)).toBe("medium");
    expect(levelOf(0.5)).toBeNull();
    expect(img2imgReady(undefined)).toBe(true);
    expect(img2imgReady({ enabled: true, image: "", denoise: 0.6 })).toBe(false);
    expect(img2imgReady({ enabled: true, image: "inputs/a.png", denoise: 0.6 })).toBe(true);
    expect(img2imgReady({ enabled: false, image: "", denoise: 0.6 })).toBe(true);
  });

  it("元画像でサイズが決まり(64 の倍数に丸め)、有効にすると Inpaint を切る", () => {
    const base = {
      ...defaultParams("sdxl"),
      inpaint: { enabled: true, image: "inputs/i.png", mask: "inputs/m.png", denoise: 0.7, growMask: 8, feather: 8, compositeBack: true },
    };
    const set = withImg2imgImage(base, IMG.path, IMG.width, IMG.height);
    expect([set.width, set.height]).toEqual([1024, 576]);
    expect(set.img2img?.image).toBe(IMG.path);
    const on = withImg2imgEnabled(set, true);
    expect(on.img2img?.enabled).toBe(true);
    expect(on.inpaint?.enabled).toBe(false);
  });

  it("種別を Flux などへ切り替えると無効になる(画像は残る)", () => {
    const on = withImg2imgEnabled(withImg2imgImage(defaultParams("sdxl"), IMG.path, 512, 512), true);
    const flux = switchKind(on, "flux");
    expect(flux.img2img).toEqual({ enabled: false, image: IMG.path, denoise: 0.6 });
  });
});

describe("構図の固定", () => {
  const withImage = withImg2imgImage(defaultParams("sdxl"), IMG.path, 1024, 1024);

  it("入れると ControlNet を元画像・Canny・強さ 0.8・終了 0.8 で設定する", () => {
    const p = withCompositionLock(withImage, true, ["a-canny.safetensors", "b-depth.safetensors"]);
    expect(p.controlnet?.enabled).toBe(true);
    expect(p.controlnet?.units[0]).toMatchObject({
      image: IMG.path,
      preprocessor: "canny",
      strength: 0.8,
      startPercent: 0,
      endPercent: 0.8,
      model: "a-canny.safetensors",
    });
    expect(isCompositionLocked(p)).toBe(true);
  });

  it("切ると ControlNet を無効にする。ControlNet の画像が別なら、固定とはみなさない", () => {
    const locked = withCompositionLock(withImage, true, ["c.safetensors"]);
    expect(isCompositionLocked(withCompositionLock(locked, false, []))).toBe(false);
    const other = { ...locked, controlnet: { enabled: true, units: [{ ...locked.controlnet!.units[0], image: "inputs/other.png" }] } };
    expect(isCompositionLocked(other)).toBe(false);
  });

  it("固定中に元画像を変えると、ControlNet の画像も揃う", () => {
    const locked = withCompositionLock(withImage, true, ["c.safetensors"]);
    const changed = withImg2imgImage(locked, "inputs/new.png", 512, 512);
    expect(changed.controlnet?.units[0].image).toBe("inputs/new.png");
    expect(isCompositionLocked(changed)).toBe(true);
  });
});

describe("画像からアニメ調に", () => {
  it("導入済みのアニメ系 SDXL だけが対象になる(SDXL 標準は対象外)", () => {
    expect(findAnimeTarget(catalog, models())?.name).toBe("animagine-xl-4.0-opt.safetensors");
    expect(findAnimeTarget(catalog, models({ checkpoints: ["sd_xl_base_1.0.safetensors"] }))).toBeNull();
    expect(findAnimeTarget(null, models())).toBeNull();
    expect(findAnimeTarget(catalog, null)).toBeNull();
  });

  it("モデル・推奨値・強さ 0.6・元画像のサイズ・構図の固定を一度に設定し、プロンプトは保つ", () => {
    const cur = { ...defaultParams("sd15"), prompt: "1girl" };
    const r = animeStart(cur, IMG, { catalog, models: models(), cannyAvailable: true });
    expect(r.outcome).toBe("ok");
    expect(r.compositionLocked).toBe(true);
    const p = r.params;
    expect(p.model).toMatchObject({ kind: "sdxl", name: "animagine-xl-4.0-opt.safetensors" });
    expect(p.prompt).toBe("1girl");
    expect([p.steps, p.cfg, p.sampler]).toEqual([28, 5, "euler_ancestral"]);
    expect([p.width, p.height]).toEqual([1024, 576]);
    expect(p.img2img).toEqual({ enabled: true, image: IMG.path, denoise: 0.6 });
    expect(p.controlnet?.units[0].image).toBe(IMG.path);
  });

  it("Canny か ControlNet モデルが無ければ、構図の固定だけオフにする", () => {
    const noCanny = animeStart(defaultParams("sdxl"), IMG, { catalog, models: models(), cannyAvailable: false });
    expect(noCanny.outcome).toBe("ok");
    expect(noCanny.compositionLocked).toBe(false);
    expect(noCanny.params.controlnet?.enabled ?? false).toBe(false);
    const noModel = animeStart(defaultParams("sdxl"), IMG, { catalog, models: models({ controlnets: [] }), cannyAvailable: true });
    expect(noModel.compositionLocked).toBe(false);
  });

  it("Inpaint が有効なら切る。アニメ系モデルが無ければ何も変えない", () => {
    const cur = {
      ...defaultParams("sdxl"),
      inpaint: { enabled: true, image: "inputs/i.png", mask: "inputs/m.png", denoise: 0.7, growMask: 8, feather: 8, compositeBack: true },
    };
    const r = animeStart(cur, IMG, { catalog, models: models(), cannyAvailable: true });
    expect(r.params.inpaint?.enabled).toBe(false);

    const none = animeStart(cur, IMG, { catalog, models: models({ checkpoints: [] }), cannyAvailable: true });
    expect(none.outcome).toBe("noAnimeModel");
    expect(none.params).toBe(cur);
  });
});
