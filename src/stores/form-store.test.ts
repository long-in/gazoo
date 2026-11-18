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
