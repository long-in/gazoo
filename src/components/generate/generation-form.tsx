"use client";

import { Dices } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { ModelsLink } from "@/components/models/models-link";
import { PresetBar } from "@/components/presets/preset-bar";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import {
  isFlux,
  MAX_SEED,
  MODEL_KINDS,
  modelsFor,
  randomSeed,
  sizePresets,
  supportsControlnet,
} from "@/lib/generation/defaults";
import type { MessageKey } from "@/lib/i18n";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { type AppError, toAppError } from "@/lib/tauri/errors";
import { MAX_ENQUEUE_COUNT, type ModelKind } from "@/lib/tauri/jobs";
import { cn } from "@/lib/utils";
import { useAssistantStore } from "@/stores/assistant-store";
import { useEngineStore } from "@/stores/engine-store";
import { useFormStore } from "@/stores/form-store";
import { useJobStore } from "@/stores/job-store";
import { useSettingsStore } from "@/stores/settings-store";

import { ControlnetSection } from "./controlnet-section";
import { FaceDetailerSection } from "./face-detailer-section";
import { Field, NumberInput, SliderNumber } from "./fields";

const BUILTIN_VAE = "__builtin__";

/** 値と表示名のペアから `Select` を作る(未選択時は placeholder)。 */
function Choice({
  id,
  value,
  options,
  placeholder,
  onChange,
  disabled,
  invalid,
}: {
  id?: string;
  value: string;
  options: { value: string; label: string }[];
  placeholder?: string;
  onChange: (v: string) => void;
  disabled?: boolean;
  invalid?: boolean;
}) {
  return (
    <Select
      value={value === "" ? null : value}
      items={options}
      disabled={disabled}
      onValueChange={(v) => v !== null && onChange(v)}
    >
      <SelectTrigger id={id} className="w-full" aria-invalid={invalid || undefined}>
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

const asOptions = (names: string[]) => names.map((n) => ({ value: n, label: n }));

export function GenerationForm() {
  const t = useT();
  const errorMessage = useErrorMessage();

  const { params, count, hydrated } = useFormStore();
  const { hydrate, patch, patchModel, setKind, setCount, applyModels } = useFormStore.getState();

  const status = useEngineStore((s) => s.status);
  const models = useEngineStore((s) => s.models);
  const enqueue = useJobStore((s) => s.enqueue);
  const settingsLoaded = useSettingsStore((s) => s.loaded);
  const saved = useSettingsStore((s) => s.settings.generationDefaults);

  const [submitting, setSubmitting] = useState(false);
  const [fieldError, setFieldError] = useState<AppError | null>(null);

  const kind = params.model.kind;
  const flux = isFlux(kind);
  const err = (field: string) =>
    fieldError?.code === "VALIDATION" && fieldError.details?.field === field ? fieldError.message : null;

  // 設定(DB)から下書きを復元する(初回のみ)
  useEffect(() => {
    if (settingsLoaded) hydrate(saved);
  }, [settingsLoaded, saved, hydrate]);

  // モデル一覧に合わせて未選択の項目を埋める
  useEffect(() => {
    if (hydrated) applyModels(models);
  }, [hydrated, models, kind, applyModels]);

  // 下書きを DB へ自動保存(入力が止まって 0.8 秒後)
  const firstSave = useRef(true);
  useEffect(() => {
    if (!hydrated) return;
    if (firstSave.current) {
      firstSave.current = false; // 復元直後の保存は不要
      return;
    }
    const timer = setTimeout(() => {
      void useSettingsStore.getState().update({ generationDefaults: params }).catch(() => {});
    }, 800);
    return () => clearTimeout(timer);
  }, [params, hydrated]);

  const modelNames = useMemo(() => modelsFor(kind, models), [kind, models]);
  const vaeOptions = useMemo(() => {
    const list = asOptions(models?.vaes ?? []);
    return flux ? list : [{ value: BUILTIN_VAE, label: t("generate.vae.builtin") }, ...list];
  }, [models, flux, t]);

  // ControlNet を有効にしている間は参照画像が必要
  const cnParams = params.controlnet;
  const needsReference =
    cnParams?.enabled === true && supportsControlnet(kind) && !cnParams.units[0]?.image;

  const ready =
    status?.connected === true &&
    !needsReference &&
    params.prompt.trim() !== "" &&
    params.model.name !== "" &&
    (!flux || (params.model.clip.length === 2 && params.model.clip.every(Boolean) && !!params.model.vae));

  async function submit() {
    if (!ready || submitting) return;
    setSubmitting(true);
    setFieldError(null);
    try {
      await enqueue(params, count);
      toast.success(t("generate.enqueued"));
    } catch (e) {
      const appError = toAppError(e);
      setFieldError(appError);
      toast.error(errorMessage(appError));
      // モデルが無い(削除・移動された等): 一覧を取り直してフォームの選択を最新化する
      if (appError.code === "ENGINE_MODEL_MISSING") void useEngineStore.getState().loadModels();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
      onKeyDown={(e) => {
        if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
          e.preventDefault();
          void submit();
        }
      }}
    >
      <PresetBar />

      <div className="-mb-2 flex justify-end">
        <Button type="button" size="xs" variant="outline" onClick={() => useAssistantStore.getState().setPanelOpen(true)}>
          {t("assistant.open")}
        </Button>
      </div>

      <Field label={t("generate.prompt")} htmlFor="prompt" error={err("prompt")}>
        <Textarea
          id="prompt"
          rows={4}
          value={params.prompt}
          placeholder={t("generate.promptPlaceholder")}
          aria-invalid={err("prompt") ? true : undefined}
          onChange={(e) => patch({ prompt: e.target.value })}
        />
      </Field>

      {flux ? (
        <p className="rounded-lg border bg-muted/40 p-2 text-xs text-muted-foreground">{t("generate.fluxHint")}</p>
      ) : (
        <Field label={t("generate.negative")} htmlFor="negative">
          <Textarea
            id="negative"
            rows={2}
            value={params.negativePrompt}
            onChange={(e) => patch({ negativePrompt: e.target.value })}
          />
        </Field>
      )}

      <div className="grid grid-cols-2 gap-3">
        <Field label={t("generate.kind")} htmlFor="kind">
          <Choice
            id="kind"
            value={kind}
            options={MODEL_KINDS.map((k) => ({ value: k, label: t(`generate.kind.${k}` as MessageKey) }))}
            onChange={(v) => setKind(v as ModelKind)}
          />
        </Field>
        <Field label={t("generate.model")} htmlFor="model" error={err("model")}>
          <Choice
            id="model"
            value={params.model.name}
            options={asOptions(modelNames)}
            placeholder={modelNames.length === 0 ? t("generate.model.none") : t("generate.model.placeholder")}
            disabled={modelNames.length === 0}
            invalid={!!err("model")}
            onChange={(v) => patchModel({ name: v })}
          />
        </Field>
      </div>
      {modelNames.length === 0 && status?.connected && (
        <div className="flex flex-col gap-1">
          <p className="text-xs text-muted-foreground">{t("generate.model.none")}</p>
          {/* Flux 系の取得は未対応(docs/12 §1)。SD 1.5 / SDXL のときだけ案内する */}
          {(kind === "sd15" || kind === "sdxl") && <ModelsLink category="checkpoint" kind={kind} />}
        </div>
      )}
      {kind === "flux_gguf" && status?.connected && !status.features.ggufNode && (
        <p className="rounded-lg border border-amber-500/40 bg-amber-500/10 p-2 text-sm">{t("engine.gguf.missing")}</p>
      )}

      {flux && (
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("generate.clip.t5")} htmlFor="clip-t5" error={err("clip")}>
            <Choice
              id="clip-t5"
              value={params.model.clip[0] ?? ""}
              options={asOptions(models?.clips ?? [])}
              placeholder={t("generate.model.placeholder")}
              onChange={(v) => patchModel({ clip: [v, params.model.clip[1] ?? ""] })}
            />
          </Field>
          <Field label={t("generate.clip.l")} htmlFor="clip-l">
            <Choice
              id="clip-l"
              value={params.model.clip[1] ?? ""}
              options={asOptions(models?.clips ?? [])}
              placeholder={t("generate.model.placeholder")}
              onChange={(v) => patchModel({ clip: [params.model.clip[0] ?? "", v] })}
            />
          </Field>
        </div>
      )}

      {(flux || vaeOptions.length > 1) && (
        <Field label={t("generate.vae")} htmlFor="vae" error={err("vae")}>
          <Choice
            id="vae"
            value={params.model.vae ?? (flux ? "" : BUILTIN_VAE)}
            options={vaeOptions}
            placeholder={t("generate.model.placeholder")}
            onChange={(v) => patchModel({ vae: v === BUILTIN_VAE ? null : v })}
          />
        </Field>
      )}

      <Field label={t("generate.size")} error={err("width") ?? err("height")}>
        <div className="flex items-center gap-2">
          <NumberInput
            id="width"
            value={params.width}
            min={256}
            max={2048}
            step={64}
            invalid={!!err("width")}
            onChange={(n) => patch({ width: n })}
          />
          <span aria-hidden>×</span>
          <NumberInput
            id="height"
            value={params.height}
            min={256}
            max={2048}
            step={64}
            invalid={!!err("height")}
            onChange={(n) => patch({ height: n })}
          />
        </div>
        <div className="flex flex-wrap gap-1.5">
          {sizePresets(kind).map(({ w, h }) => {
            const active = params.width === w && params.height === h;
            return (
              <Button
                key={`${w}x${h}`}
                type="button"
                size="xs"
                variant={active ? "default" : "outline"}
                onClick={() => patch({ width: w, height: h })}
              >
                {w}×{h}
              </Button>
            );
          })}
        </div>
      </Field>

      <Field label={t("generate.steps")} htmlFor="steps" error={err("steps")}>
        <SliderNumber id="steps" value={params.steps} min={1} max={60} step={1} invalid={!!err("steps")} onChange={(n) => patch({ steps: n })} />
      </Field>

      {flux ? (
        <Field label={t("generate.guidance")} htmlFor="guidance" error={err("guidance")}>
          <SliderNumber id="guidance" value={params.guidance} min={0} max={10} step={0.1} onChange={(n) => patch({ guidance: Math.round(n * 10) / 10 })} />
        </Field>
      ) : (
        <Field label={t("generate.cfg")} htmlFor="cfg" error={err("cfg")}>
          <SliderNumber id="cfg" value={params.cfg} min={1} max={20} step={0.5} onChange={(n) => patch({ cfg: n })} />
        </Field>
      )}

      <div className="grid grid-cols-2 gap-3">
        <Field label={t("generate.sampler")} htmlFor="sampler" error={err("sampler")}>
          <Choice id="sampler" value={params.sampler} options={asOptions(models?.samplers ?? [params.sampler])} onChange={(v) => patch({ sampler: v })} />
        </Field>
        <Field label={t("generate.scheduler")} htmlFor="scheduler" error={err("scheduler")}>
          <Choice id="scheduler" value={params.scheduler} options={asOptions(models?.schedulers ?? [params.scheduler])} onChange={(v) => patch({ scheduler: v })} />
        </Field>
      </div>

      <Field label={t("generate.seed")} htmlFor="seed" error={err("seed")}>
        <div className="flex items-center gap-2">
          <NumberInput
            id="seed"
            value={params.seed}
            min={0}
            max={MAX_SEED}
            step={1}
            disabled={params.seedRandomize}
            invalid={!!err("seed")}
            onChange={(n) => patch({ seed: Math.trunc(n) })}
          />
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t("generate.seed.shuffle")}
            title={t("generate.seed.shuffle")}
            disabled={params.seedRandomize}
            onClick={() => patch({ seed: randomSeed() })}
          >
            <Dices />
          </Button>
        </div>
        <label className="flex items-center gap-2 text-sm">
          <Switch checked={params.seedRandomize} onCheckedChange={(v) => patch({ seedRandomize: v })} />
          {t("generate.seed.random")}
        </label>
      </Field>

      <ControlnetSection error={err} />

      <FaceDetailerSection error={err} />

      <details className="rounded-lg border p-3">
        <summary className="cursor-pointer text-sm font-medium">{t("generate.advanced")}</summary>
        <div className="mt-3">
          <Field label={t("generate.batch")} htmlFor="batch" error={err("batchSize")}>
            <NumberInput id="batch" value={params.batchSize} min={1} max={8} step={1} invalid={!!err("batchSize")} onChange={(n) => patch({ batchSize: Math.trunc(n) })} />
          </Field>
        </div>
      </details>

      <div className="sticky bottom-0 -mx-1 flex items-end gap-3 border-t bg-background/95 px-1 pb-1 pt-3 backdrop-blur">
        <div className="w-24 shrink-0">
          <Field label={t("generate.count")} htmlFor="count" error={err("count")}>
            <NumberInput
              id="count"
              value={count}
              min={1}
              max={MAX_ENQUEUE_COUNT}
              step={1}
              onChange={(n) => setCount(Math.min(Math.max(Math.trunc(n), 1), MAX_ENQUEUE_COUNT))}
            />
          </Field>
        </div>
        <Button type="submit" className="flex-1" disabled={!ready || submitting}>
          {submitting ? t("generate.submitting") : t("generate.submit")}
        </Button>
      </div>
      <p className={cn("-mt-2 text-xs text-muted-foreground")}>{t("generate.shortcut")}</p>
    </form>
  );
}
