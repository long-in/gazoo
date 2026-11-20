import { create } from "zustand";

import { type Density, defaultUiState, parseUiState, type UiState } from "@/lib/ui-state";

import { useSettingsStore } from "./settings-store";

const SAVE_DELAY_MS = 600;
let timer: ReturnType<typeof setTimeout> | null = null;

/** 画面の状態(パネルの開閉・幅・密度)。`settings.uiState` に自動保存する(docs/29 §6)。 */
interface UiStoreState extends UiState {
  hydrated: boolean;
  /** 設定から取り込む(起動時に 1 回)。 */
  hydrate: (raw: unknown) => void;
  setDensity: (density: Density) => void;
  setPanelOpen: (id: string, open: boolean) => void;
  setWidth: (id: string, width: number) => void;
}

function scheduleSave(get: () => UiStoreState) {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    const { density, panels, widths } = get();
    // 保存に失敗しても画面は動く(次の変更でやり直す)
    void useSettingsStore.getState().update({ uiState: { density, panels, widths } }).catch(() => {});
  }, SAVE_DELAY_MS);
}

export const useUiStore = create<UiStoreState>((set, get) => ({
  ...defaultUiState,
  hydrated: false,
  hydrate: (raw) => set({ ...parseUiState(raw), hydrated: true }),
  setDensity: (density) => {
    set({ density });
    scheduleSave(get);
  },
  setPanelOpen: (id, open) => {
    set((s) => ({ panels: { ...s.panels, [id]: open } }));
    scheduleSave(get);
  },
  setWidth: (id, width) => {
    set((s) => ({ widths: { ...s.widths, [id]: width } }));
    scheduleSave(get);
  },
}));
