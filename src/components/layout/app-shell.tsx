"use client";

import {
  Download,
  Images,
  type LucideIcon,
  Settings,
  Sparkles,
  BookmarkCheck,
} from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { type ReactNode, useSyncExternalStore } from "react";

import { EngineStatusBadge } from "@/components/engine/engine-status-badge";
import { cn } from "@/lib/utils";
import { type MessageKey } from "@/lib/i18n";
import { useT } from "@/lib/i18n/use-t";
import { detectMac, modifierLabel } from "@/lib/shortcuts";

import { JobStatusBar } from "./job-status-bar";
import { useGlobalShortcuts } from "./use-global-shortcuts";

const NAV_ITEMS: { href: string; labelKey: MessageKey; icon: LucideIcon }[] = [
  { href: "/", labelKey: "nav.generate", icon: Sparkles },
  { href: "/gallery", labelKey: "nav.gallery", icon: Images },
  { href: "/presets", labelKey: "nav.presets", icon: BookmarkCheck },
  { href: "/models", labelKey: "nav.models", icon: Download },
  { href: "/settings", labelKey: "nav.settings", icon: Settings },
];

function isActive(pathname: string, href: string) {
  if (href === "/") return pathname === "/";
  return pathname === href || pathname.startsWith(`${href}/`);
}

const subscribe = () => () => {};

export function AppShell({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  const t = useT();
  useGlobalShortcuts();
  // 表示用の修飾キー(サーバー側の事前レンダリングでは Ctrl 表記で固定し、不一致を避ける)
  const isMac = useSyncExternalStore(subscribe, detectMac, () => false);

  return (
    <div className="flex h-screen">
      <nav
        aria-label="Main"
        className="flex w-52 shrink-0 flex-col gap-1 border-r bg-sidebar p-3 text-sidebar-foreground"
      >
        <div className="px-2 pb-3 pt-1 text-lg font-semibold">{t("app.name")}</div>
        {NAV_ITEMS.map(({ href, labelKey, icon: Icon }, index) => {
          const active = isActive(pathname, href);
          return (
            <Link
              key={href}
              href={href}
              aria-current={active ? "page" : undefined}
              title={`${t(labelKey)} (${modifierLabel(isMac)}${index + 1})`}
              className={cn(
                "flex items-center gap-2 rounded-lg px-2 py-2 text-sm transition-colors hover:bg-sidebar-accent",
                active && "bg-sidebar-accent font-medium text-sidebar-accent-foreground",
              )}
            >
              <Icon className="size-4" />
              {t(labelKey)}
            </Link>
          );
        })}
        <EngineStatusBadge />
      </nav>
      <div className="flex min-w-0 flex-1 flex-col">
        <JobStatusBar />
        <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
      </div>
    </div>
  );
}
