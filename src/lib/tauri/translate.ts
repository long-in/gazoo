import type { AssistantProvider } from "./assistant";
import { invoke } from "./invoke";
import type { GenerationParams } from "./jobs";

/**
 * プロンプト翻訳(docs/34-prompt-translate.md、docs/04-ipc-api.md「プロンプト翻訳」)。
 * プロバイダ・モデルは AI サポートとは別設定。キー・同意・ローカル LLM は AI サポートと共有する。
 */
export interface TranslateStatus {
  provider: AssistantProvider;
  model: string;
  /** 今すぐ実行できるか。 */
  ready: boolean;
  /** 実行できない理由(`ready = false` のとき)。 */
  reason: "no_key" | "no_consent" | "local_not_ready" | "error" | null;
}

export interface TranslatePatch {
  provider?: AssistantProvider;
  model?: string;
}

/** 翻訳先のモデル(フォームの選択中のもの)。 */
export interface TranslateTarget {
  kind: GenerationParams["model"]["kind"] | null;
  /** カタログのファミリー ID(カタログ外なら null)。 */
  familyId: string | null;
  /** 参照画像つき生成か。 */
  reference: boolean;
}

/** 入力の最大文字数(Rust の `MAX_INPUT_CHARS` と同じ)。 */
export const MAX_TRANSLATE_INPUT_CHARS = 2000;

export const translateStatus = () =>
  invoke<TranslateStatus>("translate_status");

export const translateUpdate = (patch: TranslatePatch) =>
  invoke<TranslateStatus>("translate_update", { patch });

/** 翻訳・校正した本文を返す(中断されたら null)。 */
export const translateRun = (
  requestId: string,
  text: string,
  target: TranslateTarget,
) =>
  invoke<{ text: string | null }>("translate_run", {
    requestId,
    text,
    target,
  }).then((r) => r.text);

export const translateCancel = (requestId: string) =>
  invoke<void>("translate_cancel", { requestId });
