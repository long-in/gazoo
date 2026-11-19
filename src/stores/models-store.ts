import { create } from "zustand";

import { type BaseFilter, isActiveStatus } from "@/lib/models/catalog";
import { type AppError, toAppError } from "@/lib/tauri/errors";
import {
  type CatalogView,
  type DownloadState,
  type ModelCategory,
  modelsCatalog,
  modelsDownloadCancel,
  modelsDownloadList,
  modelsDownloadStart,
  modelsSetDir,
  modelsVerify,
} from "@/lib/tauri/models";
import { useEngineStore } from "@/stores/engine-store";

interface ModelsState {
  view: CatalogView | null;
  loading: boolean;
  error: AppError | null;
  downloads: DownloadState[];
  category: ModelCategory;
  base: BaseFilter;
  /** ユーザーが選んだ variant(ファミリー ID → variant ID)。 */
  chosen: Record<string, string>;

  load: () => Promise<void>;
  loadDownloads: () => Promise<void>;
  /** `models://progress` / `models://done` の反映。完了したら導入状況とモデル一覧を取り直す。 */
  applyDownload: (state: DownloadState) => void;
  setCategory: (category: ModelCategory) => void;
  setBase: (base: BaseFilter) => void;
  choose: (familyId: string, variantId: string) => void;
  /** 失敗時は `AppError` を throw する(同意なし `MODELS_NO_CONSENT` など)。 */
  start: (familyId: string, variantId: string, consent?: boolean) => Promise<void>;
  cancel: (downloadId: string, discard?: boolean) => Promise<void>;
  setDir: (path: string | null) => Promise<void>;
  verify: (familyId: string, variantId: string) => Promise<boolean>;
}

export const useModelsStore = create<ModelsState>((set, get) => ({
  view: null,
  loading: false,
  error: null,
  downloads: [],
  category: "checkpoint",
  base: "all",
  chosen: {},

  load: async () => {
    set({ loading: true, error: null });
    try {
      set({ view: await modelsCatalog(), loading: false });
    } catch (e) {
      set({ error: toAppError(e), loading: false });
    }
  },

  loadDownloads: async () => {
    try {
      const list = await modelsDownloadList();
      // 起動直後に届いたイベントの方が新しい可能性があるため、一覧は ID ごとに古い方を置き換えない
      set((s) => {
        const byId = new Map(list.map((d) => [d.downloadId, d]));
        for (const d of s.downloads) byId.set(d.downloadId, d);
        return { downloads: [...byId.values()] };
      });
    } catch {
      /* 一覧の取得失敗は、次のイベントで回復する */
    }
  },

  applyDownload: (state) => {
    const prev = get().downloads.find((d) => d.downloadId === state.downloadId);
    set((s) => {
      // 同じモデルの過去の結果(失敗・取消など、終了済み)は、新しいダウンロードで置き換わる
      const kept = s.downloads.filter(
        (d) =>
          d.downloadId === state.downloadId ||
          !(d.familyId === state.familyId && d.variantId === state.variantId && !isActiveStatus(d.status)),
      );
      const exists = kept.some((d) => d.downloadId === state.downloadId);
      return {
        downloads: exists
          ? kept.map((d) => (d.downloadId === state.downloadId ? state : d))
          : [...kept, state],
      };
    });
    if (state.status === "done" && prev?.status !== "done") {
      void get().load();
      void useEngineStore.getState().loadModels();
    }
  },

  setCategory: (category) => set({ category }),
  setBase: (base) => set({ base }),
  choose: (familyId, variantId) => set((s) => ({ chosen: { ...s.chosen, [familyId]: variantId } })),

  start: async (familyId, variantId, consent) => {
    await modelsDownloadStart(familyId, variantId, consent);
    await get().loadDownloads();
  },

  cancel: async (downloadId, discard) => {
    await modelsDownloadCancel(downloadId, discard);
  },

  setDir: async (path) => {
    await modelsSetDir(path);
    await get().load();
  },

  verify: async (familyId, variantId) => (await modelsVerify(familyId, variantId)).ok,
}));
