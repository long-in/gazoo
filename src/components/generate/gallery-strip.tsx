"use client";

import { ChevronDown, ChevronUp, Search, Star, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { ImageDetailSheet } from "@/components/gallery/image-detail-sheet";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Switch } from "@/components/ui/switch";
import { useImageActions } from "@/lib/gallery/use-image-as";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { imageSrc } from "@/lib/image-src";
import { toAppError } from "@/lib/tauri/errors";
import { type ImageDetail, imageGet, imageSetFavorite } from "@/lib/tauri/images";
import type { ImageItem } from "@/lib/tauri/jobs";
import { cn } from "@/lib/utils";
import { useStripStore } from "@/stores/strip-store";

const SEARCH_DEBOUNCE_MS = 300;
const detailCache = new Map<string, ImageDetail>();

async function getDetail(id: string): Promise<ImageDetail | null> {
  const hit = detailCache.get(id);
  if (hit) return hit;
  try {
    const d = await imageGet(id);
    detailCache.set(id, d);
    return d;
  } catch {
    return null;
  }
}

/** 生成ページ下部のギャラリー帯(docs/27 §3)。横スクロール。選択した画像の設定・プロンプト・元画像への設定ができる。 */
export function GalleryStrip() {
  const t = useT();
  const errorMessage = useErrorMessage();
  const actions = useImageActions(true);

  const items = useStripStore((s) => s.items);
  const loading = useStripStore((s) => s.loading);
  const loaded = useStripStore((s) => s.loaded);
  const hasMore = useStripStore((s) => s.nextCursor !== null);
  const favoritesOnly = useStripStore((s) => s.favoritesOnly);
  const collapsed = useStripStore((s) => s.collapsed);
  const selectedId = useStripStore((s) => s.selectedId);
  const { reload, setQuery, setFavoritesOnly, setCollapsed, toggleSelect, select } = useStripStore.getState();

  const [text, setText] = useState(useStripStore.getState().query);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [selDetail, setSelDetail] = useState<ImageDetail | null>(null);
  const empty = items.length === 0;
  const scrollRef = useRef<HTMLDivElement>(null);
  const lastFocused = useRef<HTMLElement | null>(null);

  // 検索語・お気に入りの変更を、入力が止まってから反映して読み込み直す(初回は即時)
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

  // 選択中の画像の詳細(プロンプト・設定)
  useEffect(() => {
    let cancelled = false;
    if (!selectedId) return;
    void getDetail(selectedId).then((d) => {
      if (!cancelled) setSelDetail(d);
    });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);
  const detail = selectedId && selDetail?.image.id === selectedId ? selDetail : null;
  const selectedImage = detail ? (items.find((i) => i.id === detail.image.id) ?? detail.image) : null;

  // 選択した画像が帯の中にあれば、見える位置までスクロールする
  useEffect(() => {
    if (!selectedId) return;
    scrollRef.current
      ?.querySelector<HTMLElement>(`[data-image-id="${selectedId}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedId]);

  // 右端に近づいたら続きを読み込む
  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    if (hasMore && !loading && el.scrollWidth - el.scrollLeft - el.clientWidth < 400) void useStripStore.getState().loadMore();
  }, [hasMore, loading]);
  // 縦のホイール回転は横スクロールに変換する
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      el.scrollLeft += e.deltaY;
      e.preventDefault();
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [collapsed, empty]);
  // 画面幅より少ない件数しか読み込めていない場合に続きを取得する
  useEffect(() => {
    onScroll();
  }, [items.length, onScroll]);

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-image-id]");
    const next = e.key === "ArrowRight" ? el?.nextElementSibling : el?.previousElementSibling;
    if (next instanceof HTMLElement) {
      e.preventDefault();
      next.focus();
      select(next.dataset.imageId ?? null);
    }
  }

  async function toggleFavorite(image: ImageItem) {
    try {
      await imageSetFavorite(image.id, !image.isFavorite);
      useStripStore.getState().applyFavorite(image.id, !image.isFavorite);
      setSelDetail((d) => (d ? { ...d, image: { ...d.image, isFavorite: !image.isFavorite } } : d));
      detailCache.delete(image.id);
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  const params = detail?.job.params;
  const filtered = text.trim() !== "" || favoritesOnly;
  const menuItem = "justify-start";

  return (
    <section aria-label={t("strip.title")} className="flex shrink-0 flex-col gap-1.5 border-t bg-muted/20 px-4 py-2">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <h2 className="text-sm font-medium">{t("strip.title")}</h2>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-expanded={!collapsed}
          aria-label={collapsed ? t("strip.expand") : t("strip.collapse")}
          title={collapsed ? t("strip.expand") : t("strip.collapse")}
          onClick={() => setCollapsed(!collapsed)}
        >
          {collapsed ? <ChevronUp /> : <ChevronDown />}
        </Button>
        {!collapsed && (
          <>
            <div className="relative w-44">
              <Search aria-hidden className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                type="search"
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder={t("gallery.search")}
                aria-label={t("gallery.search")}
                className="h-7 pl-7 text-xs"
              />
            </div>
            <label className="flex items-center gap-1.5 text-xs">
              <Switch checked={favoritesOnly} onCheckedChange={setFavoritesOnly} />
              <Star aria-hidden className="size-3.5 text-amber-400" />
              {t("gallery.favoritesOnly")}
            </label>
          </>
        )}
        {!collapsed && selectedImage && params && (
          <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-1.5">
            <span className="min-w-0 max-w-[22rem] flex-1 truncate text-xs text-muted-foreground" title={params.prompt}>
              {t("strip.selected")}: {params.prompt}
            </span>
            <Button type="button" size="xs" onClick={() => actions.loadSettings(selectedImage, params)}>
              {t("strip.loadSettings")}
            </Button>
            <Popover>
              <PopoverTrigger render={<Button type="button" size="xs" variant="outline"/>}>
                  {t("strip.promptOnly")}
                  <ChevronDown />
                </PopoverTrigger>
              <PopoverContent align="end" className="flex w-40 flex-col gap-1 p-2">
                <Button type="button" size="sm" variant="ghost" className={menuItem} onClick={() => actions.loadPrompt(params, "replace")}>
                  {t("strip.promptReplace")}
                </Button>
                <Button type="button" size="sm" variant="ghost" className={menuItem} onClick={() => actions.loadPrompt(params, "append")}>
                  {t("strip.promptAppend")}
                </Button>
              </PopoverContent>
            </Popover>
            <Popover>
              <PopoverTrigger render={<Button type="button" size="xs" variant="outline"/>}>
                  {t("strip.base")}
                  <ChevronDown />
                </PopoverTrigger>
              <PopoverContent align="end" className="flex w-56 flex-col gap-1 p-2">
                <Button type="button" size="sm" variant="ghost" className={menuItem} onClick={() => void actions.setAsImg2img(selectedImage, false)}>
                  {t("gallery.useAsImg2img")}
                </Button>
                <Button type="button" size="sm" variant="ghost" className={menuItem} onClick={() => void actions.setAsImg2img(selectedImage, true)}>
                  {t("gallery.useAsAnime")}
                </Button>
                <Button type="button" size="sm" variant="ghost" className={menuItem} onClick={() => void actions.setAsReference(selectedImage)}>
                  {t("gallery.useAsReference")}
                </Button>
                <Button type="button" size="sm" variant="ghost" className={menuItem} onClick={() => void actions.setAsInpaint(selectedImage)}>
                  {t("gallery.useAsInpaint")}
                </Button>
                <Button type="button" size="sm" variant="ghost" className={menuItem} onClick={() => void actions.setAsQwenReference(selectedImage)}>
                  {t("gallery.useAsQwenReference")}
                </Button>
              </PopoverContent>
            </Popover>
            <Button
              type="button"
              size="icon-xs"
              variant="outline"
              aria-pressed={selectedImage.isFavorite}
              aria-label={selectedImage.isFavorite ? t("gallery.unfavorite") : t("gallery.favorite")}
              title={selectedImage.isFavorite ? t("gallery.unfavorite") : t("gallery.favorite")}
              onClick={() => void toggleFavorite(selectedImage)}
            >
              <Star className={selectedImage.isFavorite ? "fill-amber-400 text-amber-400" : undefined} />
            </Button>
            <Button type="button" size="xs" variant="outline" onClick={() => setDetailId(selectedImage.id)}>
              {t("strip.details")}
            </Button>
            <Button type="button" size="icon-xs" variant="ghost" aria-label={t("strip.deselect")} title={t("strip.deselect")} onClick={() => select(null)}>
              <X />
            </Button>
          </div>
        )}
      </div>

      {!collapsed &&
        (loaded && items.length === 0 ? (
          <p className="flex h-28 items-center justify-center text-xs text-muted-foreground">
            {filtered ? t("strip.noResults") : t("strip.empty")}
          </p>
        ) : (
          <div ref={scrollRef} onScroll={onScroll} onKeyDown={onKeyDown} className="flex gap-2 overflow-x-auto pb-1" data-testid="strip-scroll">
            {items.map((img) => (
              <Thumb
                key={img.id}
                image={img}
                selected={img.id === selectedId}
                onSelect={() => toggleSelect(img.id)}
                onOpen={(el) => {
                  lastFocused.current = el;
                  setDetailId(img.id);
                }}
              />
            ))}
            {loading && <span className="flex w-16 shrink-0 items-center text-xs text-muted-foreground">{t("strip.loading")}</span>}
          </div>
        ))}

      <ImageDetailSheet
        id={detailId}
        stay
        onClose={() => {
          setDetailId(null);
          // 閉じたら帯のサムネイルへフォーカスを戻す
          setTimeout(() => lastFocused.current?.focus(), 0);
        }}
      />
    </section>
  );
}

function Thumb({
  image,
  selected,
  onSelect,
  onOpen,
}: {
  image: ImageItem;
  selected: boolean;
  onSelect: () => void;
  onOpen: (el: HTMLElement) => void;
}) {
  const [tip, setTip] = useState(`${image.width}×${image.height} · seed ${image.seed}`);
  const [label, setLabel] = useState(`seed ${image.seed}`);

  function loadTip() {
    void getDetail(image.id).then((d) => {
      if (!d) return;
      const p = d.job.params;
      const head = p.prompt.length > 120 ? `${p.prompt.slice(0, 120)}…` : p.prompt;
      setTip(`${head}\n${p.model.name} · ${image.width}×${image.height}`);
      setLabel(head);
    });
  }

  return (
    <button
      type="button"
      data-image-id={image.id}
      title={tip}
      aria-label={label}
      aria-pressed={selected}
      onMouseEnter={loadTip}
      onFocus={loadTip}
      onClick={onSelect}
      onDoubleClick={(e) => onOpen(e.currentTarget)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onOpen(e.currentTarget);
        }
      }}
      className={cn(
        "relative size-28 shrink-0 overflow-hidden rounded-md border bg-muted/30 outline-none transition focus-visible:ring-2 focus-visible:ring-ring",
        selected && "ring-2 ring-primary",
      )}
    >
      <img src={imageSrc(image.thumbPath)} alt="" className="size-full object-cover" loading="lazy" draggable={false} />
      {image.isFavorite && <Star aria-hidden className="absolute right-1 top-1 size-3.5 fill-amber-400 text-amber-400" />}
    </button>
  );
}
