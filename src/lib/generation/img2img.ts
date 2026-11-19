import type { EngineModels } from "@/lib/tauri/engine";
import type { GenerationParams, Img2imgParams } from "@/lib/tauri/jobs";
import type { ModelCatalog } from "@/lib/tauri/models";

import { isSelectableFamily, recommendedPatch, switchTarget } from "@/lib/models/profile";

import {
  autofill,
  defaultControlnet,
  defaultControlnetUnit,
  pickControlnetModel,
  supportsControlnet,
  switchKind,
} from "./defaults";
import { defaultImg2img } from "./img2img-defaults";
import { sizeForImage } from "./inpaint";

export { defaultImg2img };

/** 強さのボタン(弱・中・強。docs/19 §3)。 */
export const DENOISE_LEVELS = { weak: 0.45, medium: 0.6, strong: 0.75 } as const;
export type DenoiseLevel = keyof typeof DENOISE_LEVELS;

/** 数値に対応するボタン(一致しなければ `null`)。 */
export function levelOf(denoise: number): DenoiseLevel | null {
  const hit = (Object.keys(DENOISE_LEVELS) as DenoiseLevel[]).find((k) => Math.abs(DENOISE_LEVELS[k] - denoise) < 1e-6);
  return hit ?? null;
}

/** 画像から生成を実行できる状態か(元画像がある)。 */
export function img2imgReady(i2i: Img2imgParams | undefined): boolean {
  return i2i?.enabled !== true || i2i.image !== "";
}

/** 構図の固定(ControlNet の画像が元画像と同じで、有効)になっているか。 */
export function isCompositionLocked(params: GenerationParams): boolean {
  const image = params.img2img?.image ?? "";
  const cn = params.controlnet;
  return image !== "" && cn?.enabled === true && cn.units[0]?.image === image;
}

/** 構図の固定の既定(docs/19 §4): 輪郭(Canny)・強さ 0.8・終了 0.8。 */
const LOCK_UNIT = { preprocessor: "canny", strength: 0.8, startPercent: 0, endPercent: 0.8 } as const;

/** 構図の固定を入れる/切る。入れるときは元画像を ControlNet の参照画像にし、モデルは候補から選ぶ。 */
export function withCompositionLock(params: GenerationParams, on: boolean, controlnets: string[]): GenerationParams {
  const cn = params.controlnet ?? defaultControlnet();
  if (!on) return { ...params, controlnet: { ...cn, enabled: false } };
  const base = { ...(cn.units[0] ?? defaultControlnetUnit()), ...LOCK_UNIT, image: params.img2img?.image ?? "" };
  const unit = { ...base, model: pickControlnetModel(controlnets, base.preprocessor, base.model) };
  return { ...params, controlnet: { enabled: true, units: [unit] } };
}

/** 元画像を設定する。生成サイズを画像に合わせ、構図の固定中なら ControlNet の画像も揃える。 */
export function withImg2imgImage(params: GenerationParams, path: string, width: number, height: number): GenerationParams {
  const size = sizeForImage(width, height);
  const locked = isCompositionLocked(params);
  const next: GenerationParams = {
    ...params,
    width: size.width,
    height: size.height,
    img2img: { ...(params.img2img ?? defaultImg2img()), image: path },
  };
  if (!locked || !next.controlnet) return next;
  return { ...next, controlnet: { ...next.controlnet, units: [{ ...next.controlnet.units[0], image: path }] } };
}

/** 画像から生成を入れる/切る。入れるときは、同時に使えない Inpaint を切る。 */
export function withImg2imgEnabled(params: GenerationParams, enabled: boolean): GenerationParams {
  return {
    ...params,
    img2img: { ...(params.img2img ?? defaultImg2img()), enabled },
    ...(enabled && params.inpaint && { inpaint: { ...params.inpaint, enabled: false } }),
  };
}

/** 導入済みのアニメ系 SDXL(カタログ順。推奨の variant を優先して導入済みのもの)。無ければ `null`。 */
export function findAnimeTarget(catalog: ModelCatalog | null | undefined, models: EngineModels | null) {
  if (!catalog || !models) return null;
  for (const family of catalog.families) {
    if (!isSelectableFamily(family) || !family.tags.includes("anime") || !family.profile) continue;
    const target = switchTarget(family, models);
    if (target?.kind === "sdxl") return { family, ...target };
  }
  return null;
}

export type AnimeStartOutcome = "ok" | "noAnimeModel";

/**
 * 「画像からアニメ調に」(docs/19 §3.1)。元画像・アニメ系 SDXL・推奨値・強さ 0.6 を一度に設定する。
 * 構図の固定は、ControlNet の Canny とモデルが使えるときだけ入れる。アニメ系モデルが無ければ何も変えない。
 */
export function animeStart(
  current: GenerationParams,
  image: { path: string; width: number; height: number },
  ctx: { catalog: ModelCatalog | null | undefined; models: EngineModels | null; cannyAvailable: boolean },
): { params: GenerationParams; outcome: AnimeStartOutcome; compositionLocked: boolean } {
  const target = findAnimeTarget(ctx.catalog, ctx.models);
  if (!target?.family.profile) return { params: current, outcome: "noAnimeModel", compositionLocked: false };

  const switched = switchKind(current, target.kind);
  let next: GenerationParams = {
    ...switched,
    ...recommendedPatch(target.family.profile),
    model: { ...switched.model, name: target.name },
    // 画像と同時に使えない機能は切る
    ...(switched.inpaint && { inpaint: { ...switched.inpaint, enabled: false } }),
  };
  next = autofill(next, ctx.models, ctx.catalog);
  next = { ...next, img2img: { enabled: true, image: "", denoise: DENOISE_LEVELS.medium } };
  next = withImg2imgImage(next, image.path, image.width, image.height);
  const controlnets = ctx.models?.controlnets ?? [];
  const canLock = ctx.cannyAvailable && controlnets.length > 0 && supportsControlnet(next.model.kind);
  if (canLock) next = withCompositionLock(next, true, controlnets);
  return { params: next, outcome: "ok", compositionLocked: canLock };
}
