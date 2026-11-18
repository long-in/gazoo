"use client";

import { EngineBanner } from "@/components/engine/engine-banner";
import { useT } from "@/lib/i18n/use-t";

import { useAssistantStore } from "@/stores/assistant-store";

import { AssistantPanel } from "./assistant-panel";
import { GenerationForm } from "./generation-form";
import { ResultPanel } from "./result-panel";

export function GenerateView() {
  const t = useT();
  const panelOpen = useAssistantStore((s) => s.panelOpen);
  return (
    <div className="flex h-full min-h-0 flex-col gap-4 p-6">
      <h1 className="text-2xl font-semibold">{t("nav.generate")}</h1>
      <EngineBanner />
      <div className="grid min-h-0 flex-1 gap-6 lg:grid-cols-[minmax(22rem,28rem)_1fr]">
        <div className="min-h-0 overflow-y-auto pr-2">
          <GenerationForm />
        </div>
        <div className="min-h-0 overflow-y-auto">
          <ResultPanel />
        </div>
      </div>
      {/* フォームの外に置く(フォームの Ctrl+Enter で生成が走らないように) */}
      <AssistantPanel open={panelOpen} onOpenChange={(o) => useAssistantStore.getState().setPanelOpen(o)} />
    </div>
  );
}
