import { describe, expect, it } from "vitest";

import type { EngineModels } from "@/lib/tauri/engine";

import {
  autofill,
  defaultControlnet,
  defaultControlnetUnit,
  defaultFaceDetailer,
  pickControlnetModel,
  defaultParams,
  MAX_SEED,
  modelsFor,
  randomSeed,
  restoreParams,
  switchKind,
} from "./defaults";

const MODELS: EngineModels = {
  checkpoints: ["sd15.safetensors", "sdxl.safetensors"],
  unets: ["flux1-dev.safetensors"],
  ggufUnets: ["flux1-dev-Q4_K_S.gguf"],
  clips: ["clip_l.safetensors", "t5xxl_fp8_e4m3fn.safetensors"],
  vaes: ["ae.safetensors", "sdxl_vae.safetensors"],
  loras: [],
  faceDetectors: ["bbox/hand_yolov8s.pt", "bbox/face_yolov8m.pt"],
  controlnets: ["control_v11f1p_sd15_depth.safetensors", "control_v11p_sd15_canny.safetensors", "control_v11p_sd15_openpose.safetensors"],
  samplers: ["euler", "dpmpp_2m"],
  schedulers: ["simple", "karras", "normal"],
};

describe("defaultParams", () => {
  it("種別ごとに推奨値が違う", () => {
    expect(defaultParams("sd15")).toMatchObject({ width: 512, steps: 20, cfg: 7 });
    expect(defaultParams("sdxl")).toMatchObject({ width: 1024, sampler: "dpmpp_2m", scheduler: "karras" });
    expect(defaultParams("flux_gguf")).toMatchObject({ cfg: 1, guidance: 3.5, scheduler: "simple" });
  });

  it("サイズはすべて 64 の倍数", () => {
    for (const k of ["sd15", "sdxl", "flux", "flux_gguf"] as const) {
      const p = defaultParams(k);
      expect(p.width % 64).toBe(0);
      expect(p.height % 64).toBe(0);
    }
  });
});

describe("switchKind", () => {
  it("プロンプトとシード設定は引き継ぎ、それ以外は種別の既定に戻す", () => {
    const sd = { ...defaultParams("sd15"), prompt: "a cat", negativePrompt: "blur", seed: 7, seedRandomize: false };
    const flux = switchKind(sd, "flux");
    expect(flux.model.kind).toBe("flux");
    expect(flux).toMatchObject({ prompt: "a cat", negativePrompt: "blur", seed: 7, seedRandomize: false });
    expect(flux.width).toBe(1024);
    expect(flux.model.name).toBe("");
  });

  it("同じ種別なら同一参照を返す", () => {
    const p = defaultParams("sdxl");
    expect(switchKind(p, "sdxl")).toBe(p);
  });
});

describe("modelsFor / autofill", () => {
  it("種別ごとに対応する一覧を返す", () => {
    expect(modelsFor("sdxl", MODELS)).toEqual(MODELS.checkpoints);
    expect(modelsFor("flux", MODELS)).toEqual(MODELS.unets);
    expect(modelsFor("flux_gguf", MODELS)).toEqual(MODELS.ggufUnets);
    expect(modelsFor("sdxl", null)).toEqual([]);
  });

  it("一覧が未取得なら何も変えない", () => {
    const p = defaultParams("sdxl");
    expect(autofill(p, null)).toBe(p);
  });

  it("SD: 先頭のチェックポイントを選び、VAE は内蔵(null)のまま", () => {
    const p = autofill(defaultParams("sdxl"), MODELS);
    expect(p.model).toMatchObject({ name: "sd15.safetensors", clip: [], vae: null });
  });

  it("Flux GGUF: モデル・T5 / CLIP-L・VAE を名前から推定して埋める", () => {
    const p = autofill(defaultParams("flux_gguf"), MODELS);
    expect(p.model.name).toBe("flux1-dev-Q4_K_S.gguf");
    expect(p.model.clip).toEqual(["t5xxl_fp8_e4m3fn.safetensors", "clip_l.safetensors"]);
    expect(p.model.vae).toBe("ae.safetensors");
  });

  it("有効な選択は保ち、存在しないものだけ直す", () => {
    const p = defaultParams("sdxl");
    p.model.name = "sdxl.safetensors";
    p.sampler = "no-such-sampler";
    const out = autofill(p, MODELS);
    expect(out.model.name).toBe("sdxl.safetensors");
    expect(out.sampler).toBe("euler");
    expect(autofill(out, MODELS)).toBe(out); // 冪等(再レンダー抑止)
  });

  it("モデルが無い種別では名前を空にする", () => {
    const p = autofill(defaultParams("flux_gguf"), { ...MODELS, ggufUnets: [] });
    expect(p.model.name).toBe("");
  });
});

