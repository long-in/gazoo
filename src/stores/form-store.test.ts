import { beforeEach, describe, expect, it } from "vitest";

import { defaultParams } from "@/lib/generation/defaults";

import { useFormStore } from "./form-store";

beforeEach(() => {
  useFormStore.setState({ params: defaultParams("sdxl"), count: 1, hydrated: false });
});

describe("form-store.loadParams", () => {
  it("画像のパラメータを読み込み、シードは固定にして復元済みにする", () => {
    const saved = { ...defaultParams("flux_gguf"), prompt: "a cat", seed: 123, seedRandomize: true };
    useFormStore.getState().loadParams(saved);
    const s = useFormStore.getState();
    expect(s.params).toMatchObject({ prompt: "a cat", seed: 123, seedRandomize: false });
    expect(s.params.model.kind).toBe("flux_gguf");
    expect(s.hydrated).toBe(true);
  });

  it("読み込み後は、設定からの初回復元で上書きされない", () => {
    useFormStore.getState().loadParams({ ...defaultParams("sd15"), prompt: "kept" });
    useFormStore.getState().hydrate({ prompt: "from settings" });
    expect(useFormStore.getState().params.prompt).toBe("kept");
  });

  it("欠けた項目は既定値で補う", () => {
    useFormStore.getState().loadParams({ prompt: "x", model: { kind: "sd15", name: "m", clip: [], vae: null } });
    expect(useFormStore.getState().params).toMatchObject({ width: 512, steps: 20 });
  });
});

describe("form-store.applyPreset", () => {
  it("シードのランダム設定を含めてそのまま復元する(loadParams との違い)", () => {
    const preset = { ...defaultParams("sd15"), prompt: "p", seedRandomize: true, seed: 5 };
    useFormStore.getState().applyPreset(preset);
    expect(useFormStore.getState().params).toMatchObject({ prompt: "p", seedRandomize: true, seed: 5 });
    expect(useFormStore.getState().hydrated).toBe(true);
  });
});

describe("form-store.patchFace", () => {
  it("未設定なら既定値から作って更新する", () => {
    useFormStore.getState().loadParams(defaultParams("sd15"));
    expect(useFormStore.getState().params.faceDetailer).toBeUndefined();
    useFormStore.getState().patchFace({ enabled: true, denoise: 0.55 });
    expect(useFormStore.getState().params.faceDetailer).toEqual({
      enabled: true,
      detector: "",
      denoise: 0.55,
      prompt: "",
      confidence: 0.5,
    });
  });

  it("一部だけ更新し、ほかの項目は保つ", () => {
    useFormStore.getState().loadParams(defaultParams("sd15"));
    useFormStore.getState().patchFace({ enabled: true, denoise: 0.55 });
    useFormStore.getState().patchFace({ prompt: "detailed eyes" });
    useFormStore.getState().patchFace({ confidence: 0.3 });
    expect(useFormStore.getState().params.faceDetailer).toMatchObject({
      enabled: true,
      denoise: 0.55,
      prompt: "detailed eyes",
      confidence: 0.3,
    });
  });
});

describe("form-store: ControlNet", () => {
  const CNS = ["control_canny.safetensors", "control_depth.safetensors"];

  it("ユニットが未設定なら既定から作り、前処理を変えると合うモデルへ切り替える", () => {
    useFormStore.getState().loadParams(defaultParams("sd15"));
    useFormStore.getState().patchControlnetUnit({ image: "inputs/a.png" }, CNS);
    expect(useFormStore.getState().params.controlnet).toEqual({
      enabled: false,
      units: [
        { image: "inputs/a.png", model: "control_canny.safetensors", preprocessor: "canny", strength: 1, startPercent: 0, endPercent: 1 },
      ],
    });
    useFormStore.getState().patchControlnetUnit({ preprocessor: "depth" }, CNS);
    expect(useFormStore.getState().params.controlnet?.units[0]).toMatchObject({
      preprocessor: "depth",
      model: "control_depth.safetensors",
      image: "inputs/a.png",
    });
    // モデルを直接選んだときは、前処理に合わせて勝手に戻さない
    useFormStore.getState().patchControlnetUnit({ model: "control_canny.safetensors" }, CNS);
    expect(useFormStore.getState().params.controlnet?.units[0].model).toBe("control_canny.safetensors");
  });

  it("有効・無効の切り替えで、ユニットの設定は残る", () => {
    useFormStore.getState().loadParams(defaultParams("sd15"));
    useFormStore.getState().patchControlnetUnit({ image: "inputs/a.png", strength: 0.6 }, CNS);
    useFormStore.getState().setControlnetEnabled(true);
    useFormStore.getState().setControlnetEnabled(false);
    expect(useFormStore.getState().params.controlnet).toMatchObject({
      enabled: false,
      units: [{ image: "inputs/a.png", strength: 0.6 }],
    });
  });

  it("setReferenceImage: 画像を設定して有効にする。Flux では有効にしない", () => {
    useFormStore.getState().loadParams(defaultParams("sdxl"));
    expect(useFormStore.getState().setReferenceImage("inputs/g.png", CNS)).toBe(true);
    expect(useFormStore.getState().params.controlnet).toMatchObject({
      enabled: true,
      units: [{ image: "inputs/g.png", model: "control_canny.safetensors" }],
    });

    useFormStore.getState().loadParams(defaultParams("flux"));
    expect(useFormStore.getState().setReferenceImage("inputs/g.png", CNS)).toBe(false);
    expect(useFormStore.getState().params.controlnet).toMatchObject({
      enabled: false,
      units: [{ image: "inputs/g.png" }],
    });
  });
});

describe("applySuggestion / undoSuggestion", () => {
  it("applies usable changes, then undoes only those fields", () => {
    useFormStore.setState({ params: { ...defaultParams("sd15"), steps: 20, width: 512 }, suggestionUndo: null });
    const models = {
      checkpoints: [],
      unets: [],
      ggufUnets: [],
      clips: [],
      vaes: [],
      loras: [],
      faceDetectors: [],
      controlnets: [],
      samplers: ["euler"],
      schedulers: ["normal"],
    };
    const ok = useFormStore.getState().applySuggestion({ steps: 40, sampler: "bad" }, models);
    expect(ok).toBe(true);
    expect(useFormStore.getState().params.steps).toBe(40);
    expect(useFormStore.getState().params.sampler).toBe("euler");

    useFormStore.getState().patch({ width: 768 }); // 適用後の手動編集
    useFormStore.getState().undoSuggestion();
    expect(useFormStore.getState().params.steps).toBe(20);
    expect(useFormStore.getState().params.width).toBe(768);
    expect(useFormStore.getState().suggestionUndo).toBeNull();
  });

  it("returns false and changes nothing when nothing is usable", () => {
    useFormStore.setState({ params: defaultParams("flux"), suggestionUndo: null });
    expect(useFormStore.getState().applySuggestion({ cfg: 7 }, null)).toBe(false);
    expect(useFormStore.getState().suggestionUndo).toBeNull();
  });
});
