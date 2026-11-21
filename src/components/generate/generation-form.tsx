"use client";

import { Dices } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { ModelsLink } from "@/components/models/models-link";
import { PresetBar } from "@/components/presets/preset-bar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import {
  isFlux,
  isSplit,
  MAX_SEED,
  MODEL_KINDS,
  clipsForKind,
  defaultNegative,
  installedKinds,
  modelsForKind,
  vaesForKind,
  needsExternalVae,
  randomSeed,
  sizePresets,
  supportsControlnet,
  supportsImg2img,
  supportsInpaint,
  usesNegative,
} from "@/lib/generation/defaults";
import { img2imgReady } from "@/lib/generation/img2img";
import { inpaintReady } from "@/lib/generation/inpaint";
import { referenceReady, supportsReference } from "@/lib/generation/reference";
import type { MessageKey } from "@/lib/i18n";
import { useErrorMessage, useT } from "@/lib/i18n/use-t";
import { type CandidateBadge, candidateBadges } from "@/lib/models/profile";
import { toAppError } from "@/lib/tauri/errors";
import { MAX_ENQUEUE_COUNT, type ModelKind } from "@/lib/tauri/jobs";
import { cn } from "@/lib/utils";
import { TranslateBox } from "@/components/generate/translate-box";
import { useAssistantStore } from "@/stores/assistant-store";
import { useEngineStore } from "@/stores/engine-store";
import { fieldErrorOf, useFormErrorStore } from "@/stores/form-error-store";
import { useFormStore } from "@/stores/form-store";
import { useJobStore } from "@/stores/job-store";
import { useModelsStore } from "@/stores/models-store";
import { useSettingsStore } from "@/stores/settings-store";

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
  options: { value: string; label: string; badges?: CandidateBadge[]; disabled?: boolean }[];
  placeholder?: string;
  onChange: (v: string) => void;
  disabled?: boolean;
  invalid?: boolean;
}) {
  const t = useT();
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
          <SelectItem key={o.value} value={o.value} disabled={o.disabled}>
            {o.label}
            {o.badges?.map((b) => (
              <Badge key={b} variant="secondary" className="ml-1.5">
                {t(`modelProfile.badge.${b}` as MessageKey)}
              </Badge>
            ))}
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
  const fieldError = useFormErrorStore((s) => s.error);
  const setFieldError = useFormErrorStore.getState().setError;
  const [resetOpen, setResetOpen] = useState(false);

  // モデルの説明・絞り込みに使うカタログ。取得ページと同じものを読む(docs/18、20)
  const catalog = useModelsStore((s) => s.view?.catalog);
  useEffect(() => {
    if (!useModelsStore.getState().view) void useModelsStore.getState().load();
  }, []);

  const kind = params.model.kind;
  const flux = isFlux(kind);
  // Anima / Qwen-Image 2.1 / Krea 2 は、テキストエンコーダ 1 つと VAE を別に選ぶ(docs/17 §3)
  const split = isSplit(kind);
  const externalVae = needsExternalVae(kind);
  const err = fieldErrorOf(fieldError);

  // 設定(DB)から下書きを復元する(初回のみ)
  useEffect(() => {
    if (settingsLoaded) hydrate(saved);
  }, [settingsLoaded, saved, hydrate]);

  // モデル一覧に合わせて未選択の項目を埋める
  useEffect(() => {
    if (hydrated) applyModels(models);
  }, [hydrated, models, kind, catalog, applyModels]);

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

  // 種別に合うモデル・エンコーダ・VAE だけを選択肢にする(docs/20 §2)
  const modelNames = useMemo(() => modelsForKind(kind, models, catalog), [kind, models, catalog]);
  const clipNames = useMemo(() => clipsForKind(kind, models, catalog), [kind, models, catalog]);
  const vaeNames = useMemo(() => vaesForKind(kind, models, catalog), [kind, models, catalog]);
  const installed = useMemo(() => installedKinds(models, catalog), [models, catalog]);
  const kindOptions = useMemo(
    () =>
      MODEL_KINDS.map((k) => {
        const missing = !installed.has(k) && k !== kind && models !== null;
        const label = t(`generate.kind.${k}` as MessageKey);
        return { value: k, label: missing ? `${label}(${t("generate.kind.notInstalled")})` : label, disabled: missing };
      }),
    [installed, kind, models, t],
  );
  const modelOptions = useMemo(
    () => modelNames.map((n) => ({ value: n, label: n, badges: candidateBadges(catalog, n) })),
    [modelNames, catalog],
  );
  const vaeOptions = useMemo(() => {
    const list = asOptions(vaeNames);
    return externalVae ? list : [{ value: BUILTIN_VAE, label: t("generate.vae.builtin") }, ...list];
  }, [vaeNames, externalVae, t]);

  // ControlNet を有効にしている間は参照画像が必要
  const cnParams = params.controlnet;
  const needsReference =
    cnParams?.enabled === true && supportsControlnet(kind) && !cnParams.units[0]?.image;

  // Inpaint を有効にしている間は、参照画像とマスクが必要
  const inpaintOn = params.inpaint?.enabled === true && supportsInpaint(kind);
  const needsInpaintInput = inpaintOn && !inpaintReady(params.inpaint);

  // 画像から生成を有効にしている間は、元画像が必要(生成サイズも元画像から決まる)
  const img2imgOn = params.img2img?.enabled === true && supportsImg2img(kind);
  const needsImg2imgInput = img2imgOn && !img2imgReady(params.img2img);
  // 参照画像つき生成(Qwen-Image 2.1)を有効にしている間は、参照画像が 1 枚以上必要(出力サイズは 1 枚目から決まる)
  const referenceOn = params.reference?.enabled === true && supportsReference(kind);
  const needsReferenceImages = referenceOn && !referenceReady(params.reference);
  const sizeLocked = inpaintOn || img2imgOn || referenceOn;

  const ready =
    status?.connected === true &&
    !needsReference &&
    !needsInpaintInput &&
    !needsImg2imgInput &&
    !needsReferenceImages &&
    params.prompt.trim() !== "" &&
    params.model.name !== "" &&
    (!flux || (params.model.clip.length === 2 && params.model.clip.every(Boolean) && !!params.model.vae)) &&
    (!split || (params.model.clip.length === 1 && !!params.model.clip[0] && !!params.model.vae));

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

  // プロンプトのクリアは確認なし。直前の内容はトーストから戻せる(docs/21 §2.2)
  function clearPrompt() {
    const previous = params.prompt;
    if (previous === "") return;
    patch({ prompt: "" });
    toast(t("generate.reset.prompt.done"), {
      duration: 8000,
      action: { label: t("generate.reset.undo"), onClick: () => useFormStore.getState().patch({ prompt: previous }) },
    });
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

      <div className="-mb-2 flex justify-end gap-2">
        <Button type="button" size="xs" variant="outline" onClick={() => setResetOpen(true)}>
          {t("generate.reset.all")}
        </Button>
        <Button type="button" size="xs" variant="outline" onClick={() => useAssistantStore.getState().setPanelOpen(true)}>
          {t("assistant.open")}
        </Button>
      </div>

      <TranslateBox params={params} />

      <Field
        label={t("generate.prompt")}
        htmlFor="prompt"
        error={err("prompt")}
        help={t("help.prompt")}
        action={
          <Button type="button" size="xs" variant="ghost" disabled={params.prompt === ""} onClick={clearPrompt}>
            {t("generate.reset.prompt")}
          </Button>
        }
      >
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
      ) : !usesNegative(kind) ? (
        <p className="rounded-lg border bg-muted/40 p-2 text-xs text-muted-foreground">{t("generate.noNegativeHint")}</p>
      ) : (
        <Field
          label={t("generate.negative")}
          htmlFor="negative"
          help={t("help.negative")}
          action={
            <Button
              type="button"
              size="xs"
              variant="ghost"
              disabled={params.negativePrompt === defaultNegative(kind)}
              onClick={() => useFormStore.getState().resetNegative()}
            >
              {t("generate.reset.negative")}
            </Button>
          }
        >
          <Textarea
            id="negative"
            rows={2}
            value={params.negativePrompt}
            onChange={(e) => patch({ negativePrompt: e.target.value })}
          />
        </Field>
      )}

      <div className="grid grid-cols-2 gap-3">
        <Field label={t("generate.kind")} htmlFor="kind" help={t("help.kind")}>
          <Choice
            id="kind"
            value={kind}
            options={kindOptions}
            onChange={(v) => setKind(v as ModelKind)}
          />
        </Field>
        <Field label={t("generate.model")} htmlFor="model" error={err("model")} help={t("help.model")}>
          <Choice
            id="model"
            value={params.model.name}
            options={modelOptions}
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
          {/* 取得できるのは SD 1.5 / SDXL のチェックポイントと、Flux(GGUF)。Flux 本体(safetensors)は対象外(docs/12 §1) */}
          {(kind === "sd15" || kind === "sdxl") && <ModelsLink category="checkpoint" kind={kind} />}
          {kind === "flux_gguf" && <ModelsLink category="flux" />}
          {isSplit(kind) && <ModelsLink category={kind} />}
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
              options={asOptions(clipNames)}
              placeholder={t("generate.model.placeholder")}
              onChange={(v) => patchModel({ clip: [v, params.model.clip[1] ?? ""] })}
            />
          </Field>
          <Field label={t("generate.clip.l")} htmlFor="clip-l">
            <Choice
              id="clip-l"
              value={params.model.clip[1] ?? ""}
              options={asOptions(clipNames)}
              placeholder={t("generate.model.placeholder")}
              onChange={(v) => patchModel({ clip: [params.model.clip[0] ?? "", v] })}
            />
          </Field>
        </div>
      )}

      {split && (
        <Field label={t("generate.clip.single")} htmlFor="clip-te" error={err("clip")}>
          <Choice
            id="clip-te"
            value={params.model.clip[0] ?? ""}
            options={asOptions(clipNames)}
            placeholder={t("generate.model.placeholder")}
            invalid={!!err("clip")}
            onChange={(v) => patchModel({ clip: [v] })}
          />
        </Field>
      )}

      {/* テキストエンコーダ・VAE が無いときも、取得ページへ案内する */}
      {isSplit(kind) &&
        status?.connected &&
        (clipNames.length === 0 || vaeNames.length === 0) && <ModelsLink category={kind} />}
      {kind === "flux_gguf" &&
        status?.connected &&
        (clipNames.length < 2 || vaeNames.length === 0) && (
          <ModelsLink category="flux" />
        )}

      {(externalVae || vaeOptions.length > 1) && (
        <Field label={t("generate.vae")} htmlFor="vae" error={err("vae")}>
          <Choice
            id="vae"
            value={params.model.vae ?? (externalVae ? "" : BUILTIN_VAE)}
            options={vaeOptions}
            placeholder={t("generate.model.placeholder")}
            onChange={(v) => patchModel({ vae: v === BUILTIN_VAE ? null : v })}
          />
        </Field>
      )}

      <Field label={t("generate.size")} error={err("width") ?? err("height")} help={t("help.size")}>
        <div className="flex items-center gap-2">
          <NumberInput
            id="width"
            value={params.width}
            min={256}
            max={2048}
            step={64}
            invalid={!!err("width")}
            disabled={sizeLocked}
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
            disabled={sizeLocked}
            onChange={(n) => patch({ height: n })}
          />
        </div>
        <div className="flex flex-wrap gap-1.5" hidden={sizeLocked}>
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

      <Field label={t("generate.steps")} htmlFor="steps" error={err("steps")} help={t("help.steps")}>
        <SliderNumber id="steps" value={params.steps} min={1} max={60} step={1} invalid={!!err("steps")} onChange={(n) => patch({ steps: n })} />
      </Field>

      {flux ? (
        <Field label={t("generate.guidance")} htmlFor="guidance" error={err("guidance")} help={t("help.guidance")}>
          <SliderNumber id="guidance" value={params.guidance} min={0} max={10} step={0.1} onChange={(n) => patch({ guidance: Math.round(n * 10) / 10 })} />
        </Field>
      ) : (
        <Field label={t("generate.cfg")} htmlFor="cfg" error={err("cfg")} help={t("help.cfg")}>
          <SliderNumber id="cfg" value={params.cfg} min={1} max={20} step={0.5} onChange={(n) => patch({ cfg: n })} />
        </Field>
      )}

      <div className="grid grid-cols-2 gap-3">
        <Field label={t("generate.sampler")} htmlFor="sampler" error={err("sampler")} help={t("help.sampler")}>
          <Choice id="sampler" value={params.sampler} options={asOptions(models?.samplers ?? [params.sampler])} onChange={(v) => patch({ sampler: v })} />
        </Field>
        <Field label={t("generate.scheduler")} htmlFor="scheduler" error={err("scheduler")} help={t("help.scheduler")}>
          <Choice id="scheduler" value={params.scheduler} options={asOptions(models?.schedulers ?? [params.scheduler])} onChange={(v) => patch({ scheduler: v })} />
        </Field>
      </div>

      <Field label={t("generate.seed")} htmlFor="seed" error={err("seed")} help={t("help.seed")}>
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

      <details className="rounded-lg border p-3">
        <summary className="cursor-pointer text-sm font-medium">{t("generate.advanced")}</summary>
        <div className="mt-3">
          <Field label={t("generate.batch")} htmlFor="batch" error={err("batchSize")} help={t("help.batch")}>
            <NumberInput id="batch" value={params.batchSize} min={1} max={8} step={1} invalid={!!err("batchSize")} onChange={(n) => patch({ batchSize: Math.trunc(n) })} />
          </Field>
        </div>
      </details>

      <div className="sticky bottom-0 -mx-1 flex items-end gap-3 border-t bg-background/95 px-1 pb-1 pt-3 backdrop-blur">
        <div className="w-24 shrink-0">
          <Field label={t("generate.count")} htmlFor="count" error={err("count")} help={t("help.count")}>
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
      {(needsReference || needsInpaintInput || needsImg2imgInput || needsReferenceImages) && (
        <p role="status" className="-mt-2 text-xs text-amber-600 dark:text-amber-400">
          {needsReference
            ? t("generate.cn.needImage")
            : needsReferenceImages
              ? t("generate.reference.needImage")
              : needsImg2imgInput
              ? t("generate.img2img.needImage")
              : params.inpaint?.image
              ? t("generate.inpaint.needMask")
              : t("generate.inpaint.needImage")}
        </p>
      )}
      <p className={cn("-mt-2 text-xs text-muted-foreground")}>{t("generate.shortcut")}</p>

      <ConfirmDialog
        open={resetOpen}
        title={t("generate.reset.all.title")}
        description={t("generate.reset.all.desc")}
        confirmLabel={t("generate.reset.all.confirm")}
        cancelLabel={t("generate.reset.all.cancel")}
        onOpenChange={setResetOpen}
        onConfirm={() => {
          useFormStore.getState().resetAll();
          toast.success(t("generate.reset.all.done"));
        }}
      />
    </form>
  );
}
