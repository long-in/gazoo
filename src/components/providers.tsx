"use client";

import { ThemeProvider, useTheme } from "next-themes";
import { type ReactNode, useEffect } from "react";

import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useLanguage } from "@/lib/i18n/use-t";
import {
  onAssistantDelta,
  onAssistantDone,
  onAssistantError,
  onAssistantSuggestion,
  onEngineStatus,
  onImageCreated,
  onImageDeleted,
  onJobProgress,
  onJobUpdated,
} from "@/lib/tauri/events";
import { toAppError } from "@/lib/tauri/errors";
import { useAssistantStore } from "@/stores/assistant-store";
import { useEngineStore } from "@/stores/engine-store";
import { useGalleryStore } from "@/stores/gallery-store";
import { usePresetStore } from "@/stores/preset-store";
import { useJobStore } from "@/stores/job-store";
import { useSettingsStore } from "@/stores/settings-store";

/** DB 上の設定(正)をテーマ・言語・lang 属性へ反映する。 */
function SettingsBootstrap() {
  const load = useSettingsStore((s) => s.load);
  const theme = useSettingsStore((s) => s.settings.theme);
  const language = useLanguage();
  const { setTheme } = useTheme();

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    setTheme(theme);
  }, [theme, setTheme]);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  return null;
}

/** エンジン状態の初期取得と、Rust 側からの変化通知の購読(アプリ全体で 1 回だけ)。 */
function EngineBootstrap() {
  const applyStatus = useEngineStore((s) => s.applyStatus);
  const refresh = useEngineStore((s) => s.refresh);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onEngineStatus(applyStatus).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    void refresh();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [applyStatus, refresh]);

  return null;
}

/** プリセット一覧の初期取得(生成フォームの適用メニューと管理画面で共有する)。 */
function PresetBootstrap() {
  const load = usePresetStore((s) => s.load);
  useEffect(() => {
    void load();
  }, [load]);
  return null;
}

/** ジョブ・進捗・生成画像のイベント購読と、初期のジョブ一覧の取得。 */
function JobBootstrap() {
  const { load, applyUpdated, applyProgress, applyImage } = useJobStore.getState();

  useEffect(() => {
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    // 購読を先に張ってから一覧を取得する(取得中の更新を取りこぼさない。重複は id で畳み込まれる)
    void Promise.all([
      onJobUpdated(applyUpdated),
      onJobProgress(applyProgress),
      // 生成画像: 「直近の結果」とギャラリーの両方に反映する
      onImageCreated((image) => {
        applyImage(image);
        useGalleryStore.getState().applyCreated(image);
      }),
      onImageDeleted(({ ids }) => {
        useGalleryStore.getState().applyDeleted(ids);
        useJobStore.getState().removeImages(ids);
      }),
    ]).then((fns) => {
      if (cancelled) fns.forEach((fn) => fn());
      else unlisteners.push(...fns);
    });
    void load();
    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [load, applyUpdated, applyProgress, applyImage]);

  return null;
}

/** AI サポートの応答イベントの購読(画面を移動しても取りこぼさないよう、アプリ全体で 1 回だけ)。 */
function AssistantBootstrap() {
  useEffect(() => {
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const st = () => useAssistantStore.getState();
    void Promise.all([
      onAssistantDelta((p) => st().onDelta(p.requestId, p.text)),
      onAssistantSuggestion((p) => st().onSuggestion(p.requestId, p.index, p.summary, p.changes)),
      onAssistantDone((p) => st().onDone(p.requestId)),
      onAssistantError((p) => st().onError(p.requestId, toAppError({ code: p.code, message: p.message }))),
    ]).then((fns) => {
      if (cancelled) fns.forEach((fn) => fn());
      else unlisteners.push(...fns);
    });
    void st().loadStatus();
    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);
  return null;
}

export function Providers({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <TooltipProvider>
        <SettingsBootstrap />
        <EngineBootstrap />
        <JobBootstrap />
        <PresetBootstrap />
        <AssistantBootstrap />
        {children}
        <Toaster richColors />
      </TooltipProvider>
    </ThemeProvider>
  );
}
