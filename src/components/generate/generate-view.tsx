"use client";

import { useEffect } from "react";

import { EngineBanner } from "@/components/engine/engine-banner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useT } from "@/lib/i18n/use-t";
import { useMinWidth } from "@/lib/use-min-width";
import { useAssistantStore } from "@/stores/assistant-store";
import { useStripStore } from "@/stores/strip-store";

import { AssistantPanel } from "./assistant-panel";
import { GalleryStrip } from "./gallery-strip";
import { GenerationForm } from "./generation-form";
import { QueueSidebar } from "./queue-sidebar";
import { Workspace } from "./workspace";

/**
 * 生成ページ(docs/27)。メイン = 生成フォーム + 補助、右サイド = キュー、画面下 = ギャラリー帯。
 * 幅: xl(1280px)以上は 3 列、lg(1024px)以上は 2 列(右サイドは [キュー][補助] のタブ)、それ未満は 1 カラム。
 */
export function GenerateView() {
  const t = useT();
  const panelOpen = useAssistantStore((s) => s.panelOpen);
  const xl = useMinWidth(1280);
  const lg = useMinWidth(1024);

  // 他の画面へ移っても、選択は持ち越さない
  useEffect(() => () => useStripStore.getState().select(null), []);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-0 flex-1 flex-col gap-3 p-4 pb-2">
        <EngineBanner />
        {xl ? (
          <div className="grid min-h-0 flex-1 grid-cols-[minmax(22rem,28rem)_1fr_20rem] gap-4">
            <Column className="pr-2">
              <GenerationForm />
            </Column>
            <Column>
              <Workspace />
            </Column>
            <Column className="rounded-lg border">
              <QueueSidebar />
            </Column>
          </div>
        ) : lg ? (
          <div className="grid min-h-0 flex-1 grid-cols-[minmax(22rem,28rem)_1fr] gap-4">
            <Column className="pr-2">
              <GenerationForm />
            </Column>
            <Tabs defaultValue="queue" className="min-h-0 gap-2">
              <TabsList>
                <TabsTrigger value="queue">{t("workbench.tab.queue")}</TabsTrigger>
                <TabsTrigger value="helper">{t("workbench.tab.helper")}</TabsTrigger>
              </TabsList>
              <TabsContent value="queue" className="min-h-0 overflow-y-auto rounded-lg border">
                <QueueSidebar />
              </TabsContent>
              <TabsContent value="helper" className="min-h-0 overflow-y-auto">
                <Workspace />
              </TabsContent>
            </Tabs>
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
            <GenerationForm />
            <Workspace />
            <div className="rounded-lg border">
              <QueueSidebar />
            </div>
          </div>
        )}
      </div>
      <GalleryStrip />
      {/* フォームの外に置く(フォームの Ctrl+Enter で生成が走らないように) */}
      <AssistantPanel open={panelOpen} onOpenChange={(o) => useAssistantStore.getState().setPanelOpen(o)} />
    </div>
  );
}

/** 独立してスクロールする列。 */
function Column({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={`min-h-0 overflow-y-auto ${className ?? ""}`}>{children}</div>;
}
