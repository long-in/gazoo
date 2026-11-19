"use client";

import {
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  ArrowUpToLine,
  ChevronDown,
  ChevronRight,
  Copy,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { imageSrc } from "@/lib/image-src";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { MessageKey } from "@/lib/i18n";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { durationMs, formatDuration, isActiveJob } from "@/lib/queue";
import { isOutOfMemory, toAppError } from "@/lib/tauri/errors";
import { imageGet } from "@/lib/tauri/images";
import type { ImageItem, Job, JobStatus } from "@/lib/tauri/jobs";
import { useFormStore } from "@/stores/form-store";
import { useJobStore } from "@/stores/job-store";

const STATUS_VARIANT: Record<JobStatus, "default" | "secondary" | "destructive" | "outline"> = {
  queued: "secondary",
  running: "default",
  completed: "outline",
  failed: "destructive",
  cancelled: "outline",
};

/** サムネイルに使う画像(最大)。 */
const MAX_THUMBS = 4;
const thumbCache = new Map<string, ImageItem>();

/** ジョブの生成画像(最大 4 枚)を取得する。取得済みはキャッシュする。 */
function useThumbs(job: Job): ImageItem[] {
  const ids = job.imageIds.slice(0, MAX_THUMBS);
  const key = ids.join(",");
  const [items, setItems] = useState<ImageItem[]>(() => ids.flatMap((id) => thumbCache.get(id) ?? []));
  useEffect(() => {
    let cancelled = false;
    void Promise.all(
      ids.map(async (id) => {
        const hit = thumbCache.get(id);
        if (hit) return hit;
        try {
          const { image } = await imageGet(id);
          thumbCache.set(id, image);
          return image;
        } catch {
          return null; // 削除済みなど
        }
      }),
    ).then((list) => {
      if (!cancelled) setItems(list.filter((x): x is ImageItem => x !== null));
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- ids は key から決まる
  }, [key]);
  return items;
}

/** 失敗の理由。コードに対応する説明を主に、エンジンの原文を小さく併記する。 */
function JobErrorNote({ code, message }: { code: string; message: string }) {
  const t = useT();
  const key = `error.${code}` as MessageKey;
  const friendly = t(key); // 未知のコードは undefined(原文のみ表示)
  return (
    <div role="alert" className="flex flex-col gap-1 text-xs">
      {friendly && <p className="font-medium text-destructive">{friendly}</p>}
      {isOutOfMemory(message) && <p className="text-amber-600 dark:text-amber-400">{t("error.hint.oom")}</p>}
      <p className="break-words font-mono text-muted-foreground">{message}</p>
    </div>
  );
}

/** 有効な拡張機能の名前(詳細表示用)。 */
function extras(job: Job, t: (k: MessageKey) => string): string[] {
  const p = job.params;
  const out: string[] = [];
  if (p.img2img?.enabled) out.push(`${t("gallery.params.img2img")} (denoise ${p.img2img.denoise})`);
  if (p.controlnet?.enabled) out.push("ControlNet");
  if (p.inpaint?.enabled) out.push(`Inpaint (denoise ${p.inpaint.denoise})`);
  if (p.faceDetailer?.enabled) out.push(`${t("gallery.params.face")} (denoise ${p.faceDetailer.denoise})`);
  return out;
}

export function JobRow({
  job,
  selected,
  onToggle,
  queueIndex,
  queuedCount,
}: {
  job: Job;
  selected: boolean;
  onToggle: () => void;
  queueIndex: number;
  queuedCount: number;
}) {
  const t = useT();
  const router = useRouter();
  const errorMessage = useErrorMessage();
  const progress = useJobStore((s) => s.progress[job.id]);
  const { cancel, reorder, retry, remove, enqueue } = useJobStore.getState();
  const thumbs = useThumbs(job);
  const [open, setOpen] = useState(false);

  const run = async (action: () => Promise<unknown>, success?: string) => {
    try {
      await action();
      if (success) toast.success(success);
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  };

  const percent =
    progress && progress.totalSteps > 0 ? Math.round((progress.step / progress.totalSteps) * 100) : null;
  const { params } = job;
  const took = durationMs(job);
  const active = isActiveJob(job);

  /** この設定を生成画面に読み込む(「この設定で生成」と同じ)。 */
  const load = () => {
    useFormStore.getState().loadParams({ ...params, seedRandomize: false });
    router.push("/");
  };
  /** 同じ設定で、新しいシードでもう一度生成する。 */
  const again = () => run(() => enqueue({ ...params, seedRandomize: true }, 1), t("queue.again.done"));

  const iconBtn = (label: string, onClick: () => void, icon: React.ReactNode, disabled = false) => (
    <Button type="button" variant="ghost" size="icon-sm" aria-label={label} title={label} disabled={disabled} onClick={onClick}>
      {icon}
    </Button>
  );

  return (
    <li className="flex flex-col gap-2 rounded-xl border p-3">
      <div className="flex items-start gap-3">
        <input
          type="checkbox"
          className="mt-1 size-4 shrink-0 accent-primary"
          checked={selected}
          onChange={onToggle}
          aria-label={t("queue.select")}
        />
        {thumbs.length > 0 && (
          <div className="flex shrink-0 gap-1">
            {thumbs.map((img) => (
              <button
                key={img.id}
                type="button"
                title={t("queue.openImage")}
                onClick={() => router.push(`/gallery?id=${img.id}`)}
                className="overflow-hidden rounded-md border bg-muted/30"
              >
                <img src={imageSrc(img.thumbPath)} alt={`seed ${img.seed}`} className="size-14 object-cover" loading="lazy" />
              </button>
            ))}
            {job.imageIds.length > MAX_THUMBS && (
              <span className="self-end text-xs text-muted-foreground">+{job.imageIds.length - MAX_THUMBS}</span>
            )}
          </div>
        )}
        <div className="min-w-0 flex-1">
          <p className="line-clamp-2 text-sm">{params.prompt}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {params.model.name} · {params.width}×{params.height} · {params.steps} steps · seed {params.seed}
            {job.imageIds.length > 0 && ` · ${job.imageIds.length} ${t("queue.images")}`}
            {took !== null && ` · ${formatDuration(took)}`}
          </p>
        </div>
        <Badge variant={STATUS_VARIANT[job.status]}>{t(`status.${job.status}` as MessageKey)}</Badge>
      </div>

      {job.status === "running" && <Progress value={percent} />}
      {job.error && <JobErrorNote code={job.error.code} message={job.error.message} />}

      <div className="flex flex-wrap items-center justify-between gap-1">
        <Button type="button" variant="ghost" size="sm" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          {open ? <ChevronDown /> : <ChevronRight />}
          {t("queue.details")}
        </Button>
        <div className="flex flex-wrap justify-end gap-1">
          {job.status === "queued" && (
            <>
              {iconBtn(t("queue.moveTop"), () => void run(() => reorder(job.id, 0)), <ArrowUpToLine />, queueIndex <= 0)}
              {iconBtn(t("queue.moveUp"), () => void run(() => reorder(job.id, queueIndex - 1)), <ArrowUp />, queueIndex <= 0)}
              {iconBtn(
                t("queue.moveDown"),
                () => void run(() => reorder(job.id, queueIndex + 1)),
                <ArrowDown />,
                queueIndex < 0 || queueIndex >= queuedCount - 1,
              )}
              {iconBtn(
                t("queue.moveBottom"),
                () => void run(() => reorder(job.id, queuedCount - 1)),
                <ArrowDownToLine />,
                queueIndex < 0 || queueIndex >= queuedCount - 1,
              )}
            </>
          )}
          {active && (
            <Button type="button" variant="outline" size="sm" onClick={() => void run(() => cancel(job.id))}>
              <X />
              {t("queue.cancel")}
            </Button>
          )}
          {!active && (
            <>
              <Button type="button" variant="outline" size="sm" onClick={load}>
                <Copy />
                {t("queue.load")}
              </Button>
              {(job.status === "failed" || job.status === "cancelled") && (
                <Button type="button" variant="outline" size="sm" onClick={() => void run(() => retry(job.id), t("queue.retried"))}>
                  <RotateCcw />
                  {t("queue.retry")}
                </Button>
              )}
              <Button type="button" variant="outline" size="sm" onClick={() => void again()}>
                <RotateCcw />
                {t("queue.again")}
              </Button>
              {iconBtn(t("queue.remove"), () => void run(() => remove([job.id])), <Trash2 />)}
            </>
          )}
        </div>
      </div>

      {open && (
        <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1 rounded-lg bg-muted/40 p-3 text-xs">
          <dt className="text-muted-foreground">{t("generate.prompt")}</dt>
          <dd className="whitespace-pre-wrap break-words">{params.prompt}</dd>
          {params.negativePrompt && (
            <>
              <dt className="text-muted-foreground">{t("generate.negative")}</dt>
              <dd className="whitespace-pre-wrap break-words">{params.negativePrompt}</dd>
            </>
          )}
          <dt className="text-muted-foreground">{t("generate.model")}</dt>
          <dd className="break-all">
            {params.model.name} ({params.model.kind})
          </dd>
          <dt className="text-muted-foreground">{t("queue.detail.sampling")}</dt>
          <dd>
            {params.sampler} / {params.scheduler} · {params.steps} steps · CFG {params.cfg}
            {params.model.kind.startsWith("flux") && ` · guidance ${params.guidance}`}
          </dd>
          <dt className="text-muted-foreground">{t("generate.size")}</dt>
          <dd>
            {params.width}×{params.height} · {t("generate.batch")} {params.batchSize} · seed {params.seed}
          </dd>
          {extras(job, t).length > 0 && (
            <>
              <dt className="text-muted-foreground">{t("queue.detail.extras")}</dt>
              <dd>{extras(job, t).join(" / ")}</dd>
            </>
          )}
          {took !== null && (
            <>
              <dt className="text-muted-foreground">{t("queue.detail.took")}</dt>
              <dd>{formatDuration(took)}</dd>
            </>
          )}
        </dl>
      )}
    </li>
  );
}
