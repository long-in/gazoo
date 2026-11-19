"use client";

import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { type MessageKey } from "@/lib/i18n";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { toAppError } from "@/lib/tauri/errors";
import type { Language, Settings, Theme } from "@/lib/tauri/settings";
import { useSettingsStore } from "@/stores/settings-store";

import { AssistantSection } from "./assistant-section";
import { EngineInstallSection } from "./engine-install-section";
import { EngineProcessSection } from "./engine-process-section";
import { StorageSection } from "./storage-section";

type LanguageChoice = Language | "auto";

const THEMES: { value: Theme; labelKey: MessageKey }[] = [
  { value: "system", labelKey: "settings.ui.theme.system" },
  { value: "light", labelKey: "settings.ui.theme.light" },
  { value: "dark", labelKey: "settings.ui.theme.dark" },
];

export function SettingsForm() {
  const loaded = useSettingsStore((s) => s.loaded);
  const settings = useSettingsStore((s) => s.settings);
  const loadError = useSettingsStore((s) => s.loadError);
  const errorMessage = useErrorMessage();
  const t = useT();

  if (!loaded) return null;
  if (loadError) {
    return (
      <div className="p-8 text-sm text-destructive">
        {t("settings.loadFailed")}: {errorMessage(loadError)}
      </div>
    );
  }
  return <SettingsFormInner initial={settings} />;
}

function SettingsFormInner({ initial }: { initial: Settings }) {
  const t = useT();
  const errorMessage = useErrorMessage();
  const update = useSettingsStore((s) => s.update);

  const [draft, setDraft] = useState<Settings>(initial);
  const [fieldError, setFieldError] = useState<{ field?: string; message: string } | null>(null);
  const [saving, setSaving] = useState(false);

  const languageChoice: LanguageChoice = draft.language ?? "auto";

  async function save() {
    setSaving(true);
    setFieldError(null);
    try {
      await update({
        comfyuiUrl: draft.comfyuiUrl,
        theme: draft.theme,
        language: draft.language,
      });
      toast.success(t("settings.saved"));
    } catch (e) {
      const err = toAppError(e);
      setFieldError({ field: err.details?.field, message: errorMessage(err) });
      toast.error(errorMessage(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-8 p-8">
      <h1 className="text-2xl font-semibold">{t("settings.title")}</h1>

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-medium">{t("settings.engine.title")}</h2>
        <Label htmlFor="comfyui-url">{t("settings.engine.url")}</Label>
        <Input
          id="comfyui-url"
          value={draft.comfyuiUrl}
          aria-invalid={fieldError?.field === "comfyuiUrl"}
          onChange={(e) => setDraft((d) => ({ ...d, comfyuiUrl: e.target.value }))}
        />
        <p className="text-xs text-muted-foreground">{t("settings.engine.urlHint")}</p>
        {fieldError?.field === "comfyuiUrl" && (
          <p role="alert" className="text-sm text-destructive">
            {fieldError.message}
          </p>
        )}
      </section>

      <EngineInstallSection />

      <EngineProcessSection />

      <StorageSection />

      <AssistantSection />

      <section className="flex flex-col gap-3">
        <h2 className="text-lg font-medium">{t("settings.ui.title")}</h2>
        <div className="grid grid-cols-2 gap-4">
          <div className="flex flex-col gap-2">
            <Label>{t("settings.ui.theme")}</Label>
            <Select
              value={draft.theme}
              items={THEMES.map((x) => ({ value: x.value, label: t(x.labelKey) }))}
              onValueChange={(v) => v && setDraft((d) => ({ ...d, theme: v as Theme }))}
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {THEMES.map((x) => (
                  <SelectItem key={x.value} value={x.value}>
                    {t(x.labelKey)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="flex flex-col gap-2">
            <Label>{t("settings.ui.language")}</Label>
            <Select
              value={languageChoice}
              items={[
                { value: "auto", label: t("settings.ui.language.auto") },
                { value: "ja", label: "日本語" },
                { value: "en", label: "English" },
              ]}
              onValueChange={(v) =>
                v &&
                setDraft((d) => ({
                  ...d,
                  language: v === "auto" ? null : (v as Language),
                }))
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">{t("settings.ui.language.auto")}</SelectItem>
                <SelectItem value="ja">日本語</SelectItem>
                <SelectItem value="en">English</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      </section>

      <div>
        <Button onClick={save} disabled={saving}>
          {t("settings.save")}
        </Button>
      </div>
    </div>
  );
}
