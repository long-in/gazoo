"use client";

import { convertFileSrc, isTauri } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { toAppError } from "@/lib/tauri/errors";
import type { ImageItem } from "@/lib/tauri/jobs";
import { useJobStore } from "@/stores/job-store";
import { toast } from "sonner";

export function imageSrc(path: string): string {
  return isTauri() ? convertFileSrc(path) : "";
}

/** 実行中ジョブの進捗と、このセッションで生成した画像。 */
export function ResultPanel() {
  const t = useT();
  const errorMessage = useErrorMessage();
  const jobs = useJobStore((s) => s.jobs);
  const progress = useJobStore((s) => s.progress);
  const images = useJobStore((s) => s.recentImages);
  const cancel = useJobStore((s) => s.cancel);

  const running = jobs.find((j) => j.status === "running");
  const waiting = jobs.filter((j) => j.status === "queued").length;
  const p = running ? progress[running.id] : undefined;
  const percent = p && p.totalSteps > 0 ? Math.round((p.step / p.totalSteps) * 100) : null;

  async function onCancel(id: string) {
    try {
      await cancel(id);
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <section aria-live="polite" className="rounded-xl border p-4">
        {running ? (
          <div className="flex flex-col gap-3">
            <div className="flex items-center justify-between gap-3">
              <p className="font-medium">
                {p?.phase === "face_detail" ? t("generate.running.face") : t("generate.running")}
                {p && (
                  <span className="ml-2 text-sm font-normal text-muted-foreground">
                    {t("generate.step")} {p.step} / {p.totalSteps}
                  </span>
                )}
              </p>
              <Button type="button" variant="outline" size="sm" onClick={() => void onCancel(running.id)}>
                {t("generate.cancel")}
              </Button>
            </div>
            {/* 進捗が取れない間(モデル読み込み中など)は不定表示 */}
            <Progress value={percent} />
            <p className="line-clamp-2 text-xs text-muted-foreground">{running.params.prompt}</p>
            {waiting > 0 && (
              <p className="text-xs text-muted-foreground">
                {t("generate.waitingCount")}: {waiting}
              </p>
            )}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            {waiting > 0 ? `${t("generate.waitingCount")}: ${waiting}` : t("generate.idle")}
          </p>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-medium">{t("generate.results")}</h2>
        {images.length === 0 ? (
          <p className="rounded-xl border border-dashed p-8 text-center text-sm text-muted-foreground">
            {t("generate.results.empty")}
          </p>
        ) : (
          <ul className="grid grid-cols-2 gap-3 xl:grid-cols-3">
            {images.map((img) => (
              <ResultImage key={img.id} image={img} />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function ResultImage({ image }: { image: ImageItem }) {
  return (
    <li className="overflow-hidden rounded-lg border bg-muted/30">
      <img
        src={imageSrc(image.filePath)}
        alt={`seed ${image.seed}`}
        width={image.width}
        height={image.height}
        className="aspect-square w-full object-contain"
        loading="lazy"
      />
      <p className="px-2 py-1 text-xs text-muted-foreground">
        {image.width}×{image.height} · seed {image.seed}
      </p>
    </li>
  );
}
