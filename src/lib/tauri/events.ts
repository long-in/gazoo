import { isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { SuggestedChanges } from "./assistant";
import type { EngineStatus } from "./engine";
import type { ImageItem, Job, JobProgress } from "./jobs";

/** Tauri イベント購読の集約先(docs/04-ipc-api.md「イベント」)。Tauri 外では何もしない。 */
async function subscribe<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<T>(event, (e) => handler(e.payload));
}

export const onEngineStatus = (handler: (s: EngineStatus) => void) =>
  subscribe<EngineStatus>("engine://status", handler);

export const onJobUpdated = (handler: (job: Job) => void) =>
  subscribe<Job>("job://updated", handler);

export const onJobProgress = (handler: (p: JobProgress) => void) =>
  subscribe<JobProgress>("job://progress", handler);

export const onImageCreated = (handler: (image: ImageItem) => void) =>
  subscribe<ImageItem>("image://created", handler);

export const onImageDeleted = (handler: (payload: { ids: string[] }) => void) =>
  subscribe<{ ids: string[] }>("image://deleted", handler);

export const onAssistantDelta = (handler: (p: { requestId: string; text: string }) => void) =>
  subscribe<{ requestId: string; text: string }>("assistant://delta", handler);

export interface AssistantSuggestionEvent {
  requestId: string;
  index: number;
  summary: string;
  changes: SuggestedChanges;
}

export const onAssistantSuggestion = (handler: (p: AssistantSuggestionEvent) => void) =>
  subscribe<AssistantSuggestionEvent>("assistant://suggestion", handler);

export const onAssistantDone = (handler: (p: { requestId: string; cancelled: boolean }) => void) =>
  subscribe<{ requestId: string; cancelled: boolean }>("assistant://done", handler);

export const onAssistantError = (
  handler: (p: { requestId: string; code: string; message: string }) => void,
) => subscribe<{ requestId: string; code: string; message: string }>("assistant://error", handler);
