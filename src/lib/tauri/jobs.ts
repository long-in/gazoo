import { invoke } from "./invoke";

export type ModelKind = "sd15" | "sdxl" | "flux" | "flux_gguf";

export interface ModelSpec {
  kind: ModelKind;
  name: string;
  /** Flux 系: [T5, CLIP-L] の順。SD 系は空。 */
  clip: string[];
  /** null = チェックポイント内蔵(SD 系のみ)。Flux 系は必須。 */
  vae: string | null;
}

/** docs/03-data-model.md の params_json と同一。 */
export interface GenerationParams {
  schemaVersion: number;
  model: ModelSpec;
  prompt: string;
  negativePrompt: string;
  width: number;
  height: number;
  steps: number;
  cfg: number;
  guidance: number;
  sampler: string;
  scheduler: string;
  seed: number;
  seedRandomize: boolean;
  batchSize: number;
}

export type JobStatus = "queued" | "running" | "completed" | "failed" | "cancelled";

export interface Job {
  id: string;
  status: JobStatus;
  position: number;
  params: GenerationParams;
  error?: { code: string; message: string };
  imageIds: string[];
  createdAt: number;
  startedAt?: number;
  finishedAt?: number;
}

export interface ImageItem {
  id: string;
  jobId: string;
  width: number;
  height: number;
  seed: number;
  isFavorite: boolean;
  createdAt: number;
  /** 絶対パス。表示には `convertFileSrc` で URL に変換する。 */
  filePath: string;
  thumbPath: string;
}

export interface JobProgress {
  jobId: string;
  step: number;
  totalSteps: number;
  node?: string;
}

export interface Page<T> {
  items: T[];
  nextCursor?: string;
}

export const MAX_ENQUEUE_COUNT = 20;

export const jobEnqueue = (params: GenerationParams, count = 1) =>
  invoke<Job[]>("job_enqueue", { params, count });

export const jobList = (opts: { statuses?: JobStatus[]; cursor?: string; limit?: number } = {}) =>
  invoke<Page<Job>>("job_list", opts);

export const jobCancel = (id: string) => invoke<void>("job_cancel", { id });

export const jobReorder = (id: string, newPosition: number) =>
  invoke<void>("job_reorder", { id, newPosition });

export const jobRetry = (id: string) => invoke<Job>("job_retry", { id });

export const jobClearFinished = () => invoke<number>("job_clear_finished");
