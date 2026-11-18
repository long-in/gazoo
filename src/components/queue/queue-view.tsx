"use client";

import { ArrowDown, ArrowUp, RotateCcw, X } from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { MessageKey } from "@/lib/i18n";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { isOutOfMemory, toAppError } from "@/lib/tauri/errors";
import type { Job, JobStatus } from "@/lib/tauri/jobs";
import { useJobStore } from "@/stores/job-store";

const STATUS_VARIANT: Record<JobStatus, "default" | "secondary" | "destructive" | "outline"> = {
  queued: "secondary",
  running: "default",
  completed: "outline",
  failed: "destructive",
  cancelled: "outline",
};

export function QueueView() {
  const t = useT();
  const errorMessage = useErrorMessage();
  const jobs = useJobStore((s) => s.jobs);
  const clearFinished = useJobStore((s) => s.clearFinished);

  const active = jobs.filter((j) => j.status === "running" || j.status === "queued");
  const history = jobs.filter((j) => j.status !== "running" && j.status !== "queued");

  async function onClear() {
    try {
      await clearFinished();
      toast.success(t("queue.cleared"));
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-8 p-8">
      <h1 className="text-2xl font-semibold">{t("queue.title")}</h1>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-medium">{t("queue.active")}</h2>
        {active.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("queue.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {active.map((job, i) => (
              <JobRow
                key={job.id}
                job={job}
                // 待機中ジョブ内の位置(実行中は並べ替え対象外)
                queueIndex={job.status === "queued" ? active.filter((j) => j.status === "queued").indexOf(job) : -1}
                queuedCount={active.filter((j) => j.status === "queued").length}
                isFirst={i === 0}
              />
            ))}
          </ul>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium">{t("queue.history")}</h2>
          <Button type="button" variant="outline" size="sm" disabled={history.length === 0} onClick={() => void onClear()}>
            {t("queue.clear")}
          </Button>
        </div>
        {history.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("queue.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {history.map((job) => (
              <JobRow key={job.id} job={job} queueIndex={-1} queuedCount={0} isFirst={false} />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
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

function JobRow({
  job,
  queueIndex,
  queuedCount,
}: {
  job: Job;
  queueIndex: number;
  queuedCount: number;
  isFirst: boolean;
}) {
  const t = useT();
  const errorMessage = useErrorMessage();
  const progress = useJobStore((s) => s.progress[job.id]);
  const { cancel, reorder, retry } = useJobStore.getState();

  const run = async (action: () => Promise<unknown>, successKey?: MessageKey) => {
    try {
      await action();
      if (successKey) toast.success(t(successKey));
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  };

  const percent =
    progress && progress.totalSteps > 0 ? Math.round((progress.step / progress.totalSteps) * 100) : null;
  const { params } = job;

  return (
    <li className="flex flex-col gap-2 rounded-xl border p-3">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="line-clamp-2 text-sm">{params.prompt}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {params.model.name} · {params.width}×{params.height} · {params.steps} steps · seed {params.seed}
            {job.imageIds.length > 0 && ` · ${job.imageIds.length} ${t("queue.images")}`}
          </p>
        </div>
        <Badge variant={STATUS_VARIANT[job.status]}>{t(`status.${job.status}` as MessageKey)}</Badge>
      </div>

      {job.status === "running" && <Progress value={percent} />}
      {job.error && <JobErrorNote code={job.error.code} message={job.error.message} />}

      <div className="flex justify-end gap-1">
        {job.status === "queued" && (
          <>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("queue.moveUp")}
              title={t("queue.moveUp")}
              disabled={queueIndex <= 0}
              onClick={() => void run(() => reorder(job.id, queueIndex - 1))}
            >
              <ArrowUp />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("queue.moveDown")}
              title={t("queue.moveDown")}
              disabled={queueIndex < 0 || queueIndex >= queuedCount - 1}
              onClick={() => void run(() => reorder(job.id, queueIndex + 1))}
            >
              <ArrowDown />
            </Button>
          </>
        )}
        {(job.status === "queued" || job.status === "running") && (
          <Button type="button" variant="outline" size="sm" onClick={() => void run(() => cancel(job.id))}>
            <X />
            {t("queue.cancel")}
          </Button>
        )}
        {(job.status === "failed" || job.status === "cancelled") && (
          <Button type="button" variant="outline" size="sm" onClick={() => void run(() => retry(job.id), "queue.retried")}>
            <RotateCcw />
            {t("queue.retry")}
          </Button>
        )}
      </div>
    </li>
  );
}
