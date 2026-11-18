import type { EngineModels } from "@/lib/tauri/engine";
import type { GenerationParams, ModelKind } from "@/lib/tauri/jobs";

export const MODEL_KINDS: ModelKind[] = ["sd15", "sdxl", "flux", "flux_gguf"];

export const MAX_SEED = Number.MAX_SAFE_INTEGER; // 2^53 - 1(Rust 側の上限と同じ)

export const isFlux = (kind: ModelKind) => kind === "flux" || kind === "flux_gguf";

/** モデル種別ごとの推奨値(docs/05-ui-design.md §3)。 */
export function defaultParams(kind: ModelKind): GenerationParams {
  const base = {
    schemaVersion: 1,
    model: { kind, name: "", clip: [] as string[], vae: null as string | null },
    prompt: "",
    negativePrompt: "",
    seed: 0,
    seedRandomize: true,
    batchSize: 1,
  };
  switch (kind) {
    case "sd15":
      return { ...base, width: 512, height: 512, steps: 20, cfg: 7, guidance: 3.5, sampler: "euler", scheduler: "normal" };
    case "sdxl":
      return { ...base, width: 1024, height: 1024, steps: 25, cfg: 6.5, guidance: 3.5, sampler: "dpmpp_2m", scheduler: "karras" };
    case "flux":
    case "flux_gguf":
      return { ...base, width: 1024, height: 1024, steps: 20, cfg: 1, guidance: 3.5, sampler: "euler", scheduler: "simple" };
  }
}

/** 種別を切り替える。プロンプトとシード設定は引き継ぎ、サイズ・ステップ・サンプラー等は種別の既定に戻す。 */
export function switchKind(current: GenerationParams, kind: ModelKind): GenerationParams {
  if (current.model.kind === kind) return current;
  return {
    ...defaultParams(kind),
    prompt: current.prompt,
    negativePrompt: current.negativePrompt,
    seed: current.seed,
    seedRandomize: current.seedRandomize,
    batchSize: current.batchSize,
  };
}

/** 種別に対応するモデル一覧。 */
export function modelsFor(kind: ModelKind, models: EngineModels | null): string[] {
  if (!models) return [];
  switch (kind) {
    case "sd15":
    case "sdxl":
      return models.checkpoints;
    case "flux":
      return models.unets;
    case "flux_gguf":
      return models.ggufUnets;
  }
}

const first = (list: string[], pattern?: RegExp) =>
  (pattern ? list.find((n) => pattern.test(n)) : undefined) ?? list[0] ?? "";

/**
 * エンジンのモデル一覧に合わせて、未選択・存在しないモデルを埋める。
 * 既に有効な選択はそのまま保つ。一覧が未取得(null)なら何も変えない。
 */
export function autofill(params: GenerationParams, models: EngineModels | null): GenerationParams {
  if (!models) return params;
  const { kind } = params.model;
  const list = modelsFor(kind, models);
  const name = list.includes(params.model.name) ? params.model.name : (list[0] ?? "");

  let clip = params.model.clip;
  let vae = params.model.vae;
  if (isFlux(kind)) {
    const t5 = models.clips.includes(clip[0]) ? clip[0] : first(models.clips, /t5/i);
    const clipL = models.clips.includes(clip[1]) ? clip[1] : first(models.clips, /clip_l|clip-l/i);
    clip = [t5, clipL];
    vae = vae && models.vaes.includes(vae) ? vae : first(models.vaes, /^ae\b|ae\.|flux/i) || null;
  } else {
    clip = [];
    // SD 系の VAE は任意(null = チェックポイント内蔵)。存在しない指定だけ外す。
    vae = vae && models.vaes.includes(vae) ? vae : null;
  }

  const sampler = models.samplers.includes(params.sampler) ? params.sampler : (models.samplers[0] ?? params.sampler);
  const scheduler = models.schedulers.includes(params.scheduler)
    ? params.scheduler
    : (models.schedulers[0] ?? params.scheduler);

  const next = { ...params, model: { ...params.model, name, clip, vae }, sampler, scheduler };
  return JSON.stringify(next) === JSON.stringify(params) ? params : next;
}

/** 保存済みの下書き(古い形式・欠損を含みうる)を、種別の既定値の上にマージして復元する。 */
export function restoreParams(saved: Partial<GenerationParams> | null | undefined): GenerationParams {
  const kind = saved?.model?.kind;
  const base = defaultParams(kind && MODEL_KINDS.includes(kind) ? kind : "sdxl");
  if (!saved) return base;
  return {
    ...base,
    ...saved,
    model: { ...base.model, ...saved.model, clip: saved.model?.clip ?? [], vae: saved.model?.vae ?? null },
  };
}

export function randomSeed(): number {
  // 乱数は 2^53 未満の整数(Rust 側と同じ範囲)
  const buf = new Uint32Array(2);
  crypto.getRandomValues(buf);
  return (buf[0] % 0x200000) * 0x100000000 + buf[1];
}

/** サイズの定番プリセット(64 の倍数)。 */
export function sizePresets(kind: ModelKind): { w: number; h: number }[] {
  return kind === "sd15"
    ? [{ w: 512, h: 512 }, { w: 512, h: 768 }, { w: 768, h: 512 }, { w: 768, h: 768 }]
    : [{ w: 1024, h: 1024 }, { w: 832, h: 1216 }, { w: 1216, h: 832 }, { w: 768, h: 768 }];
}
