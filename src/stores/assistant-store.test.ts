import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultParams } from "@/lib/generation/defaults";

const api = vi.hoisted(() => ({
  assistantStatus: vi.fn(),
  assistantAsk: vi.fn(),
  assistantCancel: vi.fn(),
  assistantConversationList: vi.fn(),
  assistantConversationGet: vi.fn(),
  assistantConversationDelete: vi.fn(),
  assistantSuggestionApplied: vi.fn(),
}));
vi.mock("@/lib/tauri/assistant", () => api);

import { useAssistantStore } from "./assistant-store";

beforeEach(() => {
  Object.values(api).forEach((f) => f.mockReset());
  api.assistantConversationList.mockResolvedValue([]);
  api.assistantSuggestionApplied.mockResolvedValue(undefined);
  api.assistantAsk.mockResolvedValue({ requestId: "r1", conversationId: "c1", assistantMessageId: "m1" });
  useAssistantStore.setState({ conversationId: null, messages: [], requestId: null, error: null });
});

describe("assistant store", () => {
  it("streams an answer and a suggestion into the pending message", async () => {
    const s = useAssistantStore.getState();
    await s.send("質問", defaultParams("sd15"));
    expect(useAssistantStore.getState().messages.map((m) => m.role)).toEqual(["user", "assistant"]);
    expect(useAssistantStore.getState().conversationId).toBe("c1");

    s.onDelta("r1", "こんに");
    s.onDelta("r1", "ちは");
    s.onDelta("other", "混入しない");
    s.onSuggestion("r1", 0, "増やす", { steps: 40 });
    s.onDone("r1");

    const st = useAssistantStore.getState();
    expect(st.requestId).toBeNull();
    const a = st.messages[1];
    expect(a.text).toBe("こんにちは");
    expect(a.pending).toBe(false);
    expect(a.suggestions[0]).toEqual({ summary: "増やす", changes: { steps: 40 }, applied: false });
  });

  it("drops an empty answer and keeps the error on failure", async () => {
    await useAssistantStore.getState().send("q", defaultParams("sd15"));
    useAssistantStore.getState().onError("r1", { code: "ASSISTANT_RATE_LIMIT", message: "x" });
    const st = useAssistantStore.getState();
    expect(st.messages).toHaveLength(1);
    expect(st.error?.code).toBe("ASSISTANT_RATE_LIMIT");
    expect(st.requestId).toBeNull();
  });

  it("marks a suggestion applied locally and on the backend", async () => {
    useAssistantStore.setState({
      messages: [
        {
          id: "m1",
          role: "assistant",
          text: "a",
          suggestions: [{ summary: "s", changes: { steps: 1 }, applied: false }],
        },
      ],
    });
    await useAssistantStore.getState().markApplied("m1", 0);
    expect(useAssistantStore.getState().messages[0].suggestions[0].applied).toBe(true);
    expect(api.assistantSuggestionApplied).toHaveBeenCalledWith("m1", 0);
  });

  it("does not switch conversations while a request is running", () => {
    useAssistantStore.setState({ requestId: "r", conversationId: "c", messages: [{ id: "x", role: "user", text: "t", suggestions: [] }] });
    useAssistantStore.getState().newConversation();
    expect(useAssistantStore.getState().conversationId).toBe("c");
  });

  it("clears the open conversation when it is deleted", async () => {
    api.assistantConversationDelete.mockResolvedValue({ deleted: 1 });
    useAssistantStore.setState({ conversationId: "c", messages: [{ id: "x", role: "user", text: "t", suggestions: [] }] });
    await useAssistantStore.getState().deleteConversations(["c"]);
    expect(useAssistantStore.getState().conversationId).toBeNull();
    expect(useAssistantStore.getState().messages).toEqual([]);
  });
});
