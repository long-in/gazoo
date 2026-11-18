"use client";

import { RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n/use-t";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";

/** サイドナビ下部の ComfyUI 接続状態表示(docs/05-ui-design.md §2)。 */
export function EngineStatusBadge() {
  const t = useT();
  const status = useEngineStore((s) => s.status);
  const reconnecting = useEngineStore((s) => s.reconnecting);
  const reconnect = useEngineStore((s) => s.reconnect);

  const label = !status
    ? t("engine.checking")
    : status.connected
      ? t("engine.connected")
      : t("engine.disconnected");

  return (
    <div className="mt-auto flex items-center gap-2 rounded-lg border bg-background/50 px-2 py-1.5">
      <span
        aria-hidden
        className={cn(
          "size-2 shrink-0 rounded-full",
          !status && "bg-muted-foreground/50",
          status?.connected && "bg-emerald-500",
          status && !status.connected && "bg-destructive",
        )}
      />
      <span className="min-w-0 flex-1 truncate text-xs" role="status" title={status?.url}>
        {label}
      </span>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label={t("engine.reconnect")}
        title={t("engine.reconnect")}
        disabled={reconnecting}
        onClick={() => void reconnect()}
      >
        <RefreshCw className={cn("size-3", reconnecting && "animate-spin")} />
      </Button>
    </div>
  );
}
