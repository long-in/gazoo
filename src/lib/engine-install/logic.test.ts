import { describe, expect, it } from "vitest";

import type { InstallState, InstallView, Preflight } from "@/lib/tauri/engine-install";

import { canProceed, formatBytes, formatMb, needsGpuConfirm, overallPercent, selectionTotals, stepPercent } from "./logic";

const view = {
  base: { downloadMb: 3600, diskMb: 9000 },
  components: [
    { id: "gguf", name: "g", license: "x", downloadMb: 20, diskMb: 60 },
    { id: "face", name: "f", license: "y", downloadMb: 600, diskMb: 1500 },
  ],
} as unknown as InstallView;

const state = (patch: Partial<InstallState>): InstallState => ({
  status: "running",
  step: 1,
  totalSteps: 8,
  stepId: null,
  received: null,
  total: null,
  error: null,
  ...patch,
});

describe("engine-install logic", () => {
  it("selectionTotals: 基本+選択分", () => {
    expect(selectionTotals(view, [])).toEqual({ downloadMb: 3600, diskMb: 9000 });
    expect(selectionTotals(view, ["gguf", "face"])).toEqual({ downloadMb: 4220, diskMb: 10560 });
    expect(selectionTotals(view, ["unknown"])).toEqual({ downloadMb: 3600, diskMb: 9000 });
  });

  it("formatMb / formatBytes", () => {
    expect(formatMb(600)).toBe("600 MB");
    expect(formatMb(3500)).toBe("3.5 GB");
    expect(formatMb(12000)).toBe("12 GB");
    expect(formatBytes(12_582_912_000)).toBe("13 GB");
  });

  it("canProceed: 対象外・容量不足では進めない", () => {
    const ok = { supported: true, enoughSpace: true } as Preflight;
    expect(canProceed(ok)).toBe(true);
    expect(canProceed({ ...ok, enoughSpace: false })).toBe(false);
    expect(canProceed({ ...ok, supported: false })).toBe(false);
    expect(canProceed(null)).toBe(false);
  });

  it("needsGpuConfirm: NVIDIA が無い Windows だけ", () => {
    expect(needsGpuConfirm("missing")).toBe(true);
    expect(needsGpuConfirm("found")).toBe(false);
    expect(needsGpuConfirm("notApplicable")).toBe(false);
  });

  it("stepPercent: サイズ不明は null", () => {
    expect(stepPercent({ received: null, total: null })).toBeNull();
    expect(stepPercent({ received: 0, total: 0 })).toBeNull();
    expect(stepPercent({ received: 50, total: 200 })).toBe(25);
    expect(stepPercent({ received: 300, total: 200 })).toBe(100);
  });

  it("overallPercent: 手順番号と手順内の進捗から求める", () => {
    expect(overallPercent(state({ step: 0 }))).toBe(0);
    expect(overallPercent(state({ step: 1 }))).toBe(0);
    expect(overallPercent(state({ step: 3, received: 50, total: 100 }))).toBe(31);
    expect(overallPercent(state({ step: 8 }))).toBe(87);
    expect(overallPercent(state({ status: "done", step: 8 }))).toBe(100);
  });
});
