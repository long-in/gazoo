import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

import { invoke } from "./invoke";

/** モデル取得(docs/12-model-downloader.md、docs/04-ipc-api.md「モデル取得」)。型は Rust 側が正。 */

export interface Localized {
  ja: string;
  en: string;
}

export type ModelCategory = "checkpoint" | "controlnet" | "detector";
export type BaseModel = "sd15" | "sdxl";

export interface ModelVariant {
  id: string;
  label: Localized;
  baseModel?: BaseModel;
  precision?: "fp16" | "fp32";
  recommended: boolean;
  source: { url: string; repo: string };
  sizeBytes: number;
  sha256: string;
  destDir: "checkpoints" | "controlnet" | "ultralytics/bbox";
  fileName: string;
  license: { name: string; url: string; commercialUse: boolean | "see-license" };
  requiresConsent: boolean;
  note?: Localized;
}

export interface ModelFamily {
  id: string;
  category: ModelCategory;
  name: Localized;
  description: Localized;
  usedBy: string[];
  variants: ModelVariant[];
}

export interface ModelCatalog {
  schemaVersion: number;
  families: ModelFamily[];
}

export interface InstalledStatus {
  familyId: string;
  variantId: string;
  installed: boolean;
  /** ファイルサイズがカタログと一致するか(一致しなければ破損・別物の可能性)。 */
  sizeMatches: boolean;
}

export type DirSource = "setting" | "detected" | "unknown";

export interface CatalogView {
  catalog: ModelCatalog;
  installed: InstalledStatus[];
  modelsDir: string | null;
  modelsDirSource: DirSource;
  freeBytes: number | null;
}

export type DownloadStatus = "queued" | "downloading" | "verifying" | "done" | "failed" | "cancelled";

export interface DownloadState {
  downloadId: string;
  familyId: string;
  variantId: string;
  status: DownloadStatus;
  receivedBytes: number;
  totalBytes: number;
  bytesPerSec?: number;
  error?: { code: string; message: string };
}

export const modelsCatalog = () => invoke<CatalogView>("models_catalog");

export const modelsDownloadStart = (familyId: string, variantId: string, consent?: boolean) =>
  invoke<{ downloadId: string }>("models_download_start", { familyId, variantId, consent });

export const modelsDownloadCancel = (downloadId: string, discard?: boolean) =>
  invoke<void>("models_download_cancel", { downloadId, discard });

export const modelsDownloadList = () => invoke<DownloadState[]>("models_download_list");

export const modelsVerify = (familyId: string, variantId: string) =>
  invoke<{ ok: boolean }>("models_verify", { familyId, variantId });

export const modelsSetDir = (path: string | null) =>
  invoke<{ modelsDir: string | null; source: DirSource }>("models_set_dir", { path });

/** フォルダ選択ダイアログ。キャンセルなら null。 */
export async function pickModelsDir(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

/** ライセンスなどの外部ページを既定のブラウザで開く(http / https のみ)。 */
export async function openExternal(url: string): Promise<void> {
  if (!/^https:\/\//.test(url)) return;
  await openUrl(url);
}