describe("restoreParams", () => {
  it("null なら sdxl の既定", () => {
    expect(restoreParams(null).model.kind).toBe("sdxl");
  });

  it("欠けた項目は既定で補い、保存値は優先する", () => {
    const p = restoreParams({ model: { kind: "flux", name: "m", clip: [], vae: null }, prompt: "x", steps: 8 });
    expect(p.model.kind).toBe("flux");
    expect(p).toMatchObject({ prompt: "x", steps: 8, width: 1024, guidance: 3.5 });
  });

  it("不正な種別は無視する", () => {
    const p = restoreParams({ model: { kind: "bogus" as never, name: "", clip: [], vae: null } });
    expect(p.width).toBe(1024);
  });
});

describe("randomSeed", () => {
  it("0 以上 2^53 未満の整数", () => {
    for (let i = 0; i < 200; i++) {
      const s = randomSeed();
      expect(Number.isSafeInteger(s)).toBe(true);
      expect(s).toBeGreaterThanOrEqual(0);
      expect(s).toBeLessThanOrEqual(MAX_SEED);
    }
  });
});

describe("顔補正(faceDetailer)", () => {
  const withFace = (kind: Parameters<typeof defaultParams>[0], over = {}) => ({
    ...defaultParams(kind),
    faceDetailer: { ...defaultFaceDetailer(), enabled: true, detector: "bbox/face_yolov8m.pt", ...over },
  });

  it("既定は無効で、下書き・プリセットに無ければ付かない", () => {
    expect(defaultFaceDetailer().enabled).toBe(false);
    expect(defaultParams("sd15").faceDetailer).toBeUndefined();
    expect(restoreParams({ prompt: "x" }).faceDetailer).toBeUndefined();
  });

  it("復元時、欠けた項目は既定で補う", () => {
    const p = restoreParams({ faceDetailer: { enabled: true, detector: "bbox/f.pt" } as never });
    expect(p.faceDetailer).toEqual({ enabled: true, detector: "bbox/f.pt", denoise: 0.4, prompt: "", confidence: 0.5 });
  });

  it("SD 系どうしの切り替えでは設定を引き継ぐ", () => {
    const next = switchKind(withFace("sd15", { denoise: 0.6 }), "sdxl");
    expect(next.faceDetailer).toMatchObject({ enabled: true, denoise: 0.6 });
  });

  it("Flux へ切り替えると無効になるが、値は残る(SD に戻したときに再設定させない)", () => {
    const flux = switchKind(withFace("sdxl", { denoise: 0.6 }), "flux");
    expect(flux.faceDetailer).toMatchObject({ enabled: false, denoise: 0.6, detector: "bbox/face_yolov8m.pt" });
    // 戻しても自動では有効にならない
    expect(switchKind(flux, "sd15").faceDetailer?.enabled).toBe(false);
  });

  it("autofill: 検出モデルが未選択・不在なら face 系を優先して選ぶ", () => {
    expect(autofill(withFace("sd15", { detector: "" }), MODELS).faceDetailer?.detector).toBe("bbox/face_yolov8m.pt");
    expect(autofill(withFace("sd15", { detector: "bbox/gone.pt" }), MODELS).faceDetailer?.detector).toBe(
      "bbox/face_yolov8m.pt",
    );
  });

  it("autofill: 有効な選択は保ち、一覧が空(ノード未導入)なら設定に触らない", () => {
    const keep = withFace("sd15", { detector: "bbox/hand_yolov8s.pt" });
    expect(autofill(keep, MODELS).faceDetailer).toBe(keep.faceDetailer);
    const noDetectors = { ...MODELS, faceDetectors: [] };
    const p = withFace("sd15", { detector: "bbox/anything.pt" });
    expect(autofill(p, noDetectors).faceDetailer?.detector).toBe("bbox/anything.pt");
  });
});

