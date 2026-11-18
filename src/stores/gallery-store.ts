import { create } from "zustand";

import { type AppError, toAppError } from "@/lib/tauri/errors";
import { imageDelete, imageList, imageSetFavorite } from "@/lib/tauri/images";
import type { ImageItem } from "@/lib/tauri/jobs";

const PAGE_SIZE = 60;

interface GalleryState {
  items: ImageItem[];
  nextCursor: string | null;
  loading: boolean;
  loaded: boolean;
  error: AppError | null;
  query: string;
  favoritesOnly: boolean;
  selected: Set<string>;

  /** 先頭から読み込み直す(フィルタ変更時・初回)。 */
  reload: () => Promise<void>;
  loadMore: () => Promise<void>;
  setQuery: (query: string) => void;
  setFavoritesOnly: (value: boolean) => void;

  applyCreated: (image: ImageItem) => void;
  applyDeleted: (ids: string[]) => void;

  toggleSelect: (id: string) => void;
  clearSelection: () => void;
  selectAll: () => void;

  /** 失敗時は `AppError` を throw する(呼び出し側で表示)。 */
  setFavorite: (id: string, value: boolean) => Promise<void>;
  setFavoriteMany: (ids: string[], value: boolean) => Promise<void>;
  remove: (ids: string[], deleteFiles: boolean) => Promise<void>;
}

// 古いリクエストの応答が、新しい絞り込みの結果を上書きしないための世代番号
let generation = 0;

export const useGalleryStore = create<GalleryState>((set, get) => ({
  items: [],
  nextCursor: null,
  loading: false,
  loaded: false,
  error: null,
  query: "",
  favoritesOnly: false,
  selected: new Set(),

  reload: async () => {
    const gen = ++generation;
    const { query, favoritesOnly } = get();
    set({ loading: true, error: null });
    try {
      const page = await imageList({ query, favoritesOnly, limit: PAGE_SIZE });
      if (gen !== generation) return;
      set({ items: page.items, nextCursor: page.nextCursor ?? null, loading: false, loaded: true, selected: new Set() });
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

  // 絞り込み中は、新しい画像が条件に合うか判定できないため追加しない(解除時の再読み込みで現れる)
  applyCreated: (image) =>
    set((s) => {
      if (s.query.trim() !== "" || s.favoritesOnly) return s;
      if (s.items.some((i) => i.id === image.id)) return s;
      return { items: [image, ...s.items] };
    }),

  applyDeleted: (ids) =>
    set((s) => {
      const gone = new Set(ids);
      const selected = new Set([...s.selected].filter((id) => !gone.has(id)));
      return { items: s.items.filter((i) => !gone.has(i.id)), selected };
    }),

  toggleSelect: (id) =>
    set((s) => {
      const selected = new Set(s.selected);
      if (!selected.delete(id)) selected.add(id);
      return { selected };
    }),
  clearSelection: () => set({ selected: new Set() }),
  selectAll: () => set((s) => ({ selected: new Set(s.items.map((i) => i.id)) })),

  setFavorite: async (id, value) => {
    await imageSetFavorite(id, value);
    set((s) => ({
      // お気に入り絞り込み中に解除したら一覧から外す
      items: s.favoritesOnly && !value ? s.items.filter((i) => i.id !== id) : s.items.map((i) => (i.id === id ? { ...i, isFavorite: value } : i)),
    }));
  },

  setFavoriteMany: async (ids, value) => {
    await Promise.all(ids.map((id) => imageSetFavorite(id, value)));
    set((s) => {
      const target = new Set(ids);
      return {
        items:
          s.favoritesOnly && !value
            ? s.items.filter((i) => !target.has(i.id))
            : s.items.map((i) => (target.has(i.id) ? { ...i, isFavorite: value } : i)),
      };
    });
  },

  remove: async (ids, deleteFiles) => {
    try {
      await imageDelete(ids, deleteFiles);
    } finally {
      // 記録は削除されているのにファイル移動だけ失敗した場合も、一覧は実状に合わせる
      get().applyDeleted(ids);
    }
  },
}));
