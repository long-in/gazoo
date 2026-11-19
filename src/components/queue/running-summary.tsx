"use client";

import { useEffect, useState } from "react";

import { Progress } from "@/components/ui/progress";
import { useT } from "@/lib/i18n/use-t";
import { formatDuration, type QueueTab, runningTiming } from "@/lib/queue";
import { useJobStore } from "@/stores/job-store";

/** 1 秒ごとに現在時刻を返す(実行中のときだけ更新する)。 */
export function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const first = setTimeout(() => setNow(Date.now()), 0);
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      clearTimeout(first);
      clearInterval(id);
    };
  }, [active]);
  return now;
}

/** 上部のサマリ(docs/20 §3.1): 実行中ジョブの進捗・経過・残り時間の目安と、件数。 */
export function RunningSummary({ counts }: { counts: Record<QueueTab, number> }) {
  const t = useT();
  const running = useJobStore((s) => s.jobs.find((j) => j.status === "running"));
  const progress = useJobStore((s) => (running ? s.progress[running.id] : undefined));
  const now = useNow(running !== undefined);

  const waiting = counts.active - (running ? 1 : 0);
  const percent =
    progress && progress.totalSteps > 0 ? Math.round((progress.step / progress.totalSteps) * 100) : null;
  const timing = running ? runningTiming(running, progress, now) : null;

  return (
    <section aria-live="polite" className="rounded-xl border p-4">
      {running && timing ? (
        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <p className="font-medium">
              {progress?.phase === "face_detail" ? t("generate.running.face") : t("generate.running")}
              {progress && (
                <span className="ml-2 text-sm font-normal text-muted-foreground">
                  {t("generate.step")} {progress.step} / {progress.totalSteps}
                </span>
              )}
            </p>
            <p className="text-xs text-muted-foreground">
              {t("queue.elapsed")} {formatDuration(timing.elapsedMs)}
              {timing.remainingMs !== null && ` · ${t("queue.remaining")} ${formatDuration(timing.remainingMs)}`}
            </p>
          </div>
          {/* 進捗が取れない間(モデル読み込み中など)は不定表示 */}
          <Progress value={percent} />
          <p className="line-clamp-1 text-xs text-muted-foreground">{running.params.prompt}</p>
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">{t("generate.idle")}</p>
      )}
      <p className="mt-2 text-xs text-muted-foreground">
        {t("queue.summary")
          .replace("{waiting}", String(Math.max(0, waiting)))
          .replace("{done}", String(counts.completed))
          .replace("{failed}", String(counts.failed))}
      </p>
    </section>
  );
}
