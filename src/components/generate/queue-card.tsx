"use client";

import { AlertTriangle, Ban, Clock, Loader2 } from "lucide-react";

import { Progress } from "@/components/ui/progress";
import { useT } from "@/lib/i18n/use-t";
import type { Job } from "@/lib/tauri/jobs";
import { cn } from "@/lib/utils";
import { useJobStore } from "@/stores/job-store";

/** 帯の中のキューのカード(docs/28 §3.1)。画像のサムネイルと同じ大きさ。 */
export function QueueCard({
  job,
  waitingIndex,
  selected,
  onSelect,
  onOpen,
}: {
  job: Job;
  /** 待機中のとき、実行順(1 始まり)。 */
  waitingIndex: number;
  selected: boolean;
  onSelect: () => void;
  onOpen: (el: HTMLElement) => void;
}) {
  const t = useT();
  const progress = useJobStore((s) => s.progress[job.id]);
  const percent = progress && progress.totalSteps > 0 ? Math.round((progress.step / progress.totalSteps) * 100) : null;

  const tone =
    job.status === "failed"
      ? "border-destructive/60 bg-destructive/10"
      : job.status === "running"
        ? "border-primary/60 bg-primary/10"
        : "border-dashed bg-muted/40";

  return (
    <button
      type="button"
      data-job-id={job.id}
      title={job.params.prompt}
      aria-label={`${t(`status.${job.status}`)}: ${job.params.prompt}`}
      aria-pressed={selected}
      onClick={onSelect}
      onDoubleClick={(e) => onOpen(e.currentTarget)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onOpen(e.currentTarget);
        }
      }}
      className={cn(
        "flex size-28 shrink-0 flex-col gap-1 overflow-hidden rounded-md border p-2 text-left outline-none transition focus-visible:ring-2 focus-visible:ring-ring",
        tone,
        selected && "ring-2 ring-primary",
      )}
    >
      <span className="flex items-center gap-1 text-[11px] font-medium">
        {job.status === "running" && <Loader2 className="size-3 animate-spin" />}
        {job.status === "queued" && <Clock className="size-3" />}
        {job.status === "failed" && <AlertTriangle className="size-3 text-destructive" />}
        {job.status === "cancelled" && <Ban className="size-3" />}
        {job.status === "queued" ? `${t("status.queued")} ${waitingIndex}` : t(`status.${job.status}`)}
      </span>
      <span className="line-clamp-4 flex-1 text-[11px] leading-snug text-muted-foreground">
        {job.status === "failed" && job.error ? job.error.message : job.params.prompt}
      </span>
      {job.status === "running" && (
        <>
          <Progress value={percent} />
          {percent !== null && <span className="text-[11px] font-medium tabular-nums">{percent}%</span>}
        </>
      )}
    </button>
  );
}
