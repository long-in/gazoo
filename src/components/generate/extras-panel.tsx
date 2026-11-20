"use client";

import { useT } from "@/lib/i18n/use-t";
import { fieldErrorOf, useFormErrorStore } from "@/stores/form-error-store";

import { ControlnetSection } from "./controlnet-section";
import { FaceDetailerSection } from "./face-detailer-section";
import { Img2imgSection } from "./img2img-section";
import { InpaintSection } from "./inpaint-section";
import { ReferenceSection } from "./reference-section";

/**
 * 拡張機能(参照画像・画像から生成・Inpaint・ControlNet・顔補正)。
 * 生成フォームの縦を短くするため、右側のワークスペースに置く(畳める。docs/20 §4.2)。
 */
export function ExtrasPanel() {
  const t = useT();
  const err = fieldErrorOf(useFormErrorStore((s) => s.error));
  return (
    <details open className="flex flex-col gap-4 rounded-lg border p-3 [&[open]>summary]:mb-1">
      <summary className="cursor-pointer text-sm font-medium">{t("generate.extras")}</summary>
      <ReferenceSection error={err} />
      <Img2imgSection error={err} />
      <InpaintSection error={err} />
      <ControlnetSection error={err} />
      <FaceDetailerSection error={err} />
    </details>
  );
}
