"use client";

import { useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { imageSrc } from "@/lib/image-src";
import type { MessageKey } from "@/lib/i18n";
import { useLanguage, useT } from "@/lib/i18n/use-t";
import { profileFor } from "@/lib/models/profile";
import { estimateMsPerImage, formatDuration, promptHistory } from "@/lib/queue";
import { type InputImage, inputImageGet } from "@/lib/tauri/inputs";
import { cn } from "@/lib/utils";
import { useFormStore } from "@/stores/form-store";
import { useJobStore } from "@/stores/job-store";
import { useModelsStore } from "@/stores/models-store";

import { ExtrasPanel } from "./extras-panel";
import { MaskEditor } from "./mask-editor";
import { ModelProfilePanel } from "./model-profile-panel";

/**
 * 生成画面の右側のワークスペース(docs/20 §4.2)。モデルの説明・参照画像・この設定の目安・プロンプト履歴。
 * 実行状況と生成結果は、画面共通のヘッダとキュー画面・ギャラリーで見る。
 */
export function Workspace() {
  return (
    <div className="flex flex-col gap-4">
      <ExtrasPanel />
      <ModelProfilePanel />
      <ReferenceWorkspace />
      <EstimatePanel />
      <PromptHistoryPanel />
    </div>
  );
}

type RefTab = "img2img" | "controlnet" | "inpaint";

/** 参照画像(画像から生成・ControlNet・Inpaint)を大きく表示する。複数あるときはタブで切り替える。 */
function ReferenceWorkspace() {
  const t = useT();
  const params = useFormStore((s) => s.params);
  const [tab, setTab] = useState<RefTab | null>(null);
  const [editing, setEditing] = useState(false);

  const sources = useMemo(() => {
    const list: { id: RefTab; path: string }[] = [];
    if (params.img2img?.enabled && params.img2img.image) list.push({ id: "img2img", path: params.img2img.image });
    const cn = params.controlnet;
    if (cn?.enabled && cn.units[0]?.image) list.push({ id: "controlnet", path: cn.units[0].image });
    if (params.inpaint?.enabled && params.inpaint.image) list.push({ id: "inpaint", path: params.inpaint.image });
    return list;
  }, [params.img2img, params.controlnet, params.inpaint]);

  const active = sources.find((s) => s.id === tab) ?? sources[0];
  const mask = active?.id === "inpaint" ? params.inpaint?.mask : undefined;

  // 画像の情報(表示用)。取得できなければ何も出さない(各セクションが欠損を知らせる)
  const [info, setInfo] = useState<{ path: string; img: InputImage } | null>(null);
  const [maskInfo, setMaskInfo] = useState<InputImage | null>(null);
  const activePath = active?.path;
  useEffect(() => {
    if (!activePath) return;
    let cancelled = false;
    inputImageGet(activePath)
      .then((img) => !cancelled && setInfo({ path: activePath, img }))
      .catch(() => !cancelled && setInfo(null));
    return () => {
      cancelled = true;
    };
  }, [activePath]);
  useEffect(() => {
    if (!mask) return;
    let cancelled = false;
    inputImageGet(mask)
      .then((img) => !cancelled && setMaskInfo(img))
      .catch(() => !cancelled && setMaskInfo(null));
    return () => {
      cancelled = true;
    };
  }, [mask]);

  if (!active) return null;
  const shown = info && info.path === active.path ? info.img : null;

  return (
    <section className="rounded-lg border p-3" aria-labelledby="ref-title">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 id="ref-title" className="text-sm font-medium">
          {t("workspace.reference.title")}
        </h2>
        {sources.length > 1 && (
          <div role="tablist" className="flex gap-1">
            {sources.map((s) => (
              <Button
                key={s.id}
                type="button"
                role="tab"
                aria-selected={active.id === s.id}
                size="xs"
                variant={active.id === s.id ? "default" : "outline"}
                onClick={() => setTab(s.id)}
              >
                {t(`workspace.reference.${s.id}` as MessageKey)}
              </Button>
            ))}
          </div>
        )}
      </div>

      {shown ? (
        <div className="mt-3 flex flex-col gap-2">
          <div className="relative overflow-hidden rounded-lg border bg-muted/30">
            <img
              src={imageSrc(shown.filePath)}
              alt={t(`workspace.reference.${active.id}` as MessageKey)}
              className="max-h-[55vh] w-full object-contain"
            />
            {mask && maskInfo && (
              // マスクを赤く重ねる(白 = 描き直す範囲)
              <div
                aria-hidden
                className="pointer-events-none absolute inset-0 bg-red-500/50"
                style={{
                  maskImage: `url(${imageSrc(maskInfo.filePath)})`,
                  WebkitMaskImage: `url(${imageSrc(maskInfo.filePath)})`,
                  maskSize: "contain",
                  WebkitMaskSize: "contain",
                  maskRepeat: "no-repeat",
                  WebkitMaskRepeat: "no-repeat",
                  maskPosition: "center",
                  WebkitMaskPosition: "center",
                  maskMode: "luminance",
                }}
              />
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            {t("workspace.reference.size")
              .replace("{src}", `${shown.width}×${shown.height}`)
              .replace("{out}", `${params.width}×${params.height}`)}
          </p>
          {active.id === "inpaint" && (
            <div>
              <Button type="button" size="sm" variant="outline" onClick={() => setEditing(true)}>
                {t(mask ? "generate.inpaint.editMask" : "generate.inpaint.drawMask")}
              </Button>
            </div>
          )}
        </div>
      ) : (
        <p className="mt-3 text-xs text-muted-foreground">{t("workspace.reference.loading")}</p>
      )}

      {active.id === "inpaint" && shown && (
        <MaskEditor
          open={editing}
          onOpenChange={setEditing}
          image={shown}
          existingMask={mask ?? ""}
          onSaved={(path) => useFormStore.getState().patchInpaint({ mask: path })}
        />
      )}
    </section>
  );
}

/** 選択中の設定の、過去の実績からの時間の目安(docs/20 §4.2)。 */
function EstimatePanel() {
  const t = useT();
  const params = useFormStore((s) => s.params);
  const count = useFormStore((s) => s.count);
  const jobs = useJobStore((s) => s.jobs);
  const catalog = useModelsStore((s) => s.view?.catalog);

  const perImage = useMemo(
    () => estimateMsPerImage(jobs, params),
    // 目安に効く項目だけで再計算する
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [jobs, params.model.kind, params.width, params.height],
  );
  const speed = profileFor(catalog, params.model.kind, params.model.name)?.profile.resources.speed;
  const images = Math.max(1, params.batchSize) * Math.max(1, count);

  return (
    <section className="rounded-lg border p-3" aria-labelledby="est-title">
      <h2 id="est-title" className="text-sm font-medium">
        {t("workspace.estimate.title")}
      </h2>
      <p className="mt-1.5 text-sm">
        {perImage !== null ? (
          <>
            {t("workspace.estimate.perImage").replace("{t}", formatDuration(perImage))}
            <span className="block text-xs text-muted-foreground">
              {t("workspace.estimate.total")
                .replace("{n}", String(images))
                .replace("{t}", formatDuration(perImage * images))}
            </span>
          </>
        ) : speed ? (
          <>
            {t("workspace.estimate.speedOnly").replace("{speed}", t(`modelProfile.speed.${speed}` as MessageKey))}
            <span className="block text-xs text-muted-foreground">{t("workspace.estimate.noHistory")}</span>
          </>
        ) : (
          <span className="text-muted-foreground">{t("workspace.estimate.noHistory")}</span>
        )}
      </p>
    </section>
  );
}

/** 過去に投入したプロンプト(ジョブの履歴から)。クリックで入力欄へ入れる(docs/20 Q-99)。 */
function PromptHistoryPanel() {
  const t = useT();
  useLanguage();
  const jobs = useJobStore((s) => s.jobs);
  const items = useMemo(() => promptHistory(jobs, 20), [jobs]);
  const { patch } = useFormStore.getState();

  return (
    <section className="rounded-lg border p-3" aria-labelledby="hist-title">
      <h2 id="hist-title" className="text-sm font-medium">
        {t("workspace.history.title")}
      </h2>
      {items.length === 0 ? (
        <p className="mt-1.5 text-xs text-muted-foreground">{t("workspace.history.empty")}</p>
      ) : (
        <ul className="mt-2 flex max-h-72 flex-col gap-1.5 overflow-y-auto">
          {items.map((h) => (
            <li key={h.prompt} className={cn("flex items-start gap-2 rounded-md border p-2")}>
              <p className="line-clamp-2 min-w-0 flex-1 text-xs">{h.prompt}</p>
              <div className="flex shrink-0 gap-1">
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  onClick={() => {
                    const cur = useFormStore.getState().params.prompt.trim();
                    patch({ prompt: cur === "" ? h.prompt : `${cur}, ${h.prompt}` });
                  }}
                >
                  {t("workspace.history.append")}
                </Button>
                <Button
                  type="button"
                  size="xs"
                  onClick={() => patch({ prompt: h.prompt, negativePrompt: h.negativePrompt })}
                >
                  {t("workspace.history.replace")}
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
