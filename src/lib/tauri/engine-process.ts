import { open } from "@tauri-apps/plugin-dialog";

import { invoke } from "./invoke";

/** ComfyUI の起動管理(docs/15、docs/04-ipc-api.md「ComfyUI の起動管理」)。型は Rust 側が正。 */

export interface ManagedEngineConfig {
  enabled: boolean;
  mainPy: string;
  python: string;
  baseDirectory: string | null;
  port: number;
  autoStart: boolean;
}

export type EngineProcStatus = "stopped" | "starting" | "running" | "stopping" | "crashed";

export interface EngineProcState {
  status: EngineProcStatus;
  pid?: number;
  exitCode?: number;
  message?: string;
  /** アプリが起動した子プロセスが存在するか。 */
  managedByApp: boolean;
}

export interface EngineSuggestions {
  mainPy?: string;
  python?: string;
  baseDirectory?: string;
}

export interface ManagedView {
  config: ManagedEngineConfig;
  state: EngineProcState;
  suggestions: EngineSuggestions;
}

export const engineManagedGet = () => invoke<ManagedView>("engine_managed_get");

export const engineManagedSet = (config: ManagedEngineConfig) =>
  invoke<ManagedView>("engine_managed_set", { config });

export const engineProcessStart = () => invoke<EngineProcState>("engine_process_start");

export const engineProcessStop = () => invoke<EngineProcState>("engine_process_stop");

/** 実行中・待機中のジョブがあり `force` でなければ `ENGINE_BUSY`(`details.running` / `details.queued`)。 */
export const engineProcessRestart = (force?: boolean) =>
  invoke<EngineProcState>("engine_process_restart", { force });

export const engineProcessLogs = (lines?: number) =>
  invoke<{ lines: string[] }>("engine_process_logs", { lines });

/** ファイル選択ダイアログ(main.py・Python)。キャンセルなら null。 */
export async function pickFile(): Promise<string | null> {
  const selected = await open({ directory: false, multiple: false });
  return typeof selected === "string" ? selected : null;
}

/** フォルダ選択ダイアログ(--base-directory)。キャンセルなら null。 */
export async function pickFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

/** 起動・停止などの操作中(状態の遷移待ち)か。 */
export const isTransient = (s: EngineProcStatus) => s === "starting" || s === "stopping";
