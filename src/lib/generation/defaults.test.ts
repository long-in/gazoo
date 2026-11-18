import { describe, expect, it } from "vitest";

import type { EngineModels } from "@/lib/tauri/engine";

import {
  autofill,
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
