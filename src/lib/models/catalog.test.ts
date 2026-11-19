import { describe, expect, it } from "vitest";

import type { DownloadState, ModelFamily, ModelVariant } from "@/lib/tauri/models";

import {
  downloadFor,
  etaSeconds,
  formatBytes,
  formatDuration,
  isBase,
  isCategory,
  matchesBase,
  pickVariant,
  progressPercent,
  visibleFamilies,
} from "./catalog";

const L = { ja: "x", en: "x" };
const variant = (id: string, over: Partial<ModelVariant> = {}): ModelVariant => ({
  id,
  label: L,
  recommended: false,
  source: { url: "https://huggingface.co/a/b/resolve/c/m.safetensors", repo: "a/b" },
  sizeBytes: 1,
  sha256: "0".repeat(64),
  destDir: "controlnet",
  fileName: `${id}.safetensors`,
  license: { name: "l", url: "https://x", commercialUse: true },
  requiresConsent: false,
  ...over,
});
const family = (id: string, category: ModelFamily["category"], variants: ModelVariant[]): ModelFamily => ({
  id,
  category,
  name: L,
  description: L,
  usedBy: [],
  variants,
});

const canny = family("controlnet-canny", "controlnet", [
  variant("sdxl-xinsir", { baseModel: "sdxl", recommended: true }),
  variant("sdxl-diffusers", { baseModel: "sdxl" }),
  variant("sd15", { baseModel: "sd15", recommended: true }),
]);
const face = family("detector-face", "detector", [variant("yolov9c", { recommended: true }), variant("yolov8m")]);
const sd15ckpt = family("checkpoint-sd15", "checkpoint", [variant("a", { baseModel: "sd15" })]);

describe("絞り込み", () => {
  it("カテゴリで絞る", () => {
    const r = visibleFamilies([canny, face, sd15ckpt], "controlnet", "all");
    expect(r.map((x) => x.family.id)).toEqual(["controlnet-canny"]);
    expect(r[0].variants).toHaveLength(3);
  });

  it("ベースモデルで variant を絞る。表示できる variant が無いファミリーは除く", () => {
    const r = visibleFamilies([canny, face, sd15ckpt], "controlnet", "sd15");
    expect(r[0].variants.map((v) => v.id)).toEqual(["sd15"]);
    expect(visibleFamilies([sd15ckpt], "checkpoint", "sdxl")).toEqual([]);
  });

  it("ベースモデルを持たない variant(検出モデル)はどの絞り込みでも表示する", () => {
    expect(visibleFamilies([face], "detector", "sdxl")[0].variants).toHaveLength(2);
    expect(matchesBase(face.variants[0], "sd15")).toBe(true);
  });

  it("URL パラメータの値を検証する", () => {
    expect(isCategory("controlnet")).toBe(true);
    expect(isCategory("evil")).toBe(false);
    expect(isCategory(null)).toBe(false);
    expect(isBase("sdxl")).toBe(true);
    expect(isBase("flux")).toBe(false);
  });
});

describe("variant の選択", () => {
  const sdxl = canny.variants.filter((v) => v.baseModel === "sdxl");

  it("ユーザーの選択が表示中ならそれを使う", () => {
    expect(pickVariant(sdxl, "sdxl-diffusers").id).toBe("sdxl-diffusers");
  });

  it("選択が絞り込みで消えていたら、推奨にする", () => {
    expect(pickVariant(sdxl, "sd15").id).toBe("sdxl-xinsir");
    expect(pickVariant(sdxl, undefined).id).toBe("sdxl-xinsir");
  });

  it("推奨が無ければ先頭", () => {
    expect(pickVariant([variant("p"), variant("q")], undefined).id).toBe("p");
  });
});

describe("ダウンロード状態", () => {
  const d = (over: Partial<DownloadState>): DownloadState => ({
    downloadId: "1",
    familyId: "f",
    variantId: "v",
    status: "downloading",
    receivedBytes: 50,
    totalBytes: 200,
    ...over,
  });

  it("同じモデルの取得中を優先して返す", () => {
    const list = [d({ downloadId: "old", status: "failed" }), d({ downloadId: "new", status: "queued" })];
    expect(downloadFor(list, "f", "v")?.downloadId).toBe("new");
    expect(downloadFor(list, "f", "other")).toBeUndefined();
  });

  it("進捗率は 0〜100 に収める", () => {
    expect(progressPercent(d({}))).toBe(25);
    expect(progressPercent(d({ receivedBytes: 999 }))).toBe(100);
    expect(progressPercent(d({ totalBytes: 0 }))).toBe(0);
  });

  it("残り時間は速度が分かるときだけ", () => {
    expect(etaSeconds(d({ bytesPerSec: 50 }))).toBe(3);
    expect(etaSeconds(d({}))).toBeNull();
  });
});

describe("書式", () => {
  it("バイト数", () => {
    expect(formatBytes(6_938_078_334)).toBe("6.9 GB");
    expect(formatBytes(52_026_019)).toBe("52 MB");
    expect(formatBytes(2_502_139_136)).toBe("2.5 GB");
    expect(formatBytes(512)).toBe("512 B");
  });

  it("時間", () => {
    expect(formatDuration(5)).toBe("5s");
    expect(formatDuration(125)).toBe("2m 5s");
    expect(formatDuration(3700)).toBe("1h 1m");
  });
});
