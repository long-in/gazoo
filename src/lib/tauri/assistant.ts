import { invoke } from "./invoke";
import type { GenerationParams } from "./jobs";

/** AI サポート(docs/11-ai-assistant.md、docs/04-ipc-api.md「AI サポート」)。キー本体は取得できない。 */
export interface AssistantStatus {
  hasKey: boolean;
  verified: boolean;
  model: string;
  consent: boolean;
  autoDelete: boolean;
}

export const ASSISTANT_MODELS = ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"] as const;
/** 保存する会話の最大数(自動削除が有効なとき)。 */
export const MAX_CONVERSATIONS = 30;

/** 提案できる項目(Rust 側で検証済み)。変えたい項目だけが入る。 */
export interface SuggestedChanges {
  prompt?: string;
  negativePrompt?: string;
  width?: number;
  height?: number;
  steps?: number;
  cfg?: number;
  guidance?: number;
  sampler?: string;
  scheduler?: string;
  batchSize?: number;
  faceDetailer?: { enabled?: boolean; denoise?: number };
}

export interface Suggestion {
  summary: string;
  changes: SuggestedChanges;
  applied: boolean;
}

export interface ConversationSummary {
  id: string;
  title: string;
  updatedAt: number;
}

export interface ConversationMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  suggestions: Suggestion[];
  createdAt: number;
}

export interface Conversation {
  id: string;
  title: string;
  messages: ConversationMessage[];
}

export interface AskStarted {
  requestId: string;
  conversationId: string;
  assistantMessageId: string;
}

export const assistantStatus = () => invoke<AssistantStatus>("assistant_status");

export const assistantSetKey = (key: string) => invoke<{ verified: boolean }>("assistant_set_key", { key });

export const assistantClearKey = () => invoke<void>("assistant_clear_key");

export const assistantUpdate = (patch: { model?: string; consent?: boolean; autoDelete?: boolean }) =>
  invoke<AssistantStatus>("assistant_update", { patch });

export const assistantAsk = (conversationId: string | null, text: string, params: GenerationParams) =>
  invoke<AskStarted>("assistant_ask", { conversationId, text, params });

export const assistantCancel = (requestId: string) => invoke<void>("assistant_cancel", { requestId });

export const assistantConversationList = () => invoke<ConversationSummary[]>("assistant_conversation_list");

export const assistantConversationGet = (id: string) => invoke<Conversation>("assistant_conversation_get", { id });

/** `ids = null` はすべて削除。 */
export const assistantConversationDelete = (ids: string[] | null) =>
  invoke<{ deleted: number }>("assistant_conversation_delete", { ids });

export const assistantSuggestionApplied = (messageId: string, index: number) =>
  invoke<void>("assistant_suggestion_applied", { messageId, index });
