"use client";

import { Pause } from "lucide-react";
import Link from "next/link";

import { Progress } from "@/components/ui/progress";
import { useT } from "@/lib/i18n/use-t";
import { useJobStore } from "@/stores/job-store";

/**
 * 画面共通の実行状況(docs/20 §4.1)。生成中・待機中・一時停止中のときだけ、細く表示する。
 * クリックでキュー画面へ移る。詳細(経過・残り時間・一覧)はキュー画面で見る。
 */
export function JobStatusBar() {
  const t = useT();
  const runningId = useJobStore((s) => s.jobs.find((j) => j.status === "running")?.id);
  const running = runningId !== undefined;
  const waiting = useJobStore((s) => s.jobs.filter((j) => j.status === "queued").length);
  const paused = useJobStore((s) => s.paused);
  const progress = useJobStore((s) => (runningId ? s.progress[runningId] : undefined));

  if (!running && waiting === 0 && !paused) return null;

  const percent =
    progress && progress.totalSteps > 0 ? Math.round((progress.step / progress.totalSteps) * 100) : null;

  return (
    <Link
      href="/queue"
      title={t("jobbar.open")}
      aria-live="polite"
      className="flex shrink-0 items-center gap-3 border-b bg-muted/40 px-4 py-1.5 text-xs transition-colors hover:bg-muted"
    >
      {paused && (
        <span className="flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400">
          <Pause className="size-3" />
          {t("jobbar.paused")}
        </span>
      )}
      {running ? (
        <>
          <span className="font-medium">
            {progress?.phase === "face_detail" ? t("generate.running.face") : t("generate.running")}
            {progress && (
              <span className="ml-1.5 font-normal text-muted-foreground">
                {progress.step} / {progress.totalSteps}
              </span>
            )}
          </span>
          <Progress value={percent} className="max-w-48 flex-1" />
        </>
      ) : (
        !paused && <span className="font-medium">{t("generate.idle")}</span>
      )}
      {waiting > 0 && (
        <span className="text-muted-foreground">
          {t("generate.waitingCount")}: {waiting}
        </span>
      )}
      <span className="ml-auto text-muted-foreground">{t("jobbar.open")}</span>
    </Link>
  );
}
