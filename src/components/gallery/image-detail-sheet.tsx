"use client";

import { save } from "@tauri-apps/plugin-dialog";
import { Brush, Copy, Download, FolderOpen, ImagePlus, Sparkles, Star, Trash2, Wand2 } from "lucide-react";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { imageSrc } from "@/lib/image-src";
import { Button } from "@/components/ui/button";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { supportsImg2img, supportsInpaint } from "@/lib/generation/defaults";
import { supportsReference } from "@/lib/generation/reference";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { toAppError } from "@/lib/tauri/errors";
import { type ImageDetail, imageExport, imageGet, imageReveal } from "@/lib/tauri/images";
import { inputImageFromGallery } from "@/lib/tauri/inputs";
import { useEngineStore } from "@/stores/engine-store";
import { useFormStore } from "@/stores/form-store";
import { useGalleryStore } from "@/stores/gallery-store";
import { useModelsStore } from "@/stores/models-store";

import { DeleteConfirm } from "./delete-confirm";

type State = { id: string; status: "loading" } | { id: string; status: "missing" } | { id: string; status: "ready"; detail: ImageDetail };

/** ギャラリーの詳細パネル(`/gallery?id=` で開く)。 */
export function ImageDetailSheet({ id, onClose }: { id: string | null; onClose: () => void }) {
  const t = useT();
  const errorMessage = useErrorMessage();
  const router = useRouter();

  const [state, setState] = useState<State | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const items = useGalleryStore((s) => s.items);
  const setFavorite = useGalleryStore((s) => s.setFavorite);
  const remove = useGalleryStore((s) => s.remove);

  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    imageGet(id)
      .then((detail) => !cancelled && setState({ id, status: "ready", detail }))
      .catch(() => !cancelled && setState({ id, status: "missing" }));
    return () => {
      cancelled = true;
    };
  }, [id]);

  // 一覧側でお気に入りを切り替えたとき等に表示を合わせる
  const current = state && state.id === id ? state : null;
  const detail = current?.status === "ready" ? current.detail : null;
  const image = detail ? (items.find((i) => i.id === detail.image.id) ?? detail.image) : null;
  const params = detail?.job.params;

  const run = async (action: () => Promise<unknown>, okKey?: Parameters<typeof t>[0]) => {
    try {
      await action();
      if (okKey) toast.success(t(okKey));
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  };

  async function copy(text: string) {
    await run(() => navigator.clipboard.writeText(text), "gallery.copied");
  }

  async function exportImage() {
    if (!image) return;
    const ext = image.filePath.split(".").pop() ?? "png";
    const dest = await save({
      defaultPath: `gazooo-${image.seed}.${ext}`,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (dest) await run(() => imageExport(image.id, dest), "gallery.exported");
  }

  function useSettings() {
    if (!image || !params) return;
    // バッチ内の各画像は同じシードを共有するため、画像のシードで再現する
    useFormStore.getState().loadParams({ ...params, seed: image.seed });
    router.push("/");
  }

  /** この画像を ControlNet の参照画像として生成画面に設定する。 */
  async function setAsReference() {
    if (!image) return;
    try {
      const input = await inputImageFromGallery(image.id);
      const controlnets = useEngineStore.getState().models?.controlnets ?? [];
      const enabled = useFormStore.getState().setReferenceImage(input.path, controlnets);
      if (enabled) toast.success(t("gallery.referenceSet"));
      else toast.info(t("gallery.referenceFlux"));
      router.push("/");
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  /** この画像を Inpaint の参照画像として生成画面に設定する(マスクは生成画面で描く)。 */
  async function setAsInpaint() {
    if (!image) return;
    try {
      const input = await inputImageFromGallery(image.id);
      const form = useFormStore.getState();
      form.setInpaintImage(input.path, input.width, input.height);
      if (supportsInpaint(form.params.model.kind)) {
        form.setInpaintEnabled(true);
        toast.success(t("gallery.inpaintSet"));
      } else {
        toast.info(t("gallery.inpaintFlux"));
      }
      router.push("/");
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  /** この画像を Qwen-Image 2.1 の参照画像に追加する(docs/24 §6.2)。種別が違うときは取り込むだけで、有効にはしない。 */
  async function setAsQwenReference() {
    if (!image) return;
    try {
      const input = await inputImageFromGallery(image.id);
      const form = useFormStore.getState();
      form.addReferenceImage(input.path, input.width, input.height);
      if (supportsReference(form.params.model.kind)) {
        form.setReferenceEnabled(true);
        toast.success(t("gallery.qwenReferenceSet"));
      } else {
        toast.info(t("gallery.qwenReferenceOther"));
      }
      router.push("/");
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  /** この画像を img2img の元画像として生成画面に設定する。anime が true なら、アニメ系のモデルと推奨値も設定する。 */
  async function setAsImg2img(anime: boolean) {
    if (!image) return;
    try {
      const input = await inputImageFromGallery(image.id);
      const form = useFormStore.getState();
      if (anime) {
        const models = useModelsStore.getState();
        if (!models.view) await models.load();
        const engine = useEngineStore.getState();
        const { outcome, compositionLocked } = form.startAnime(
          { path: input.path, width: input.width, height: input.height },
          {
            catalog: useModelsStore.getState().view?.catalog,
            models: engine.models,
            cannyAvailable: (engine.status?.features.controlnetPreprocessors ?? []).includes("canny"),
          },
        );
        if (outcome === "noAnimeModel") {
          toast.warning(t("generate.img2img.anime.noModel"));
          router.push("/models?purpose=anime");
          return;
        }
        toast.success(t(compositionLocked ? "generate.img2img.anime.done" : "generate.img2img.anime.doneNoLock"));
      } else {
        form.setImg2imgImage(input.path, input.width, input.height);
        if (supportsImg2img(form.params.model.kind)) {
          form.setImg2imgEnabled(true);
          toast.success(t("gallery.img2imgSet"));
        } else {
          toast.info(t("gallery.img2imgFlux"));
        }
      }
      router.push("/");
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  return (
    <>
      <Sheet open={id !== null} onOpenChange={(open) => !open && onClose()}>
        <SheetContent side="right" className="w-full overflow-y-auto sm:max-w-xl">
          <SheetHeader>
            <SheetTitle>{t("gallery.detail")}</SheetTitle>
            <SheetDescription className="sr-only">{t("gallery.detail")}</SheetDescription>
          </SheetHeader>

          {current?.status === "missing" && <p className="p-4 text-sm text-muted-foreground">{t("gallery.notFound")}</p>}

          {image && params && (
            <div className="flex flex-col gap-4 px-4 pb-6">
              <img
                src={imageSrc(image.filePath)}
                alt={params.prompt}
                width={image.width}
                height={image.height}
                className="max-h-[60vh] w-full rounded-lg border bg-muted/30 object-contain"
              />

              <div className="flex flex-wrap gap-2">
                <Button type="button" onClick={useSettings}>
                  <Sparkles />
                  {t("gallery.useSettings")}
                </Button>
                <Button type="button" variant="outline" onClick={() => void setAsReference()}>
                  <ImagePlus />
                  {t("gallery.useAsReference")}
                </Button>
                <Button type="button" variant="outline" onClick={() => void setAsImg2img(true)}>
                  <Wand2 />
                  {t("gallery.useAsAnime")}
                </Button>
                <Button type="button" variant="outline" onClick={() => void setAsImg2img(false)}>
                  <ImagePlus />
                  {t("gallery.useAsImg2img")}
                </Button>
                <Button type="button" variant="outline" onClick={() => void setAsQwenReference()}>
                  <ImagePlus />
                  {t("gallery.useAsQwenReference")}
                </Button>
                <Button type="button" variant="outline" onClick={() => void setAsInpaint()}>
                  <Brush />
                  {t("gallery.useAsInpaint")}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  aria-pressed={image.isFavorite}
                  onClick={() => void run(() => setFavorite(image.id, !image.isFavorite))}
                >
                  <Star className={image.isFavorite ? "fill-amber-400 text-amber-400" : undefined} />
                  {image.isFavorite ? t("gallery.unfavorite") : t("gallery.favorite")}
                </Button>
                <Button type="button" variant="outline" onClick={() => void exportImage()}>
                  <Download />
                  {t("gallery.export")}
                </Button>
                <Button type="button" variant="outline" onClick={() => void run(() => imageReveal(image.id))}>
                  <FolderOpen />
                  {t("gallery.reveal")}
                </Button>
                <Button type="button" variant="outline" className="text-destructive" onClick={() => setConfirmDelete(true)}>
                  <Trash2 />
                  {t("gallery.delete")}
                </Button>
              </div>

              <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-2 text-sm">
                <dt className="text-muted-foreground">{t("gallery.params.prompt")}</dt>
                <dd className="flex items-start gap-1 break-words">
                  <span className="min-w-0 flex-1 whitespace-pre-wrap">{params.prompt}</span>
                  <Button type="button" variant="ghost" size="icon-xs" aria-label={t("gallery.copyPrompt")} title={t("gallery.copyPrompt")} onClick={() => void copy(params.prompt)}>
                    <Copy />
                  </Button>
                </dd>
                {params.negativePrompt && (
                  <>
                    <dt className="text-muted-foreground">{t("gallery.params.negative")}</dt>
                    <dd className="whitespace-pre-wrap break-words">{params.negativePrompt}</dd>
                  </>
                )}
                <dt className="text-muted-foreground">{t("gallery.params.model")}</dt>
                <dd className="break-all">{params.model.name}</dd>
                <dt className="text-muted-foreground">{t("gallery.params.size")}</dt>
                <dd>
                  {image.width}×{image.height}
                </dd>
                <dt className="text-muted-foreground">{t("gallery.params.steps")}</dt>
                <dd>
                  {params.steps}
                  {params.model.kind.startsWith("flux") ? ` · guidance ${params.guidance}` : ` · CFG ${params.cfg}`}
                </dd>
                <dt className="text-muted-foreground">{t("gallery.params.sampler")}</dt>
                <dd>
                  {params.sampler} / {params.scheduler}
                </dd>
                {params.controlnet?.enabled && params.controlnet.units[0] && (
                  <>
                    <dt className="text-muted-foreground">{t("gallery.params.controlnet")}</dt>
                    <dd className="break-all">
                      {params.controlnet.units[0].preprocessor} · {params.controlnet.units[0].model} ·{" "}
                      {params.controlnet.units[0].strength}
                    </dd>
                  </>
                )}
                {params.reference?.enabled && (
                  <>
                    <dt className="text-muted-foreground">{t("gallery.params.reference")}</dt>
                    <dd>
                      {params.reference.images.length} · {params.reference.resolution}
                    </dd>
                  </>
                )}
                {params.img2img?.enabled && (
                  <>
                    <dt className="text-muted-foreground">{t("gallery.params.img2img")}</dt>
                    <dd>denoise {params.img2img.denoise}</dd>
                  </>
                )}
                {params.inpaint?.enabled && (
                  <>
                    <dt className="text-muted-foreground">{t("gallery.params.inpaint")}</dt>
                    <dd>
                      denoise {params.inpaint.denoise} · grow {params.inpaint.growMask} · blur {params.inpaint.feather}
                    </dd>
                  </>
                )}
                {params.faceDetailer?.enabled && (
                  <>
                    <dt className="text-muted-foreground">{t("gallery.params.face")}</dt>
                    <dd className="break-all">
                      {params.faceDetailer.detector.replace(/^bbox\//, "")} · {params.faceDetailer.denoise}
                      {params.faceDetailer.prompt.trim() && ` · ${params.faceDetailer.prompt}`}
                    </dd>
                  </>
                )}
                <dt className="text-muted-foreground">{t("gallery.params.seed")}</dt>
                <dd className="flex items-center gap-1">
                  <span>{image.seed}</span>
                  <Button type="button" variant="ghost" size="icon-xs" aria-label={t("gallery.copySeed")} title={t("gallery.copySeed")} onClick={() => void copy(String(image.seed))}>
                    <Copy />
                  </Button>
                </dd>
                <dt className="text-muted-foreground">{t("gallery.params.created")}</dt>
                <dd>{new Date(image.createdAt).toLocaleString()}</dd>
              </dl>
            </div>
          )}
        </SheetContent>
      </Sheet>

      <DeleteConfirm
        open={confirmDelete}
        count={1}
        onOpenChange={setConfirmDelete}
        onConfirm={() => {
          setConfirmDelete(false);
          if (!image) return;
          void run(() => remove([image.id], true), "gallery.deleted").then(onClose);
        }}
      />
    </>
  );
}
