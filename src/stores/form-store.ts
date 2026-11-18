import { create } from "zustand";

import {
  autofill,
  defaultControlnet,
  defaultControlnetUnit,
  defaultFaceDetailer,
  defaultParams,
  pickControlnetModel,
  restoreParams,
  supportsControlnet,
  switchKind,
} from "@/lib/generation/defaults";
import { applyChanges, usableChanges } from "@/lib/generation/suggestions";
import type { SuggestedChanges } from "@/lib/tauri/assistant";
import type { EngineModels } from "@/lib/tauri/engine";
import type {
  ControlnetUnit,
  FaceDetailerParams,
  GenerationParams,
  ModelKind,
  ModelSpec,
} from "@/lib/tauri/jobs";

interface FormState {
  params: GenerationParams;
  /** 同じ設定で連続して生成するジョブ数(「枚数」)。下書きには保存しない。 */
  count: number;
  hydrated: boolean;
  /** 直近に適用した AI の提案を元に戻すための値(適用した項目の元の値だけ)。 */
  suggestionUndo: SuggestedChanges | null;

  /** 保存済みの下書きから復元する(初回のみ)。 */
  hydrate: (saved: Partial<GenerationParams> | null) => void;
  patch: (patch: Partial<Omit<GenerationParams, "model">>) => void;
  patchModel: (patch: Partial<ModelSpec>) => void;
  /** 顔補正の設定を部分更新する(未設定なら既定値から作る)。 */
  patchFace: (patch: Partial<FaceDetailerParams>) => void;
  setControlnetEnabled: (enabled: boolean) => void;
  /** ControlNet のユニットを部分更新する(未設定なら既定値から作る)。前処理を変えたときは、合うモデルへ切り替える。 */
  patchControlnetUnit: (patch: Partial<ControlnetUnit>, controlnets?: string[]) => void;
  /** 参照画像を設定して ControlNet を有効にする(非対応の種別では無効のまま。有効にできたかを返す)。 */
  setReferenceImage: (path: string, controlnets?: string[]) => boolean;
  setKind: (kind: ModelKind) => void;
  setCount: (count: number) => void;
  /** ギャラリーの「この設定で生成」: 画像のパラメータをフォームに読み込む(シードは固定して再現する)。 */
  loadParams: (params: Partial<GenerationParams>) => void;
  /** プリセットの適用: 保存された設定をそのまま復元する(シードのランダム設定も含む)。 */
  applyPreset: (params: Partial<GenerationParams>) => void;
  /** AI の提案を適用する(適用できた項目があれば true)。 */
  applySuggestion: (changes: SuggestedChanges, models: EngineModels | null) => boolean;
  /** 直近の提案の適用を、その項目だけ元に戻す。 */
  undoSuggestion: () => void;
  /** エンジンのモデル一覧が取れた(変わった)ときに、未選択・不在の項目を埋める。 */
  applyModels: (models: EngineModels | null) => void;
}

export const useFormStore = create<FormState>((set, get) => ({
  params: defaultParams("sdxl"),
  count: 1,
  hydrated: false,
  suggestionUndo: null,

  hydrate: (saved) => {
    if (get().hydrated) return;
    set({ params: restoreParams(saved), hydrated: true });
  },

  patch: (patch) => set((s) => ({ params: { ...s.params, ...patch } })),

  patchModel: (patch) => set((s) => ({ params: { ...s.params, model: { ...s.params.model, ...patch } } })),

  patchFace: (patch) =>
    set((s) => ({
      params: { ...s.params, faceDetailer: { ...(s.params.faceDetailer ?? defaultFaceDetailer()), ...patch } },
    })),

  setControlnetEnabled: (enabled) =>
    set((s) => ({
      params: { ...s.params, controlnet: { ...(s.params.controlnet ?? defaultControlnet()), enabled } },
    })),

  patchControlnetUnit: (patch, controlnets = []) =>
    set((s) => {
      const cn = s.params.controlnet ?? defaultControlnet();
      const unit = { ...(cn.units[0] ?? defaultControlnetUnit()), ...patch };
      if (patch.preprocessor !== undefined || unit.model === "") {
        unit.model = pickControlnetModel(controlnets, unit.preprocessor, unit.model);
      }
      return { params: { ...s.params, controlnet: { ...cn, units: [unit] } } };
    }),

  setReferenceImage: (path, controlnets = []) => {
    get().patchControlnetUnit({ image: path }, controlnets);
    const enabled = supportsControlnet(get().params.model.kind);
    get().setControlnetEnabled(enabled);
    return enabled;
  },

  setKind: (kind) => set((s) => ({ params: switchKind(s.params, kind) })),

  setCount: (count) => set({ count }),

  applyPreset: (params) => set({ params: restoreParams(params), hydrated: true }),

  loadParams: (params) =>
    set({ params: { ...restoreParams(params), seedRandomize: false }, hydrated: true }),

  applySuggestion: (changes, models) => {
    const params = get().params;
    const usable = usableChanges(params, changes, models);
    if (Object.keys(usable).length === 0) return false;
    const { next, inverse } = applyChanges(params, usable);
    set({ params: autofill(next, models), suggestionUndo: inverse });
    return true;
  },

  undoSuggestion: () => {
    const undo = get().suggestionUndo;
    if (!undo) return;
    set((s) => ({ params: applyChanges(s.params, undo).next, suggestionUndo: null }));
  },

  applyModels: (models) =>
    set((s) => {
      const next = autofill(s.params, models);
      return next === s.params ? s : { params: next };
    }),
}));
