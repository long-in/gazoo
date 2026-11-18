import { create } from "zustand";

import {
  type GenerationParams,
  type ImageItem,
  type Job,
  type JobProgress,
  jobCancel,
  jobClearFinished,
  jobEnqueue,
  jobList,
  jobReorder,
  jobRetry,
} from "@/lib/tauri/jobs";

/** 直近の生成画像として保持する最大数(セッション内)。 */
const MAX_RECENT_IMAGES = 24;

const rank = (j: Job) => (j.status === "running" ? 0 : j.status === "queued" ? 1 : 2);

/** バックエンドの `job_list` と同じ並び: 実行中 → 待機(並び順)→ 終了(新しい順)。 */
export function sortJobs(jobs: Job[]): Job[] {
  return [...jobs].sort((a, b) => {
    const r = rank(a) - rank(b);
    if (r !== 0) return r;
    if (a.status === "queued" && b.status === "queued") return a.position - b.position;
    return b.createdAt - a.createdAt;
  });
}

interface JobState {
  /** 常に `sortJobs` 済み(セレクタが毎回新しい配列を返さないよう、保持する)。 */
  jobs: Job[];
  progress: Record<string, JobProgress>;
  recentImages: ImageItem[];
  loaded: boolean;

  load: () => Promise<void>;
  applyUpdated: (job: Job) => void;
  applyProgress: (p: JobProgress) => void;
  applyImage: (image: ImageItem) => void;
  /** ギャラリーで削除された画像を「直近の結果」からも外す。 */
  removeImages: (ids: string[]) => void;

  /** 以下は失敗時に `AppError` を throw する(呼び出し側で表示)。 */
  enqueue: (params: GenerationParams, count: number) => Promise<Job[]>;
  cancel: (id: string) => Promise<void>;
  reorder: (id: string, newPosition: number) => Promise<void>;
  retry: (id: string) => Promise<Job>;
  clearFinished: () => Promise<number>;
}

export const useJobStore = create<JobState>((set, get) => ({
  jobs: [],
  progress: {},
  recentImages: [],
  loaded: false,

  load: async () => {
    try {
      const page = await jobList({ limit: 200 });
      set({ jobs: sortJobs(page.items), loaded: true });
    } catch {
      set({ loaded: true });
    }
  },

  applyUpdated: (job) =>
    set((s) => {
      const others = s.jobs.filter((j) => j.id !== job.id);
      const progress = { ...s.progress };
      if (job.status !== "running") delete progress[job.id];
      return { jobs: sortJobs([...others, job]), progress };
    }),

  applyProgress: (p) => set((s) => ({ progress: { ...s.progress, [p.jobId]: p } })),

  applyImage: (image) =>
    set((s) => ({
      recentImages: [image, ...s.recentImages.filter((i) => i.id !== image.id)].slice(0, MAX_RECENT_IMAGES),
    })),

  removeImages: (ids) =>
    set((s) => {
      const gone = new Set(ids);
      return { recentImages: s.recentImages.filter((i) => !gone.has(i.id)) };
    }),

  // 投入結果は `job://updated` でも届くが、重複しても `applyUpdated` が id で畳み込む
  enqueue: async (params, count) => {
    const jobs = await jobEnqueue(params, count);
    jobs.forEach(get().applyUpdated);
    return jobs;
  },

  cancel: (id) => jobCancel(id),

  reorder: (id, newPosition) => jobReorder(id, newPosition),

  retry: async (id) => {
    const job = await jobRetry(id);
    get().applyUpdated(job);
    return job;
  },

  clearFinished: async () => {
    const n = await jobClearFinished();
    await get().load();
    return n;
  },
}));
