import { create } from "zustand";

import { type AppError, toAppError } from "@/lib/tauri/errors";
import { imageList } from "@/lib/tauri/images";
import type { ImageItem } from "@/lib/tauri/jobs";

const PAGE_SIZE = 40;

/**
 * 生成ページのギャラリー帯(docs/27 §3)のストア。`/gallery` のストアとは独立(フィルタを共有しない)。
 * 読み込みは同じ `image_list`。
 */
interface StripState {
  items: ImageItem[];
  nextCursor: string | null;
  loading: boolean;
  loaded: boolean;
  error: AppError | null;
  query: string;
  favoritesOnly: boolean;
  collapsed: boolean;
  /** 選択中の画像(1 枚)。帯に無い画像(キューのサムネイルから選んだ等)でもよい。 */
  selectedId: string | null;

  reload: () => Promise<void>;
  loadMore: () => Promise<void>;
  setQuery: (query: string) => void;
  setFavoritesOnly: (value: boolean) => void;
  setCollapsed: (value: boolean) => void;
  /** 同じ画像をもう一度選ぶと解除する。 */
  toggleSelect: (id: string) => void;
  select: (id: string | null) => void;

  applyCreated: (image: ImageItem) => void;
  applyDeleted: (ids: string[]) => void;
  applyFavorite: (id: string, value: boolean) => void;
}

// 古いリクエストの応答が新しい絞り込みの結果を上書きしないための世代番号
let generation = 0;

export const useStripStore = create<StripState>((set, get) => ({
  items: [],
  nextCursor: null,
  loading: false,
  loaded: false,
  error: null,
  query: "",
  favoritesOnly: false,
  collapsed: false,
  selectedId: null,

  reload: async () => {
    const gen = ++generation;
    const { query, favoritesOnly } = get();
    set({ loading: true, error: null });
    try {
      const page = await imageList({ query, favoritesOnly, limit: PAGE_SIZE });
      if (gen !== generation) return;
      set({ items: page.items, nextCursor: page.nextCursor ?? null, loading: false, loaded: true });
    } catch (e) {
      if (gen !== generation) return;
      set({ loading: false, loaded: true, error: toAppError(e) });
    }
  },

  loadMore: async () => {
    const { nextCursor, loading, query, favoritesOnly } = get();
    if (!nextCursor || loading) return;
    const gen = generation;
    set({ loading: true });
    try {
      const page = await imageList({ query, favoritesOnly, cursor: nextCursor, limit: PAGE_SIZE });
      if (gen !== generation) return;
      set((s) => {
        const seen = new Set(s.items.map((i) => i.id));
        return {
          items: [...s.items, ...page.items.filter((i) => !seen.has(i.id))],
          nextCursor: page.nextCursor ?? null,
          loading: false,
        };
      });
    } catch (e) {
      if (gen !== generation) return;
      set({ loading: false, error: toAppError(e) });
    }
  },

  setQuery: (query) => set({ query }),
  setFavoritesOnly: (favoritesOnly) => set({ favoritesOnly }),
  setCollapsed: (collapsed) => set({ collapsed }),
  toggleSelect: (id) => set((s) => ({ selectedId: s.selectedId === id ? null : id })),
  select: (selectedId) => set({ selectedId }),

  // フィルタ中は、新しい画像が条件に合うか判定できないため追加しない(解除時の再読み込みで現れる)
  applyCreated: (image) =>
    set((s) => {
      if (s.query.trim() !== "" || s.favoritesOnly) return s;
      if (s.items.some((i) => i.id === image.id)) return s;
      return { items: [image, ...s.items] };
    }),

  applyDeleted: (ids) =>
    set((s) => {
      const gone = new Set(ids);
      return {
        items: s.items.filter((i) => !gone.has(i.id)),
        selectedId: s.selectedId !== null && gone.has(s.selectedId) ? null : s.selectedId,
      };
    }),

  applyFavorite: (id, value) =>
    set((s) => ({
      // お気に入りのみ表示中に解除したら一覧から外す
      items:
        s.favoritesOnly && !value ? s.items.filter((i) => i.id !== id) : s.items.map((i) => (i.id === id ? { ...i, isFavorite: value } : i)),
    })),
}));
