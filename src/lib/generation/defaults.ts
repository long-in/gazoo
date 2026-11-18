import type { EngineModels } from "@/lib/tauri/engine";
import type {
  ControlnetParams,
  ControlnetUnit,
  FaceDetailerParams,
  GenerationParams,
  ModelKind,
  Preprocessor,
} from "@/lib/tauri/jobs";

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

/** 顔補正の既定値(docs/10 §4.2)。無効の状態で始める。 */
export function defaultFaceDetailer(): FaceDetailerParams {
  return { enabled: false, detector: "", denoise: 0.4, prompt: "", confidence: 0.5 };
}

/** 顔補正に対応するモデル種別(SD 1.5 / SDXL)。 */
export const supportsFaceDetailer = (kind: ModelKind) => !isFlux(kind);

/** ControlNet の既定値(docs/10 §5.2)。参照画像とモデルは未選択。 */
export function defaultControlnetUnit(): ControlnetUnit {
  return { image: "", model: "", preprocessor: "canny", strength: 1, startPercent: 0, endPercent: 1 };
}

export function defaultControlnet(): ControlnetParams {
  return { enabled: false, units: [] };
}

/** ControlNet に対応するモデル種別(SD 1.5 / SDXL)。 */
export const supportsControlnet = (kind: ModelKind) => !isFlux(kind);

const MODEL_HINT: Record<Preprocessor, RegExp | undefined> = {
  canny: /canny/i,
  depth: /depth/i,
  openpose: /pose/i,
  none: undefined,
};

/**
 * 前処理に合う ControlNet モデルを選ぶ。現在のモデルが合うならそのまま、合わなければ名前に前処理名を含む
 * 最初のもの、それも無ければ現在のモデル(一覧にあれば)、最後に一覧の先頭。一覧が空なら現在値のまま。
 */
export function pickControlnetModel(controlnets: string[], preprocessor: Preprocessor, current: string): string {
  if (controlnets.length === 0) return current;
  const hint = MODEL_HINT[preprocessor];
  if (hint?.test(current) && controlnets.includes(current)) return current;
  const match = hint && controlnets.find((n) => hint.test(n));
  if (match) return match;
  return controlnets.includes(current) ? current : controlnets[0];
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
    // 顔補正の設定は引き継ぐ。非対応の種別(Flux)へ切り替えるときは無効にする(値は残す)
    ...(current.faceDetailer && {
      faceDetailer: { ...current.faceDetailer, enabled: current.faceDetailer.enabled && supportsFaceDetailer(kind) },
    }),
    // ControlNet も同様(参照画像などの設定は残し、非対応の種別では無効にする)
    ...(current.controlnet && {
      controlnet: { ...current.controlnet, enabled: current.controlnet.enabled && supportsControlnet(kind) },
    }),
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

  // 顔補正の検出モデル: 未選択・存在しないものは、顔用(face)を優先して一覧から選ぶ。
  // 一覧が空(ノード未導入)なら触らない(設定は残し、利用可否は画面側で判断する)
  let faceDetailer = params.faceDetailer;
  if (faceDetailer && models.faceDetectors.length > 0 && !models.faceDetectors.includes(faceDetailer.detector)) {
    faceDetailer = { ...faceDetailer, detector: first(models.faceDetectors, /face/i) };
  }

  // ControlNet のモデル: 前処理に合うものを選ぶ。有効な選択は保つ(一覧が空なら触らない)
  let controlnet = params.controlnet;
  const unit = controlnet?.units[0];
  if (controlnet && unit) {
    const model = pickControlnetModel(models.controlnets, unit.preprocessor, unit.model);
    if (model !== unit.model) controlnet = { ...controlnet, units: [{ ...unit, model }] };
  }

  const next = {
    ...params,
    model: { ...params.model, name, clip, vae },
    sampler,
    scheduler,
    ...(faceDetailer && { faceDetailer }),
    ...(controlnet && { controlnet }),
  };
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
    // 古い下書き・プリセット(顔補正なし)は無効のまま。ある場合は欠けた項目を既定で埋める
    ...(saved.faceDetailer && { faceDetailer: { ...defaultFaceDetailer(), ...saved.faceDetailer } }),
    ...(saved.controlnet && {
      controlnet: {
        enabled: saved.controlnet.enabled ?? false,
        units: (saved.controlnet.units ?? []).slice(0, 1).map((u) => ({ ...defaultControlnetUnit(), ...u })),
      },
    }),
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