describe("ControlNet", () => {
  const withCn = (kind: Parameters<typeof defaultParams>[0], unit = {}, enabled = true) => ({
    ...defaultParams(kind),
    controlnet: { enabled, units: [{ ...defaultControlnetUnit(), image: "inputs/a.png", ...unit }] },
  });

  it("既定は無効・ユニットなしで、下書きに無ければ付かない", () => {
    expect(defaultControlnet()).toEqual({ enabled: false, units: [] });
    expect(defaultParams("sd15").controlnet).toBeUndefined();
    expect(restoreParams({ prompt: "x" }).controlnet).toBeUndefined();
  });

  it("復元時、欠けた項目は既定で補い、ユニットは 1 件に限る", () => {
    const p = restoreParams({
      controlnet: { enabled: true, units: [{ image: "inputs/a.png", model: "m" }, { image: "inputs/b.png", model: "n" }] } as never,
    });
    expect(p.controlnet?.units).toEqual([
      { image: "inputs/a.png", model: "m", preprocessor: "canny", strength: 1, startPercent: 0, endPercent: 1 },
    ]);
  });

  it("SD 系どうしの切り替えでは引き継ぎ、Flux では無効になるが値は残る", () => {
    expect(switchKind(withCn("sd15", { strength: 0.7 }), "sdxl").controlnet).toMatchObject({
      enabled: true,
      units: [{ strength: 0.7, image: "inputs/a.png" }],
    });
    const flux = switchKind(withCn("sdxl"), "flux");
    expect(flux.controlnet?.enabled).toBe(false);
    expect(flux.controlnet?.units[0].image).toBe("inputs/a.png");
    expect(switchKind(flux, "sd15").controlnet?.enabled).toBe(false);
  });

  it("pickControlnetModel: 前処理に合うモデルを優先し、有効な選択は保つ", () => {
    const list = MODELS.controlnets;
    const canny = "control_v11p_sd15_canny.safetensors";
    expect(pickControlnetModel(list, "canny", "")).toBe(canny);
    expect(pickControlnetModel(list, "depth", canny)).toBe("control_v11f1p_sd15_depth.safetensors");
    expect(pickControlnetModel(list, "openpose", canny)).toBe("control_v11p_sd15_openpose.safetensors");
    // 合うものが現在のモデルなら保つ
    expect(pickControlnetModel(list, "canny", canny)).toBe(canny);
    // 前処理なし: 現在の選択が一覧にあれば保つ。無ければ先頭
    expect(pickControlnetModel(list, "none", canny)).toBe(canny);
    expect(pickControlnetModel(list, "none", "gone.safetensors")).toBe(list[0]);
    // 合う名前が無ければ現在値(一覧にあれば)を保つ
    expect(pickControlnetModel(["a.safetensors", "b.safetensors"], "depth", "b.safetensors")).toBe("b.safetensors");
    // 一覧が空なら触らない
    expect(pickControlnetModel([], "canny", "x")).toBe("x");
  });

  it("autofill: 未選択・不在のモデルを前処理に合わせて埋め、有効な選択は保つ", () => {
    expect(autofill(withCn("sd15", { model: "", preprocessor: "openpose" }), MODELS).controlnet?.units[0].model).toBe(
      "control_v11p_sd15_openpose.safetensors",
    );
    expect(autofill(withCn("sd15", { model: "gone.safetensors" }), MODELS).controlnet?.units[0].model).toBe(
      "control_v11p_sd15_canny.safetensors",
    );
    const keep = withCn("sd15", { model: "control_v11p_sd15_canny.safetensors" });
    expect(autofill(keep, MODELS).controlnet).toBe(keep.controlnet);
    // 一覧が空(ノード未導入)なら設定に触らない
    const p = withCn("sd15", { model: "anything.safetensors" });
    expect(autofill(p, { ...MODELS, controlnets: [] }).controlnet?.units[0].model).toBe("anything.safetensors");
  });
});
