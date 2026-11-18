import { beforeEach, describe, expect, it } from "vitest";

import { defaultParams } from "@/lib/generation/defaults";
import type { ImageItem, Job } from "@/lib/tauri/jobs";

import { sortJobs, useJobStore } from "./job-store";

const job = (id: string, over: Partial<Job> = {}): Job => ({
  id,
  status: "queued",
  position: 0,
  params: defaultParams("sdxl"),
  imageIds: [],
  createdAt: 0,
  ...over,
});

const image = (id: string): ImageItem => ({
  id,
  jobId: "j",
  width: 1,
  height: 1,
  seed: 1,
  isFavorite: false,
  createdAt: 0,
  filePath: `/x/${id}.png`,
  thumbPath: `/x/${id}.png`,
});

beforeEach(() => {
  useJobStore.setState({ jobs: [], progress: {}, recentImages: [], loaded: false });
});

describe("sortJobs", () => {
  it("実行中 → 待機(position 昇順)→ 終了(新しい順)", () => {
    const sorted = sortJobs([
      job("old", { status: "completed", createdAt: 1 }),
      job("q2", { position: 5 }),
      job("new", { status: "failed", createdAt: 9 }),
      job("run", { status: "running" }),
      job("q1", { position: 2 }),
    ]);
    expect(sorted.map((j) => j.id)).toEqual(["run", "q1", "q2", "new", "old"]);
  });
});

describe("job-store", () => {
  it("更新イベントを id で畳み込み、並び順を保つ", () => {
    const { applyUpdated } = useJobStore.getState();
    applyUpdated(job("a", { position: 1 }));
    applyUpdated(job("b", { position: 0 }));
    applyUpdated(job("a", { position: 1, status: "running" }));
    expect(useJobStore.getState().jobs.map((j) => [j.id, j.status])).toEqual([
      ["a", "running"],
      ["b", "queued"],
    ]);
  });

  it("実行中でなくなったジョブの進捗は消える", () => {
    const s = useJobStore.getState();
    s.applyUpdated(job("a", { status: "running" }));
    s.applyProgress({ jobId: "a", step: 3, totalSteps: 20 });
    expect(useJobStore.getState().progress.a.step).toBe(3);
    s.applyUpdated(job("a", { status: "completed" }));
    expect(useJobStore.getState().progress.a).toBeUndefined();
  });

  it("直近の画像は新しい順で、重複せず、上限がある", () => {
    const { applyImage } = useJobStore.getState();
    for (let i = 0; i < 30; i++) applyImage(image(`i${i}`));
    applyImage(image("i29")); // 重複
    const recent = useJobStore.getState().recentImages;
    expect(recent).toHaveLength(24);
    expect(recent[0].id).toBe("i29");
    expect(new Set(recent.map((i) => i.id)).size).toBe(24);
  });
});

describe("job-store removeImages", () => {
  it("削除された画像を直近の結果から外す", () => {
    const s = useJobStore.getState();
    s.applyImage(image("x"));
    s.applyImage(image("y"));
    s.removeImages(["x"]);
    expect(useJobStore.getState().recentImages.map((i) => i.id)).toEqual(["y"]);
  });
});
