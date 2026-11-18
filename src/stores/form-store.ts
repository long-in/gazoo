import { create } from "zustand";

import {
  autofill,
  defaultParams,
  restoreParams,
  switchKind,
} from "@/lib/generation/defaults";
import type { EngineModels } from "@/lib/tauri/engine";
import type { GenerationParams, ModelKind, ModelSpec } from "@/lib/tauri/jobs";

interface FormState {
  params: GenerationParams;
  /** 同じ設定で連続して生成するジョブ数(「枚数」)。下書きには保存しない。 */
  count: number;
  hydrated: boolean;

  /** 保存済みの下書きから復元する(初回のみ)。 */
  hydrate: (saved: Partial<GenerationParams> | null) => void;
  patch: (patch: Partial<Omit<GenerationParams, "model">>) => void;
  patchModel: (patch: Partial<ModelSpec>) => void;
  setKind: (kind: ModelKind) => void;
  setCount: (count: number) => void;
  /** ギャラリーの「この設定で生成」: 画像のパラメータをフォームに読み込む(シードは固定して再現する)。 */
  loadParams: (params: Partial<GenerationParams>) => void;
  /** プリセットの適用: 保存された設定をそのまま復元する(シードのランダム設定も含む)。 */
  applyPreset: (params: Partial<GenerationParams>) => void;
  /** エンジンのモデル一覧が取れた(変わった)ときに、未選択・不在の項目を埋める。 */
  applyModels: (models: EngineModels | null) => void;
}

export const useFormStore = create<FormState>((set, get) => ({
  params: defaultParams("sdxl"),
  count: 1,
  hydrated: false,

  hydrate: (saved) => {
    if (get().hydrated) return;
    set({ params: restoreParams(saved), hydrated: true });
  },

  patch: (patch) => set((s) => ({ params: { ...s.params, ...patch } })),

  patchModel: (patch) => set((s) => ({ params: { ...s.params, model: { ...s.params.model, ...patch } } })),

  setKind: (kind) => set((s) => ({ params: switchKind(s.params, kind) })),

  setCount: (count) => set({ count }),

  applyPreset: (params) => set({ params: restoreParams(params), hydrated: true }),

  loadParams: (params) =>
    set({ params: { ...restoreParams(params), seedRandomize: false }, hydrated: true }),

  applyModels: (models) =>
    set((s) => {
      const next = autofill(s.params, models);
      return next === s.params ? s : { params: next };
    }),
}));
