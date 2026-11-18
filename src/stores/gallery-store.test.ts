import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ImageItem, Page } from "@/lib/tauri/jobs";

const imageList = vi.fn<(opts: Record<string, unknown>) => Promise<Page<ImageItem>>>();
const imageSetFavorite = vi.fn<(id: string, value: boolean) => Promise<void>>();
const imageDelete = vi.fn<(ids: string[], deleteFiles: boolean) => Promise<void>>();
vi.mock("@/lib/tauri/images", () => ({
  imageList: (o: Record<string, unknown>) => imageList(o),
  imageSetFavorite: (id: string, v: boolean) => imageSetFavorite(id, v),
  imageDelete: (ids: string[], d: boolean) => imageDelete(ids, d),
}));

import { useGalleryStore } from "./gallery-store";

const img = (id: string, over: Partial<ImageItem> = {}): ImageItem => ({
  id,
  jobId: "j",
  width: 1,
  height: 1,
  seed: 1,
  isFavorite: false,
  createdAt: 0,
  filePath: `/x/${id}.png`,
  thumbPath: `/x/${id}.jpg`,
  ...over,
});

const page = (ids: string[], nextCursor?: string): Page<ImageItem> => ({ items: ids.map((i) => img(i)), nextCursor });

beforeEach(() => {
  imageList.mockReset();
  imageSetFavorite.mockReset().mockResolvedValue();
  imageDelete.mockReset().mockResolvedValue();
  useGalleryStore.setState({
    items: [], nextCursor: null, loading: false, loaded: false, error: null,
    query: "", favoritesOnly: false, selected: new Set(),
  });
});

describe("gallery-store paging", () => {
  it("reload は先頭ページを読み、loadMore は続きを重複なしで追加する", async () => {
    imageList.mockResolvedValueOnce(page(["a", "b"], "c1")).mockResolvedValueOnce(page(["b", "c"]));
    await useGalleryStore.getState().reload();
    expect(useGalleryStore.getState().nextCursor).toBe("c1");
    await useGalleryStore.getState().loadMore();
    const s = useGalleryStore.getState();
    expect(s.items.map((i) => i.id)).toEqual(["a", "b", "c"]);
    expect(s.nextCursor).toBeNull();
    expect(imageList).toHaveBeenLastCalledWith(expect.objectContaining({ cursor: "c1" }));
  });

  it("続きが無い・読み込み中は loadMore しない", async () => {
    await useGalleryStore.getState().loadMore();
    useGalleryStore.setState({ nextCursor: "c", loading: true });
    await useGalleryStore.getState().loadMore();
    expect(imageList).not.toHaveBeenCalled();
  });

  it("絞り込み変更後は、古い応答が新しい結果を上書きしない", async () => {
    let resolveOld: (p: Page<ImageItem>) => void = () => {};
    imageList.mockImplementationOnce(() => new Promise((r) => (resolveOld = r)));
    imageList.mockResolvedValueOnce(page(["new"]));

    const old = useGalleryStore.getState().reload();
    useGalleryStore.getState().setQuery("cat");
    await useGalleryStore.getState().reload();
    resolveOld(page(["stale"]));
    await old;

    expect(useGalleryStore.getState().items.map((i) => i.id)).toEqual(["new"]);
  });

  it("読み込み失敗はエラーとして保持する", async () => {
    imageList.mockRejectedValueOnce({ code: "DB_ERROR", message: "x" });
    await useGalleryStore.getState().reload();
    const s = useGalleryStore.getState();
    expect(s.error?.code).toBe("DB_ERROR");
    expect(s.loading).toBe(false);
  });
});

describe("gallery-store events and actions", () => {
  it("新しい画像は先頭に追加。絞り込み中・重複は追加しない", () => {
    useGalleryStore.setState({ items: [img("a")] });
    useGalleryStore.getState().applyCreated(img("b"));
    useGalleryStore.getState().applyCreated(img("b"));
    expect(useGalleryStore.getState().items.map((i) => i.id)).toEqual(["b", "a"]);

    useGalleryStore.setState({ query: "cat" });
    useGalleryStore.getState().applyCreated(img("c"));
    expect(useGalleryStore.getState().items).toHaveLength(2);
    useGalleryStore.setState({ query: "", favoritesOnly: true });
    useGalleryStore.getState().applyCreated(img("d"));
    expect(useGalleryStore.getState().items).toHaveLength(2);
  });

  it("削除イベントで一覧と選択から外れる", () => {
    useGalleryStore.setState({ items: [img("a"), img("b")], selected: new Set(["a", "b"]) });
    useGalleryStore.getState().applyDeleted(["a"]);
    const s = useGalleryStore.getState();
    expect(s.items.map((i) => i.id)).toEqual(["b"]);
    expect([...s.selected]).toEqual(["b"]);
  });

  it("選択の切り替え・全選択・解除", () => {
    useGalleryStore.setState({ items: [img("a"), img("b")] });
    const s = useGalleryStore.getState();
    s.toggleSelect("a");
    expect([...useGalleryStore.getState().selected]).toEqual(["a"]);
    s.toggleSelect("a");
    expect(useGalleryStore.getState().selected.size).toBe(0);
    s.selectAll();
    expect(useGalleryStore.getState().selected.size).toBe(2);
    s.clearSelection();
    expect(useGalleryStore.getState().selected.size).toBe(0);
  });

  it("お気に入りの切り替え。お気に入り絞り込み中の解除は一覧から外れる", async () => {
    useGalleryStore.setState({ items: [img("a"), img("b")] });
    await useGalleryStore.getState().setFavorite("a", true);
    expect(useGalleryStore.getState().items[0].isFavorite).toBe(true);

    useGalleryStore.setState({ favoritesOnly: true });
    await useGalleryStore.getState().setFavorite("a", false);
    expect(useGalleryStore.getState().items.map((i) => i.id)).toEqual(["b"]);
  });

  it("失敗したお気に入り操作は状態を変えない", async () => {
    imageSetFavorite.mockRejectedValueOnce({ code: "IMAGE_NOT_FOUND", message: "x" });
    useGalleryStore.setState({ items: [img("a")] });
    await expect(useGalleryStore.getState().setFavorite("a", true)).rejects.toMatchObject({ code: "IMAGE_NOT_FOUND" });
    expect(useGalleryStore.getState().items[0].isFavorite).toBe(false);
  });

  it("削除は IPC を呼んで一覧を更新する。ファイル移動に失敗しても記録は消えているので一覧は更新する", async () => {
    useGalleryStore.setState({ items: [img("a"), img("b")] });
    await useGalleryStore.getState().remove(["a"], true);
    expect(imageDelete).toHaveBeenCalledWith(["a"], true);
    expect(useGalleryStore.getState().items.map((i) => i.id)).toEqual(["b"]);

    imageDelete.mockRejectedValueOnce({ code: "STORAGE_ERROR", message: "x" });
    await expect(useGalleryStore.getState().remove(["b"], true)).rejects.toMatchObject({ code: "STORAGE_ERROR" });
    expect(useGalleryStore.getState().items).toHaveLength(0);
  });
});
