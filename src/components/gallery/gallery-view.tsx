"use client";

import { Search, Star, X } from "lucide-react";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { toAppError } from "@/lib/tauri/errors";
import { useGalleryStore } from "@/stores/gallery-store";

import { DeleteConfirm } from "./delete-confirm";
import { GalleryGrid } from "./gallery-grid";
import { ImageDetailSheet } from "./image-detail-sheet";

const SEARCH_DEBOUNCE_MS = 300;

export function GalleryView() {
  const t = useT();
  const errorMessage = useErrorMessage();
  const router = useRouter();
  const openId = useSearchParams().get("id");

  const items = useGalleryStore((s) => s.items);
  const loading = useGalleryStore((s) => s.loading);
  const loaded = useGalleryStore((s) => s.loaded);
  const error = useGalleryStore((s) => s.error);
  const hasMore = useGalleryStore((s) => s.nextCursor !== null);
  const favoritesOnly = useGalleryStore((s) => s.favoritesOnly);
  const selected = useGalleryStore((s) => s.selected);
  const { reload, loadMore, setQuery, setFavoritesOnly, toggleSelect, clearSelection, selectAll, setFavoriteMany, remove } =
    useGalleryStore.getState();

  const [text, setText] = useState(useGalleryStore.getState().query);
  const [confirmBulkDelete, setConfirmBulkDelete] = useState(false);

  // 検索語・お気に入りフィルタの変更を(入力が止まってから)反映して読み込み直す。初回は即時。
  const first = useRef(true);
  useEffect(() => {
    const delay = first.current ? 0 : SEARCH_DEBOUNCE_MS;
    first.current = false;
    const timer = setTimeout(() => {
      setQuery(text);
      void reload();
    }, delay);
    return () => clearTimeout(timer);
  }, [text, favoritesOnly, setQuery, reload]);

  const open = useCallback((id: string) => router.push(`/gallery/?id=${encodeURIComponent(id)}`), [router]);
  const close = useCallback(() => router.replace("/gallery/"), [router]);

  async function bulk(action: () => Promise<unknown>, okKey?: Parameters<typeof t>[0]) {
    try {
      await action();
      if (okKey) toast.success(t(okKey));
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  const ids = [...selected];
  const filtered = text.trim() !== "" || favoritesOnly;

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 p-6">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="mr-2 text-2xl font-semibold">{t("gallery.title")}</h1>
        <div className="relative min-w-56 flex-1 sm:max-w-sm">
          <Search aria-hidden className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input
            type="search"
            className="pl-8"
            value={text}
            placeholder={t("gallery.search")}
            aria-label={t("gallery.search")}
            onChange={(e) => setText(e.target.value)}
          />
        </div>
        <label className="flex items-center gap-2 text-sm">
          <Switch checked={favoritesOnly} onCheckedChange={setFavoritesOnly} />
          <Star aria-hidden className="size-4 text-amber-400" />
          {t("gallery.favoritesOnly")}
        </label>
      </div>

      {selected.size > 0 && (
        <div role="toolbar" className="flex flex-wrap items-center gap-2 rounded-lg border bg-muted/40 px-3 py-2">
          <span className="text-sm font-medium">
            {selected.size} {t("gallery.selected")}
          </span>
          <Button type="button" variant="ghost" size="sm" onClick={selectAll}>
            {t("gallery.selectAll")}
          </Button>
          <Button type="button" variant="outline" size="sm" onClick={() => void bulk(() => setFavoriteMany(ids, true))}>
            <Star />
            {t("gallery.favoriteMany")}
          </Button>
          <Button type="button" variant="outline" size="sm" className="text-destructive" onClick={() => setConfirmBulkDelete(true)}>
            {t("gallery.delete")}
          </Button>
          <Button type="button" variant="ghost" size="icon-sm" className="ml-auto" aria-label={t("gallery.clearSelection")} title={t("gallery.clearSelection")} onClick={clearSelection}>
            <X />
          </Button>
        </div>
      )}

      {error && items.length === 0 ? (
        <p role="alert" className="text-sm text-destructive">
          {t("gallery.loadFailed")}: {errorMessage(error)}
        </p>
      ) : loaded && items.length === 0 ? (
        <p className="rounded-xl border border-dashed p-12 text-center text-sm text-muted-foreground">
          {filtered ? t("gallery.noResults") : t("gallery.empty")}
        </p>
      ) : (
        <GalleryGrid
          items={items}
          hasMore={hasMore}
          loading={loading}
          selected={selected}
          onLoadMore={() => void loadMore()}
          onOpen={open}
          onToggleSelect={toggleSelect}
        />
      )}
      {loading && items.length > 0 && <p className="text-center text-xs text-muted-foreground">{t("gallery.loadingMore")}</p>}

      <ImageDetailSheet id={openId} onClose={close} />

      <DeleteConfirm
        open={confirmBulkDelete}
        count={ids.length}
        onOpenChange={setConfirmBulkDelete}
        onConfirm={() => {
          setConfirmBulkDelete(false);
          void bulk(() => remove(ids, true), "gallery.deleted");
        }}
      />
    </div>
  );
}
