"use client";

import { MoreHorizontal, Pause, Play, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { MessageKey } from "@/lib/i18n";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { countByTab, filterJobs, isActiveJob, QUEUE_TABS, type QueueTab } from "@/lib/queue";
import { toAppError } from "@/lib/tauri/errors";
import { cn } from "@/lib/utils";
import { useJobStore } from "@/stores/job-store";

import { JobRow } from "@/components/queue/job-row";
import { RunningSummary } from "@/components/queue/running-summary";

type Confirm = "cancelQueued" | "retryFailed" | "removeSelected" | "clear" | null;

/**
 * キュー(生成ページの右サイド。docs/27 §4)。フィルタ・検索・サムネイル・詳細・一括操作・一時停止。
 * キューページ(`/queue`)は廃止し、20 §3 の機能はすべてここに置く。
 */
export function QueueSidebar() {
  const t = useT();
  const errorMessage = useErrorMessage();
  const jobs = useJobStore((s) => s.jobs);
  const paused = useJobStore((s) => s.paused);
  const nextCursor = useJobStore((s) => s.nextCursor);
  const loadingMore = useJobStore((s) => s.loadingMore);

  const [tab, setTab] = useState<QueueTab>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<Confirm>(null);

  const counts = useMemo(() => countByTab(jobs), [jobs]);
  const visible = useMemo(() => filterJobs(jobs, tab, query), [jobs, tab, query]);
  const queued = useMemo(() => jobs.filter((j) => j.status === "queued"), [jobs]);
  const failed = useMemo(() => jobs.filter((j) => j.status === "failed" || j.status === "cancelled"), [jobs]);

  // 存在しなくなったジョブは選択から外す
  const selectedJobs = useMemo(() => jobs.filter((j) => selected.has(j.id)), [jobs, selected]);
  const selActive = selectedJobs.filter(isActiveJob);
  const selRetry = selectedJobs.filter((j) => j.status === "failed" || j.status === "cancelled");
  const selFinished = selectedJobs.filter((j) => !isActiveJob(j));

  async function run(action: () => Promise<unknown>, success?: string) {
    try {
      await action();
      if (success) toast.success(success);
    } catch (e) {
      toast.error(errorMessage(toAppError(e)));
    }
  }

  const { cancel, retry, remove, clearFinished, setPaused, loadMore } = useJobStore.getState();

  const toggle = (id: string) =>
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const allVisibleSelected = visible.length > 0 && visible.every((j) => selected.has(j.id));
  const toggleAll = () =>
    setSelected(allVisibleSelected ? new Set() : new Set(visible.map((j) => j.id)));

  /** 確認後の一括処理。 */
  async function onConfirm() {
    const which = confirm;
    setConfirm(null);
    if (which === "cancelQueued") {
      await run(async () => {
        for (const j of queued) await cancel(j.id);
      }, t("queue.bulk.cancelled"));
    } else if (which === "retryFailed") {
      await run(async () => {
        for (const j of failed) await retry(j.id);
      }, t("queue.bulk.retried"));
    } else if (which === "removeSelected") {
      await run(async () => {
        await remove(selFinished.map((j) => j.id));
        setSelected(new Set());
      }, t("queue.bulk.removed"));
    } else if (which === "clear") {
      await run(async () => {
        await clearFinished();
        setSelected(new Set());
      }, t("queue.cleared"));
    }
  }

  const confirmText: Record<Exclude<Confirm, null>, { title: MessageKey; desc: MessageKey; ok: MessageKey }> = {
    cancelQueued: { title: "queue.confirm.cancelQueued.title", desc: "queue.confirm.cancelQueued.desc", ok: "queue.cancelQueued" },
    retryFailed: { title: "queue.confirm.retryFailed.title", desc: "queue.confirm.retryFailed.desc", ok: "queue.retryFailed" },
    removeSelected: { title: "queue.confirm.remove.title", desc: "queue.confirm.remove.desc", ok: "queue.remove" },
    clear: { title: "queue.confirm.clear.title", desc: "queue.confirm.clear.desc", ok: "queue.clear" },
  };
  const ct = confirm ? confirmText[confirm] : null;

  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-base font-semibold">{t("queue.title")}</h2>
        <div className="flex items-center gap-1">
          <Button
            type="button"
            size="sm"
            variant={paused ? "default" : "outline"}
            onClick={() => void run(() => setPaused(!paused))}
          >
            {paused ? <Play /> : <Pause />}
            {paused ? t("queue.resume") : t("queue.pause")}
          </Button>
          <Popover>
            <PopoverTrigger render={<Button type="button" size="icon-sm" variant="outline" aria-label={t("queue.menu")} title={t("queue.menu")}/>}>
                <MoreHorizontal />
              </PopoverTrigger>
            <PopoverContent align="end" className="flex w-56 flex-col gap-1 p-2">
              <Button type="button" size="sm" variant="ghost" className="justify-start" disabled={queued.length === 0} onClick={() => setConfirm("cancelQueued")}>
                {t("queue.cancelQueued")}
              </Button>
              <Button type="button" size="sm" variant="ghost" className="justify-start" disabled={failed.length === 0} onClick={() => setConfirm("retryFailed")}>
                {t("queue.retryFailed")}
              </Button>
              <Button type="button" size="sm" variant="ghost" className="justify-start" disabled={counts.all - counts.active === 0} onClick={() => setConfirm("clear")}>
                {t("queue.clear")}
              </Button>
            </PopoverContent>
          </Popover>
        </div>
      </div>

      {paused && (
        <p role="status" className="rounded-lg border border-amber-500/40 bg-amber-500/10 p-2 text-sm">
          {t("queue.paused")}
        </p>
      )}

      <RunningSummary counts={counts} />

      <div className="flex flex-col gap-2">
        <div role="tablist" aria-label={t("queue.title")} className="flex flex-wrap gap-1">
          {QUEUE_TABS.map((k) => (
            <Button
              key={k}
              type="button"
              role="tab"
              aria-selected={tab === k}
              size="sm"
              variant={tab === k ? "default" : "outline"}
              onClick={() => setTab(k)}
            >
              {t(`queue.tab.${k}` as MessageKey)}
              <span className={cn("ml-1 text-xs", tab === k ? "opacity-80" : "text-muted-foreground")}>{counts[k]}</span>
            </Button>
          ))}
        </div>
        <div className="relative w-full">
          <Search className="pointer-events-none absolute left-2 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("queue.search")}
            aria-label={t("queue.search")}
            className="pl-8"
          />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-sm">
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            className="size-4 accent-primary"
            checked={allVisibleSelected}
            disabled={visible.length === 0}
            onChange={toggleAll}
          />
          {selected.size > 0 ? t("queue.selected").replace("{n}", String(selectedJobs.length)) : t("queue.selectAll")}
        </label>
        {selectedJobs.length > 0 && (
          <>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={selActive.length === 0}
              onClick={() =>
                void run(async () => {
                  for (const j of selActive) await cancel(j.id);
                  setSelected(new Set());
                })
              }
            >
              {t("queue.cancel")}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={selRetry.length === 0}
              onClick={() =>
                void run(async () => {
                  for (const j of selRetry) await retry(j.id);
                  setSelected(new Set());
                }, t("queue.retried"))
              }
            >
              {t("queue.retry")}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={selFinished.length === 0}
              onClick={() => setConfirm("removeSelected")}
            >
              {t("queue.remove")}
            </Button>
          </>
        )}
      </div>

      {visible.length === 0 ? (
        <p className="rounded-xl border border-dashed p-8 text-center text-sm text-muted-foreground">
          {jobs.length === 0 ? t("queue.empty") : t("queue.noMatch")}
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {visible.map((job) => (
            <JobRow
              key={job.id}
              job={job}
              selected={selected.has(job.id)}
              onToggle={() => toggle(job.id)}
              queueIndex={job.status === "queued" ? queued.indexOf(job) : -1}
              queuedCount={queued.length}
            />
          ))}
        </ul>
      )}

      {nextCursor && (
        <div className="flex justify-center">
          <Button type="button" variant="outline" disabled={loadingMore} onClick={() => void loadMore()}>
            {t("queue.loadMore")}
          </Button>
        </div>
      )}

      {ct && (
        <ConfirmDialog
          open
          title={t(ct.title)}
          description={t(ct.desc)}
          confirmLabel={t(ct.ok)}
          cancelLabel={t("common.cancel")}
          destructive={confirm !== "retryFailed"}
          onOpenChange={(o) => !o && setConfirm(null)}
          onConfirm={() => void onConfirm()}
        />
      )}
    </div>
  );
}
